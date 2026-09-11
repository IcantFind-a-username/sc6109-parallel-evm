//! Multi-version memory.
//!
//! For every state location, the value each transaction in the block wrote
//! there, keyed by transaction index. Reading as transaction `j` means taking
//! the entry with the greatest index strictly below `j`, or falling through to
//! the base snapshot — exactly the value sequential execution would have shown
//! `j`, *provided every earlier transaction's entry is final*. Validation is how
//! that proviso gets checked. See `docs/DESIGN.md` section 3.
//!
//! # Scope in M2a
//!
//! - Slot granularity only. Account granularity is refused at construction;
//!   see [`MVMemory::new`] for why.
//! - No `ESTIMATE` markers. Round-based execution never reads a location while
//!   its writer is mid-abort, so the marker has no job yet. It arrives with the
//!   collaborative scheduler in M2b.
//! - No account creation or self-destruct inside the block. Refused loudly
//!   rather than mishandled; see `docs/DESIGN.md` section 6.

use crate::outcome::{AccountSummary, StateSnapshot};
use crate::state::{existing, same_account, BaseState, StateError, StateView};
use crate::types::{Granularity, Incarnation, Key, ReadOrigin, ReadSet, TxIdx, Version};
use dashmap::DashMap;
use revm::primitives::{Address, StorageKey, StorageValue, B256, KECCAK_EMPTY, U256};
use revm::state::{AccountInfo, Bytecode, EvmState};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

/// A value stored at a location.
#[derive(Clone, Debug)]
enum Value {
    /// Balance, nonce and code hash, or `None` for an account that does not
    /// exist under EIP-161. Bytecode is stripped before storing: cloning it on
    /// every read would be expensive, and revm fetches code by hash from the
    /// base snapshot when `info.code` is absent.
    Account(Option<AccountInfo>),
    Slot(StorageValue),
}

#[derive(Clone, Debug)]
struct Entry {
    incarnation: Incarnation,
    value: Value,
}

/// Result of resolving a location for a reader.
enum Resolved {
    Base,
    Written(Version, Value),
}

impl Resolved {
    fn origin(&self) -> ReadOrigin {
        match self {
            Resolved::Base => ReadOrigin::Base,
            Resolved::Written(v, _) => ReadOrigin::Written(*v),
        }
    }
}

/// The multi-version store shared by every worker.
pub struct MVMemory {
    data: DashMap<Key, BTreeMap<TxIdx, Entry>>,
    /// Locations each transaction wrote in its latest incarnation. A
    /// re-execution that writes fewer locations must retract the rest, or
    /// readers would keep resolving to a write that no longer exists.
    last_writes: Vec<Mutex<Vec<Key>>>,
    /// Beneficiary balance increase per transaction. See D4: the beneficiary is
    /// a commutative accumulator, kept out of conflict detection entirely.
    fee_deltas: Vec<Mutex<U256>>,
    beneficiary: Address,
    beneficiary_touched: AtomicBool,
}

impl MVMemory {
    /// # Panics
    ///
    /// On [`Granularity::Account`]. The read recorder coarsens the *key* under
    /// that setting but records the *exact* location's origin, and re-resolving
    /// the coarse key does not re-check lower writers of the account. Such a
    /// validation can pass while a transaction read a value that has since
    /// changed. That is a soundness hole, not a tuning difference, and the right
    /// semantics for coarse conflict detection is an open decision. Refusing is
    /// the only honest behaviour until it is made. See E7 in
    /// `docs/AI_USAGE.md`.
    pub fn new(transactions: usize, beneficiary: Address, granularity: Granularity) -> Self {
        assert_eq!(
            granularity,
            Granularity::Slot,
            "multi-version memory supports slot granularity only; account granularity is \
             unsound for validation as currently implemented (see E7 in docs/AI_USAGE.md)"
        );
        Self {
            data: DashMap::new(),
            last_writes: (0..transactions).map(|_| Mutex::new(Vec::new())).collect(),
            fee_deltas: (0..transactions).map(|_| Mutex::new(U256::ZERO)).collect(),
            beneficiary,
            beneficiary_touched: AtomicBool::new(false),
        }
    }

    /// The latest write to `key` by a transaction strictly before `tx`.
    fn resolve(&self, key: &Key, tx: TxIdx) -> Resolved {
        let Some(versions) = self.data.get(key) else {
            return Resolved::Base;
        };
        match versions.range(..tx).next_back() {
            Some((&writer, entry)) => {
                Resolved::Written(Version::new(writer, entry.incarnation), entry.value.clone())
            }
            None => Resolved::Base,
        }
    }

    /// Records the writes of one execution, replacing that transaction's
    /// previous incarnation.
    ///
    /// New entries are inserted before stale ones are retracted. A concurrent
    /// reader can therefore see a mix of old and new — which validation then
    /// catches, because the versions it recorded will not match. Correctness
    /// never depends on the order of these two steps; only the number of
    /// spurious aborts does.
    ///
    /// A refused transaction is applied with no writes, which retracts whatever
    /// its previous incarnation wrote.
    ///
    /// # What counts as a write (D18, D19)
    ///
    /// An account is written only if its state after execution differs from
    /// the value this execution was *served* for it — not merely because revm
    /// marked it touched. A touched-but-unchanged account is a read. Treating it
    /// as a write once serialised every transaction that called the same
    /// precompile, and would have serialised every contract workload through
    /// the contract's own account (E12). Existence follows EIP-161, so a
    /// precompile call — absent before, empty after — changes nothing (E14).
    /// An account that was touched but never served, which revm does not
    /// produce, is conservatively treated as written.
    ///
    /// Returns whether this incarnation wrote a location its previous
    /// incarnation did not, which M2b uses to decide how much to revalidate.
    pub fn apply(
        &self,
        tx: TxIdx,
        incarnation: Incarnation,
        writes: Option<&EvmState>,
        served: &HashMap<Address, Option<AccountInfo>>,
        base: &BaseState,
    ) -> bool {
        let mut written = Vec::new();
        let mut fee = U256::ZERO;

        for (address, account) in writes.into_iter().flatten() {
            if !account.is_touched() {
                continue;
            }
            assert!(
                !account.is_created() && !account.is_selfdestructed(),
                "tx {tx}: account creation and self-destruct inside a block are not supported \
                 by multi-version memory yet (docs/DESIGN.md section 6)"
            );

            if *address == self.beneficiary {
                fee = self.beneficiary_delta(tx, &account.info, base);
                self.beneficiary_touched.store(true, Ordering::Relaxed);
                continue;
            }

            let after = existing(Some(account.info.clone().without_code()));
            let changed = match served.get(address) {
                Some(before) => !same_account(before.as_ref(), after.as_ref()),
                None => true,
            };
            if changed {
                self.insert(Key::Basic(*address), tx, incarnation, Value::Account(after));
                written.push(Key::Basic(*address));
            }

            for (index, slot) in account.storage.iter() {
                if slot.present_value != slot.original_value {
                    let key = Key::Storage(*address, *index);
                    self.insert(key, tx, incarnation, Value::Slot(slot.present_value));
                    written.push(key);
                }
            }
        }

        *self.fee_deltas[tx].lock().expect("fee lock") = fee;

        let stale = std::mem::replace(
            &mut *self.last_writes[tx].lock().expect("writes lock"),
            written.clone(),
        );
        let previous: BTreeSet<Key> = stale.iter().copied().collect();
        let current: BTreeSet<Key> = written.into_iter().collect();
        for key in stale {
            if !current.contains(&key) {
                if let Some(mut versions) = self.data.get_mut(&key) {
                    versions.remove(&tx);
                }
            }
        }
        !current.is_subset(&previous)
    }

    fn insert(&self, key: Key, tx: TxIdx, incarnation: Incarnation, value: Value) {
        self.data
            .entry(key)
            .or_default()
            .insert(tx, Entry { incarnation, value });
    }

    /// How much a transaction added to the beneficiary's balance.
    ///
    /// Views always serve the beneficiary from the base snapshot, so the
    /// transaction's written balance minus the base balance is exactly its
    /// contribution. The exemption is sound only while that holds — while no
    /// transaction does anything to the beneficiary but pay it. Anything else is
    /// refused rather than silently mis-merged.
    fn beneficiary_delta(&self, tx: TxIdx, written: &AccountInfo, base: &BaseState) -> U256 {
        let before = base.account(self.beneficiary).cloned().unwrap_or_default();
        assert!(
            written.nonce == before.nonce && written.code_hash == before.code_hash,
            "tx {tx} changed the beneficiary's nonce or code; the beneficiary exemption (D4) \
             is only sound when transactions merely pay it"
        );
        assert!(
            written.balance >= before.balance,
            "tx {tx} decreased the beneficiary's balance; the exemption (D4) is unsound here"
        );
        written.balance - before.balance
    }

    /// Whether every read `tx` made still resolves to the version it saw.
    ///
    /// Compares versions, not values: an unchanged version implies an unchanged
    /// value, and a changed version is treated as a conflict even when the new
    /// value happens to be equal. That costs occasional spurious aborts and buys
    /// a check that cannot be fooled.
    pub fn validate(&self, tx: TxIdx, reads: &ReadSet) -> bool {
        reads.mutable().all(|(key, recorded)| {
            if *key == Key::Basic(self.beneficiary) {
                return true;
            }
            self.resolve(key, tx).origin() == *recorded
        })
    }

    /// Final state once every transaction has validated.
    ///
    /// Takes the highest-index entry at every location — by construction the
    /// value sequential execution leaves there — and credits the beneficiary
    /// with the sum of every transaction's fee.
    pub fn snapshot(&self, base: &BaseState) -> StateSnapshot {
        let mut out = StateSnapshot::new();
        let mut final_info: BTreeMap<Address, AccountInfo> =
            base.accounts().map(|(a, i)| (*a, i.clone())).collect();
        let mut final_slots: BTreeMap<(Address, StorageKey), StorageValue> =
            base.slots().map(|(k, v)| (*k, *v)).collect();

        for item in self.data.iter() {
            let Some((_, entry)) = item.value().iter().next_back() else {
                continue;
            };
            match (item.key(), &entry.value) {
                (Key::Basic(address), Value::Account(info)) => match info {
                    Some(info) => {
                        final_info.insert(*address, info.clone());
                    }
                    None => {
                        final_info.remove(address);
                    }
                },
                (Key::Storage(address, index), Value::Slot(value)) => {
                    final_slots.insert((*address, *index), *value);
                }
                (key, _) => unreachable!("mismatched value stored at {key:?}"),
            }
        }

        if self.beneficiary_touched.load(Ordering::Relaxed) {
            let mut info = base.account(self.beneficiary).cloned().unwrap_or_default();
            let fees = self
                .fee_deltas
                .iter()
                .map(|d| *d.lock().expect("fee lock"))
                .fold(U256::ZERO, |a, b| a + b);
            info.balance += fees;
            final_info.insert(self.beneficiary, info);
        }

        for (address, info) in final_info {
            if info.is_empty() {
                continue; // EIP-161: an empty account does not exist.
            }
            out.accounts.insert(
                address,
                AccountSummary {
                    balance: info.balance,
                    nonce: info.nonce,
                    code_hash: info.code_hash,
                },
            );
        }
        for (key, value) in final_slots {
            if !value.is_zero() {
                out.storage.insert(key, value);
            }
        }
        out
    }
}

/// State as transaction `tx` sees it: every write by an earlier transaction
/// layered over the base snapshot.
pub struct MVView<'a> {
    mv: &'a MVMemory,
    base: &'a BaseState,
    tx: TxIdx,
}

impl<'a> MVView<'a> {
    pub fn new(mv: &'a MVMemory, base: &'a BaseState, tx: TxIdx) -> Self {
        Self { mv, base, tx }
    }
}

impl StateView for MVView<'_> {
    type Error = StateError;

    fn basic(&self, address: Address) -> Result<(Option<AccountInfo>, ReadOrigin), Self::Error> {
        if address == self.mv.beneficiary {
            // Always the base value: see MVMemory::beneficiary_delta.
            return self.base.basic(address);
        }
        match self.mv.resolve(&Key::Basic(address), self.tx) {
            Resolved::Base => self.base.basic(address),
            Resolved::Written(version, Value::Account(info)) => {
                Ok((info, ReadOrigin::Written(version)))
            }
            Resolved::Written(_, Value::Slot(_)) => unreachable!("slot stored at account key"),
        }
    }

    fn code_by_hash(&self, code_hash: B256) -> Result<(Bytecode, ReadOrigin), Self::Error> {
        if code_hash == KECCAK_EMPTY {
            return Ok((Bytecode::default(), ReadOrigin::Base));
        }
        self.base.code_by_hash(code_hash)
    }

    fn storage(
        &self,
        address: Address,
        index: StorageKey,
    ) -> Result<(StorageValue, ReadOrigin), Self::Error> {
        match self.mv.resolve(&Key::Storage(address, index), self.tx) {
            Resolved::Base => self.base.storage(address, index),
            Resolved::Written(version, Value::Slot(value)) => {
                Ok((value, ReadOrigin::Written(version)))
            }
            Resolved::Written(_, Value::Account(_)) => unreachable!("account stored at slot key"),
        }
    }

    fn block_hash(&self, number: u64) -> Result<(B256, ReadOrigin), Self::Error> {
        self.base.block_hash(number)
    }
}

#[cfg(test)]
mod tests {
    //! Direct tests of the store. The differential sweep exercises it end to
    //! end, but mutation testing showed that sweep never reaches retraction:
    //! a transfer's re-execution writes the same locations as before. These
    //! cases construct the shapes the workloads do not yet produce.

    use super::*;
    use revm::primitives::{address, HashMap as EvmStorageMap};
    use revm::state::{Account, EvmStorageSlot};

    const A: Address = address!("a100000000000000000000000000000000000001");
    const B: Address = address!("a100000000000000000000000000000000000002");
    const COINBASE: Address = Address::ZERO;

    fn touched(balance: u64, slots: &[(u64, u64, u64)]) -> Account {
        let mut acc = Account::from(AccountInfo {
            balance: U256::from(balance),
            ..Default::default()
        });
        acc.mark_touch();
        let mut storage = EvmStorageMap::default();
        for &(index, original, present) in slots {
            storage.insert(
                StorageKey::from(index),
                EvmStorageSlot::new_changed(
                    StorageValue::from(original),
                    StorageValue::from(present),
                    Default::default(),
                ),
            );
        }
        acc.storage = storage;
        acc
    }

    fn writes(accounts: Vec<(Address, Account)>) -> EvmState {
        accounts.into_iter().collect()
    }

    /// Nothing served: every touched account in `writes` counts as changed.
    /// The existing cases exercise versioning, not the write rule, and keep
    /// their meaning under it.
    fn served() -> HashMap<Address, Option<AccountInfo>> {
        HashMap::new()
    }

    fn info(balance: u64) -> AccountInfo {
        AccountInfo {
            balance: U256::from(balance),
            ..Default::default()
        }
    }

    fn mv(n: usize) -> MVMemory {
        MVMemory::new(n, COINBASE, Granularity::Slot)
    }

    #[test]
    fn reader_sees_latest_earlier_writer() {
        let (m, base) = (mv(4), BaseState::new());
        m.apply(
            0,
            0,
            Some(&writes(vec![(A, touched(10, &[]))])),
            &served(),
            &base,
        );
        m.apply(
            2,
            0,
            Some(&writes(vec![(A, touched(30, &[]))])),
            &served(),
            &base,
        );

        let view = MVView::new(&m, &base, 3);
        let (info, origin) = view.basic(A).unwrap();
        assert_eq!(info.unwrap().balance, U256::from(30));
        assert_eq!(origin, ReadOrigin::Written(Version::new(2, 0)));

        let view = MVView::new(&m, &base, 2);
        let (info, origin) = view.basic(A).unwrap();
        assert_eq!(
            info.unwrap().balance,
            U256::from(10),
            "a reader never sees its own write"
        );
        assert_eq!(origin, ReadOrigin::Written(Version::new(0, 0)));
    }

    /// The case mutation testing showed was unguarded: a re-execution that
    /// writes fewer locations than its previous incarnation must retract the
    /// rest, or later readers keep resolving to a write that no longer exists.
    #[test]
    fn re_execution_retracts_locations_no_longer_written() {
        let (m, base) = (mv(3), BaseState::new());
        m.apply(
            1,
            0,
            Some(&writes(vec![
                (A, touched(1, &[(7, 0, 99)])),
                (B, touched(5, &[])),
            ])),
            &served(),
            &base,
        );

        // Incarnation 1 writes only A's balance: B and slot 7 are gone.
        m.apply(
            1,
            1,
            Some(&writes(vec![(A, touched(1, &[]))])),
            &served(),
            &base,
        );

        let view = MVView::new(&m, &base, 2);
        assert_eq!(
            view.basic(B).unwrap().1,
            ReadOrigin::Base,
            "stale account write survived"
        );
        assert_eq!(
            view.storage(A, StorageKey::from(7)).unwrap(),
            (StorageValue::ZERO, ReadOrigin::Base),
            "stale slot write survived"
        );
    }

    #[test]
    fn refused_execution_retracts_everything() {
        let (m, base) = (mv(3), BaseState::new());
        m.apply(
            1,
            0,
            Some(&writes(vec![(A, touched(1, &[(7, 0, 99)]))])),
            &served(),
            &base,
        );
        m.apply(1, 1, None, &served(), &base);

        let view = MVView::new(&m, &base, 2);
        assert_eq!(view.basic(A).unwrap().1, ReadOrigin::Base);
        assert_eq!(
            view.storage(A, StorageKey::from(7)).unwrap().1,
            ReadOrigin::Base
        );
    }

    #[test]
    fn validation_fails_when_a_read_is_retracted_under_it() {
        let (m, base) = (mv(3), BaseState::new());
        m.apply(
            1,
            0,
            Some(&writes(vec![(B, touched(5, &[]))])),
            &served(),
            &base,
        );

        let mut reads = ReadSet::new();
        reads.record(Key::Basic(B), ReadOrigin::Written(Version::new(1, 0)));
        assert!(m.validate(2, &reads));

        m.apply(1, 1, None, &served(), &base);
        assert!(
            !m.validate(2, &reads),
            "a read of a retracted write must fail validation"
        );
    }

    #[test]
    fn validation_fails_on_new_incarnation_even_with_equal_value() {
        let (m, base) = (mv(3), BaseState::new());
        m.apply(
            0,
            0,
            Some(&writes(vec![(A, touched(10, &[]))])),
            &served(),
            &base,
        );
        let mut reads = ReadSet::new();
        reads.record(Key::Basic(A), ReadOrigin::Written(Version::new(0, 0)));

        m.apply(
            0,
            1,
            Some(&writes(vec![(A, touched(10, &[]))])),
            &served(),
            &base,
        );
        assert!(
            !m.validate(1, &reads),
            "versions, not values, are compared: a new incarnation invalidates its readers"
        );
    }

    #[test]
    fn unchanged_slots_are_not_registered_as_writes() {
        let (m, base) = (mv(2), BaseState::new());
        m.apply(
            0,
            0,
            Some(&writes(vec![(A, touched(1, &[(3, 8, 8)]))])),
            &served(),
            &base,
        );
        let view = MVView::new(&m, &base, 1);
        assert_eq!(
            view.storage(A, StorageKey::from(3)).unwrap().1,
            ReadOrigin::Base
        );
    }

    #[test]
    fn beneficiary_is_served_from_base_and_fees_sum() {
        let (m, base) = (mv(3), BaseState::new());
        m.apply(
            0,
            0,
            Some(&writes(vec![(COINBASE, touched(7, &[]))])),
            &served(),
            &base,
        );
        m.apply(
            1,
            0,
            Some(&writes(vec![(COINBASE, touched(5, &[]))])),
            &served(),
            &base,
        );

        let view = MVView::new(&m, &base, 2);
        assert_eq!(view.basic(COINBASE).unwrap().1, ReadOrigin::Base);

        let snap = m.snapshot(&base);
        assert_eq!(snap.accounts[&COINBASE].balance, U256::from(12));
    }

    #[test]
    #[should_panic(expected = "decreased the beneficiary's balance")]
    fn beneficiary_exemption_refuses_debits() {
        let mut base = BaseState::new();
        base.insert_account(
            COINBASE,
            AccountInfo {
                balance: U256::from(100),
                ..Default::default()
            },
        );
        let m = mv(1);
        m.apply(
            0,
            0,
            Some(&writes(vec![(COINBASE, touched(50, &[]))])),
            &served(),
            &base,
        );
    }

    /// E12's rule, directly: served a value, left with the same value, is a
    /// read and not a write.
    #[test]
    fn touched_but_unchanged_account_is_not_a_write() {
        let (m, base) = (mv(3), BaseState::new());
        let served = HashMap::from([(A, Some(info(10)))]);
        let wrote_new = m.apply(
            0,
            0,
            Some(&writes(vec![(A, touched(10, &[]))])),
            &served,
            &base,
        );
        assert!(!wrote_new);
        assert_eq!(
            MVView::new(&m, &base, 1).basic(A).unwrap().1,
            ReadOrigin::Base
        );
    }

    #[test]
    fn changed_account_is_a_write() {
        let (m, base) = (mv(3), BaseState::new());
        let served = HashMap::from([(A, Some(info(10)))]);
        m.apply(
            0,
            0,
            Some(&writes(vec![(A, touched(7, &[]))])),
            &served,
            &base,
        );
        let (value, origin) = MVView::new(&m, &base, 1).basic(A).unwrap();
        assert_eq!(value.unwrap().balance, U256::from(7));
        assert_eq!(origin, ReadOrigin::Written(Version::new(0, 0)));
    }

    /// The original scene of E12 and E14: a call to the sha256 precompile. Its
    /// account is absent before and empty after, which under EIP-161 is the
    /// same state. It must not become a write.
    #[test]
    fn precompile_call_is_not_a_write() {
        let sha256 = Address::with_last_byte(2);
        let (m, base) = (mv(3), BaseState::new());
        let served = HashMap::from([(sha256, None)]);
        let wrote_new = m.apply(
            0,
            0,
            Some(&writes(vec![(sha256, touched(0, &[]))])),
            &served,
            &base,
        );
        assert!(!wrote_new, "an empty touched account is not a new location");
        assert_eq!(
            MVView::new(&m, &base, 1).basic(sha256).unwrap().1,
            ReadOrigin::Base
        );
        assert!(
            !m.snapshot(&base).accounts.contains_key(&sha256),
            "EIP-161: empty accounts do not exist"
        );
    }

    #[test]
    fn apply_reports_new_locations() {
        let (m, base) = (mv(2), BaseState::new());
        assert!(m.apply(
            0,
            0,
            Some(&writes(vec![(A, touched(1, &[]))])),
            &served(),
            &base
        ));
        assert!(
            !m.apply(
                0,
                1,
                Some(&writes(vec![(A, touched(2, &[]))])),
                &served(),
                &base
            ),
            "same location set"
        );
        assert!(m.apply(
            0,
            2,
            Some(&writes(vec![(A, touched(3, &[])), (B, touched(1, &[]))])),
            &served(),
            &base
        ));
        assert!(
            !m.apply(
                0,
                3,
                Some(&writes(vec![(A, touched(3, &[]))])),
                &served(),
                &base
            ),
            "shrinking is not new"
        );
    }
}
