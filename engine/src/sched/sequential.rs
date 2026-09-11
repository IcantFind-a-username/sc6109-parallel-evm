//! Sequential baseline.
//!
//! Executes transactions one at a time, in index order, committing each before
//! starting the next. This is the definition of correct: every other scheduler
//! must produce exactly this state.
//!
//! It is also the denominator of every speedup number in the report, so it must
//! be honest — no shortcuts that the parallel path does not also take. It goes
//! through the same [`crate::exec::execute`] function and the same
//! [`ReadRecorder`](crate::state::ReadRecorder), including capturing read sets
//! it has no use for, so that per-transaction overhead is comparable.

use super::{Scheduler, SchedulerConfig};
use crate::exec::execute;
use crate::outcome::{AccountSummary, BlockOutcome, ExecStats, StateSnapshot};
use crate::state::{BaseState, SimpleState};
use revm::context::TxEnv;
use revm::primitives::KECCAK_EMPTY;
use std::time::Instant;

/// Single-threaded, in-order execution.
#[derive(Clone, Copy, Debug, Default)]
pub struct SequentialScheduler;

impl SequentialScheduler {
    pub fn new() -> Self {
        Self
    }
}

impl Scheduler for SequentialScheduler {
    fn name(&self) -> &'static str {
        "sequential"
    }

    fn execute_block(
        &self,
        txs: &[TxEnv],
        base: &BaseState,
        config: &SchedulerConfig,
    ) -> BlockOutcome {
        let mut state = SimpleState::new(base.clone());
        let mut stats = ExecStats {
            transactions: txs.len(),
            threads: 1,
            rounds: 1,
            executions_per_tx: vec![0; txs.len()],
            ..Default::default()
        };

        let start = Instant::now();
        for (idx, tx) in txs.iter().enumerate() {
            let executed = execute(state.view(), tx.clone(), &config.block, config.granularity);
            stats.executions += 1;
            stats.executions_per_tx[idx] = 1;

            match executed.outcome {
                Ok(out) => {
                    if !out.is_success() {
                        stats.reverted += 1;
                    }
                    state.commit(out.writes);
                }
                Err(err) => {
                    // The EVM refused the transaction outright. In a generated
                    // workload this means the workload is malformed, most often
                    // a nonce that does not match the sender's state. Loud
                    // rather than silent: a skipped transaction would make
                    // every downstream comparison meaningless.
                    panic!("transaction {idx} could not be executed: {err:?}");
                }
            }
        }
        stats.wall_clock = start.elapsed();

        BlockOutcome {
            state: snapshot(&state),
            stats,
        }
    }
}

/// Materialises a comparable snapshot from sequential state.
///
/// Accounts that exist only in the base snapshot and were never touched are
/// included, so that two snapshots taken from different engines cover the same
/// address space and a missing write shows up as a difference rather than as an
/// absent key on both sides.
pub fn snapshot(state: &SimpleState) -> StateSnapshot {
    let mut out = StateSnapshot::new();

    let addresses: std::collections::BTreeSet<_> = state
        .base()
        .accounts()
        .map(|(a, _)| *a)
        .chain(state.touched_accounts())
        .collect();

    for address in addresses {
        if let Some(info) = state.account(address) {
            out.accounts.insert(
                address,
                AccountSummary {
                    balance: info.balance,
                    nonce: info.nonce,
                    code_hash: if info.code_hash == KECCAK_EMPTY {
                        KECCAK_EMPTY
                    } else {
                        info.code_hash
                    },
                },
            );
        }
    }

    for (address, index) in state.touched_slots() {
        let value = state.slot(address, index);
        if !value.is_zero() {
            out.storage.insert((address, index), value);
        }
    }

    out
}
