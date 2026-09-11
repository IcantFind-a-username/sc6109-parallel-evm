//! Sequential baseline state.
//!
//! Deliberately the dumbest thing that works: two hash maps and no versioning.
//!
//! # Why this duplicates `MVMemory`
//!
//! Driving the multi-version store single-threaded in index order produces
//! sequential semantics for free, and reusing it would save this file. That
//! would also destroy the differential test. A bug in shared machinery corrupts
//! both sides of the comparison identically, the two agree, and the test
//! reports success while the engine is wrong.
//!
//! Differential testing has power only when the implementations are
//! independent. These hundred lines are what make the M1 and M2 gates mean
//! something. See D8 in `DECISIONS.md`.

use super::{BaseState, StateError, StateView};
use crate::types::ReadOrigin;
use revm::primitives::{Address, HashMap, StorageKey, StorageValue, B256};
use revm::state::{Account, AccountInfo, Bytecode, EvmState};

/// Mutable state for sequential execution: the base snapshot plus whatever
/// transactions have committed so far.
#[derive(Clone, Debug)]
pub struct SimpleState {
    base: BaseState,
    accounts: HashMap<Address, AccountInfo>,
    storage: HashMap<(Address, StorageKey), StorageValue>,
    destroyed: Vec<Address>,
}

impl SimpleState {
    pub fn new(base: BaseState) -> Self {
        Self {
            base,
            accounts: HashMap::default(),
            storage: HashMap::default(),
            destroyed: Vec::new(),
        }
    }

    /// A read-only view for the next transaction.
    pub fn view(&self) -> SimpleView<'_> {
        SimpleView { state: self }
    }

    /// Applies the output of one transaction.
    ///
    /// Honours revm's account status flags: an account marked selfdestructed
    /// loses its storage, and one marked created starts from empty storage
    /// rather than inheriting whatever the snapshot held at that address.
    pub fn commit(&mut self, changes: EvmState) {
        for (address, account) in changes {
            if !account.is_touched() {
                continue;
            }
            if account.is_selfdestructed() {
                self.destroy(address);
                continue;
            }
            if account.is_created() {
                self.clear_storage(address);
            }
            self.apply_account(address, &account);
        }
    }

    fn apply_account(&mut self, address: Address, account: &Account) {
        self.accounts.insert(address, account.info.clone());
        for (index, slot) in account.storage.iter() {
            self.storage.insert((address, *index), slot.present_value());
        }
    }

    fn destroy(&mut self, address: Address) {
        self.accounts.insert(address, AccountInfo::default());
        self.clear_storage(address);
        self.destroyed.push(address);
    }

    fn clear_storage(&mut self, address: Address) {
        self.storage.retain(|(addr, _), _| *addr != address);
    }

    /// An account as it currently exists. Under EIP-161 an empty account does
    /// not: a transfer of nothing to a nonexistent address, or a call to a
    /// precompile, leaves a touched empty account that mainnet deletes (D19).
    pub fn account(&self, address: Address) -> Option<AccountInfo> {
        let info = match self.accounts.get(&address) {
            Some(written) => Some(written.clone()),
            None => self.base.account(address).cloned(),
        };
        super::existing(info)
    }

    pub fn slot(&self, address: Address, index: StorageKey) -> StorageValue {
        if self.destroyed.contains(&address) && !self.storage.contains_key(&(address, index)) {
            return StorageValue::default();
        }
        self.storage
            .get(&(address, index))
            .copied()
            .unwrap_or_else(|| self.base.slot(address, index))
    }

    pub fn base(&self) -> &BaseState {
        &self.base
    }

    /// Accounts this block has written. Used to build a snapshot that covers
    /// the same address space as any other engine's.
    pub fn touched_accounts(&self) -> impl Iterator<Item = Address> + '_ {
        self.accounts.keys().copied()
    }

    /// Every slot either present in the base snapshot or written by this block.
    pub fn touched_slots(&self) -> Vec<(Address, StorageKey)> {
        let mut keys: Vec<(Address, StorageKey)> = self
            .base
            .slots()
            .map(|(k, _)| *k)
            .chain(self.storage.keys().copied())
            .collect();
        keys.sort_unstable();
        keys.dedup();
        keys
    }
}

/// A borrowed, read-only view of [`SimpleState`].
///
/// Every read reports [`ReadOrigin::Base`]: sequential execution has no
/// versions to distinguish, and its read sets are never validated. They are
/// still captured, so the recorder is exercised on this path too.
#[derive(Clone, Copy, Debug)]
pub struct SimpleView<'a> {
    state: &'a SimpleState,
}

impl StateView for SimpleView<'_> {
    type Error = StateError;

    fn basic(&self, address: Address) -> Result<(Option<AccountInfo>, ReadOrigin), Self::Error> {
        Ok((self.state.account(address), ReadOrigin::Base))
    }

    fn code_by_hash(&self, code_hash: B256) -> Result<(Bytecode, ReadOrigin), Self::Error> {
        self.state.base.code_by_hash(code_hash)
    }

    fn storage(
        &self,
        address: Address,
        index: StorageKey,
    ) -> Result<(StorageValue, ReadOrigin), Self::Error> {
        Ok((self.state.slot(address, index), ReadOrigin::Base))
    }

    fn block_hash(&self, number: u64) -> Result<(B256, ReadOrigin), Self::Error> {
        self.state.base.block_hash(number)
    }
}
