//! Dependency structure of a workload, measured rather than assumed.
//!
//! D17 puts measured dependency density on Figure 2's x-axis because generator
//! parameters are not a comparable scale for conflict (E9). This module computes
//! it — and the critical path — from the read and write sets of an actual
//! sequential execution, so it applies unchanged to any workload, including
//! contract workloads with no generator knob at all.

use crate::exec::execute;
use crate::state::SimpleState;
use crate::types::{Granularity, Key, TxIdx};
use crate::workload::Workload;
use revm::context::BlockEnv;
use std::collections::HashMap;

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

/// Executes the workload sequentially and measures its dependency structure.
///
/// Keys are slot-granular, matching the multi-version store. The block
/// beneficiary is excluded, as it is from conflict detection (D4): counting it
/// would make every transaction depend on its predecessor.
///
/// Write-after-write needs no separate treatment: the EVM loads an account or
/// slot before modifying it, so every write is preceded by a read of the same
/// location, and read-after-write captures the chain.
///
/// # Panics
///
/// If the EVM refuses a transaction — the same condition under which the
/// sequential scheduler panics, since the workload is then malformed.
pub fn analyse(workload: &Workload, block: &BlockEnv) -> DependencyProfile {
    let beneficiary = Key::Basic(block.beneficiary);
    let mut state = SimpleState::new(workload.base.clone());
    let mut last_writer: HashMap<Key, TxIdx> = HashMap::new();
    let mut depth = vec![0usize; workload.txs.len()];
    let mut dependent = 0;

    for (j, tx) in workload.txs.iter().enumerate() {
        let out = execute(state.view(), tx.clone(), block, Granularity::Slot);
        let done = out
            .outcome
            .unwrap_or_else(|e| panic!("transaction {j} could not be executed: {e:?}"));

        let deepest = out
            .reads
            .mutable()
            .filter(|(k, _)| *k != beneficiary)
            .filter_map(|(k, _)| last_writer.get(k))
            .map(|&i| depth[i])
            .max();
        if deepest.is_some() {
            dependent += 1;
        }
        depth[j] = 1 + deepest.unwrap_or(0);

        for (address, account) in done.writes.iter() {
            if !account.is_touched() || *address == block.beneficiary {
                continue;
            }
            last_writer.insert(Key::Basic(*address), j);
            for (index, slot) in account.storage.iter() {
                if slot.present_value != slot.original_value {
                    last_writer.insert(Key::Storage(*address, *index), j);
                }
            }
        }
        state.commit(done.writes);
    }

    DependencyProfile {
        transactions: workload.txs.len(),
        dependent,
        critical_path: depth.iter().copied().max().unwrap_or(0),
    }
}
