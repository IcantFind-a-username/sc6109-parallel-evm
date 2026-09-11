//! Dependency structure of a workload, measured rather than assumed.
//!
//! Everything here comes from one sequential execution with the engine's own
//! read recorder — exactly what a block builder does under EIP-7928 — so it
//! describes the real read and write sets rather than a generator's intent,
//! and applies unchanged to contract workloads with no generator knob at all.
//!
//! Two consumers:
//!
//! - [`analyse`] turns the access sets into Figure 2's x-axis (D17):
//!   dependency density and critical path.
//! - The static scheduler takes them as its declared access sets, in the role
//!   of an EIP-7928 block access list (DESIGN §7).

use crate::exec::execute;
use crate::state::{existing, same_account, SimpleState};
use crate::types::{Granularity, Key, TxIdx};
use revm::context::{BlockEnv, TxEnv};
use revm::primitives::Address;
use revm::state::{AccountInfo, EvmState};
use std::collections::{BTreeSet, HashMap};

use crate::state::BaseState;
use crate::workload::Workload;

/// The locations one transaction read and wrote, beneficiary excluded (D4).
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct AccessSet {
    pub reads: BTreeSet<Key>,
    pub writes: BTreeSet<Key>,
}

/// How a block's transactions depend on each other.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct DependencyProfile {
    pub transactions: usize,
    /// Transactions that read at least one location an earlier transaction in
    /// the block wrote.
    pub dependent: usize,
    /// Longest chain of read-after-write dependencies, counted in transactions.
    /// Zero for an empty block.
    pub critical_path: usize,
}

impl DependencyProfile {
    /// Fraction of transactions with at least one read-after-write dependency.
    pub fn density(&self) -> f64 {
        if self.transactions == 0 {
            return 0.0;
        }
        self.dependent as f64 / self.transactions as f64
    }

    /// Upper bound on speedup from dependencies alone: block size over critical
    /// path. Ignores every overhead, so real schedulers sit well below it.
    pub fn parallelism_ceiling(&self) -> f64 {
        if self.critical_path == 0 {
            return 0.0;
        }
        self.transactions as f64 / self.critical_path as f64
    }
}

/// The locations a transaction's output writes, by the store's rule (D18,
/// D19): an account only if its state changed from `before`, a slot only if
/// its value changed. Keys are coarsened to the account under account
/// granularity, and the beneficiary is left out.
pub fn write_keys(
    writes: &EvmState,
    before: impl Fn(Address) -> Option<AccountInfo>,
    beneficiary: Address,
    granularity: Granularity,
) -> BTreeSet<Key> {
    let mut out = BTreeSet::new();
    for (address, account) in writes.iter() {
        if !account.is_touched() || *address == beneficiary {
            continue;
        }
        let after = existing(Some(account.info.clone()));
        if !same_account(before(*address).as_ref(), after.as_ref()) {
            out.insert(Key::Basic(*address));
        }
        for (index, slot) in account.storage.iter() {
            if slot.present_value != slot.original_value {
                out.insert(Key::storage(*address, *index, granularity));
            }
        }
    }
    out
}

/// Executes a block sequentially and records every transaction's access set,
/// at the given granularity.
///
/// Sequential, not each transaction against pre-block state: a sender's second
/// transaction run against pre-block state would be refused for its nonce and
/// its real access set lost. A builder derives the list while building the
/// block, which is sequential by definition.
///
/// # Panics
///
/// If the EVM refuses a transaction — the workload is then malformed, as it is
/// for the sequential scheduler.
pub fn profile(
    txs: &[TxEnv],
    base: &BaseState,
    block: &BlockEnv,
    granularity: Granularity,
) -> Vec<AccessSet> {
    let beneficiary = Key::Basic(block.beneficiary);
    let mut state = SimpleState::new(base.clone());
    let mut sets = Vec::with_capacity(txs.len());
    for (j, tx) in txs.iter().enumerate() {
        let out = execute(state.view(), tx.clone(), block, granularity);
        let done = out
            .outcome
            .unwrap_or_else(|e| panic!("transaction {j} could not be executed: {e:?}"));
        let reads = out
            .reads
            .mutable()
            .map(|(k, _)| *k)
            .filter(|k| *k != beneficiary)
            .collect();
        let writes = write_keys(
            &done.writes,
            |a| state.account(a),
            block.beneficiary,
            granularity,
        );
        sets.push(AccessSet { reads, writes });
        state.commit(done.writes);
    }
    sets
}

/// Measures a workload's dependency structure at slot granularity.
///
/// Write-after-write needs no separate treatment: the EVM loads an account or
/// slot before modifying it, so every write is preceded by a read of the same
/// location, and read-after-write captures the chain. What counts as a write is
/// exactly what the multi-version store counts; Figure 2's x-axis and the
/// scheduler under test must never disagree on it — they did once (E12).
pub fn analyse(workload: &Workload, block: &BlockEnv) -> DependencyProfile {
    let sets = profile(&workload.txs, &workload.base, block, Granularity::Slot);
    let mut last_writer: HashMap<Key, TxIdx> = HashMap::new();
    let mut depth = vec![0usize; sets.len()];
    let mut dependent = 0;
    for (j, set) in sets.iter().enumerate() {
        let deepest = set
            .reads
            .iter()
            .filter_map(|k| last_writer.get(k))
            .map(|&i| depth[i])
            .max();
        if deepest.is_some() {
            dependent += 1;
        }
        depth[j] = 1 + deepest.unwrap_or(0);
        for k in &set.writes {
            last_writer.insert(*k, j);
        }
    }
    DependencyProfile {
        transactions: sets.len(),
        dependent,
        critical_path: depth.iter().copied().max().unwrap_or(0),
    }
}
