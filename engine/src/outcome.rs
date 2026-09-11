//! What executing a block produces.
//!
//! Every scheduler returns the same two things: a canonical snapshot of final
//! state, and statistics about how it got there. The snapshot is what the
//! differential test compares; the statistics are what the report plots.

use revm::primitives::{Address, StorageKey, StorageValue, B256, U256};
use std::collections::BTreeMap;
use std::time::Duration;

/// A comparable summary of one account.
///
/// Code is compared by hash rather than by bytes: two accounts with the same
/// code hash necessarily hold the same code, and carrying bytecode around would
/// make snapshots large and slow to diff.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct AccountSummary {
    pub balance: U256,
    pub nonce: u64,
    pub code_hash: B256,
}

/// Canonical final state of a block.
///
/// `BTreeMap` so that iteration order is deterministic and two snapshots can be
/// diffed by a single ordered walk. Determinism here is not cosmetic: a
/// differential failure has to be reproducible to be debuggable.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct StateSnapshot {
    pub accounts: BTreeMap<Address, AccountSummary>,
    pub storage: BTreeMap<(Address, StorageKey), StorageValue>,
}

impl StateSnapshot {
    pub fn new() -> Self {
        Self::default()
    }

    /// Human-readable differences against another snapshot.
    ///
    /// Empty means the two agree. A differential test that merely reports
    /// "states differ" costs hours; one that names the account and the slot
    /// costs minutes.
    pub fn diff(&self, other: &Self) -> Vec<String> {
        let mut out = Vec::new();

        let addrs: std::collections::BTreeSet<&Address> =
            self.accounts.keys().chain(other.accounts.keys()).collect();
        for addr in addrs {
            match (self.accounts.get(addr), other.accounts.get(addr)) {
                (Some(a), Some(b)) if a != b => {
                    if a.balance != b.balance {
                        out.push(format!("{addr}: balance {} vs {}", a.balance, b.balance));
                    }
                    if a.nonce != b.nonce {
                        out.push(format!("{addr}: nonce {} vs {}", a.nonce, b.nonce));
                    }
                    if a.code_hash != b.code_hash {
                        out.push(format!("{addr}: code {} vs {}", a.code_hash, b.code_hash));
                    }
                }
                (Some(_), None) => out.push(format!("{addr}: present on left, absent on right")),
                (None, Some(_)) => out.push(format!("{addr}: absent on left, present on right")),
                _ => {}
            }
        }

        let slots: std::collections::BTreeSet<&(Address, StorageKey)> =
            self.storage.keys().chain(other.storage.keys()).collect();
        for key in slots {
            let (l, r) = (self.storage.get(key), other.storage.get(key));
            if l != r {
                let (addr, index) = key;
                out.push(format!(
                    "{addr} slot {index}: {} vs {}",
                    l.copied().unwrap_or_default(),
                    r.copied().unwrap_or_default()
                ));
            }
        }

        out
    }
}

/// How a block was executed. Feeds every figure in the report.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct ExecStats {
    /// Transactions in the block.
    pub transactions: usize,
    /// Total executions performed, counting re-executions after aborts. Equals
    /// `transactions` when nothing aborted.
    pub executions: usize,
    /// Executions discarded because validation failed.
    pub aborts: usize,
    /// Transactions that reverted or halted. A reverted transaction is a
    /// successful execution with a failed outcome, not an abort.
    pub reverted: usize,
    /// Executions that stopped at an `ESTIMATE` and were parked (Block-STM
    /// only). Not counted in `executions`.
    pub dependency_waits: usize,
    /// Executions the EVM refused mid-flight because of a stale read — E6's
    /// case — and that validation then sent back. Zero for sequential.
    pub speculative_refusals: usize,
    /// Threads the scheduler was configured with.
    pub threads: usize,
    /// Execute-then-validate passes over the block. Always 1 for sequential;
    /// for round-based optimistic execution this is bounded by the length of
    /// the longest dependency chain in the block.
    pub rounds: usize,
    /// Wall-clock time for the whole block.
    pub wall_clock: Duration,
    /// Executions per transaction index, for the abort-distribution figure.
    pub executions_per_tx: Vec<u32>,
}

impl ExecStats {
    /// Aborts divided by total executions. Zero when nothing ran.
    pub fn abort_rate(&self) -> f64 {
        if self.executions == 0 {
            return 0.0;
        }
        self.aborts as f64 / self.executions as f64
    }

    /// Executions per transaction. 1.0 means nothing was ever re-executed.
    pub fn execution_factor(&self) -> f64 {
        if self.transactions == 0 {
            return 0.0;
        }
        self.executions as f64 / self.transactions as f64
    }

    pub fn throughput_tps(&self) -> f64 {
        let secs = self.wall_clock.as_secs_f64();
        if secs <= 0.0 {
            return 0.0;
        }
        self.transactions as f64 / secs
    }
}

/// The result of executing one block.
#[derive(Clone, Debug)]
pub struct BlockOutcome {
    pub state: StateSnapshot,
    pub stats: ExecStats,
}
