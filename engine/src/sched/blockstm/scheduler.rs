//! Block-STM with real execution (M2b, step 3).
//!
//! Workers pull tasks from the [`Coordinator`]; an execution task runs the
//! transaction through the shared execution path against the multi-version
//! store, and a validation task re-checks its read set. Three ways an
//! execution can end, each with its own route:
//!
//! - **Completed** — writes applied to the store, read set kept for
//!   validation.
//! - **Refused by the EVM** — kept as an outcome with no writes, read set and
//!   all (E6). Under speculation a refusal is usually a stale read, such as a
//!   sender's second transaction reading its nonce before the first wrote it
//!   back; validation will see the read changed and re-run it. One that
//!   survives validation is fatal, since sequential execution would refuse it
//!   too.
//! - **Blocked** — a read hit an `ESTIMATE`. The transaction is parked on the
//!   writer and resumed when the writer next finishes (see the coordinator).
//!
//! On a validation abort the transaction's writes are marked as estimates
//! before it is re-queued, so later readers wait for the rewrite instead of
//! reading values about to change.

use super::coordinator::{Coordinator, Execution};
use crate::exec::execute;
use crate::mv::{MVMemory, MVView};
use crate::outcome::{BlockOutcome, ExecStats};
use crate::sched::{assert_beneficiary_only_paid, Scheduler, SchedulerConfig};
use crate::state::{BaseState, StateError};
use crate::types::ReadSet;
use revm::context::result::EVMError;
use revm::context::TxEnv;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering::Relaxed};
use std::sync::Mutex;
use std::time::Instant;

/// Collaborative Block-STM.
#[derive(Clone, Copy, Debug, Default)]
pub struct BlockStmScheduler;

impl BlockStmScheduler {
    pub fn new() -> Self {
        Self
    }
}

/// Total executions after which the scheduler gives up and panics.
///
/// A backstop, not a proven bound. Unlike the round-based scheduler, whose
/// round count is provably at most `n`, there is no simple constant bounding
/// Block-STM's incarnations (coordinator module docs). A store inconsistency
/// would otherwise show up as a benchmark that never ends — the least
/// informative failure there is (E8). Quadratic is far above anything a
/// correct run approaches: even a fully serial block re-executes each
/// transaction a small number of times.
fn execution_limit(n: usize) -> usize {
    n.saturating_mul(n).saturating_add(16 * n).max(1_024)
}

impl Scheduler for BlockStmScheduler {
    fn name(&self) -> &'static str {
        "blockstm"
    }

    fn execute_block(
        &self,
        txs: &[TxEnv],
        base: &BaseState,
        config: &SchedulerConfig,
    ) -> BlockOutcome {
        let n = txs.len();
        let beneficiary = config.block.beneficiary;
        assert_beneficiary_only_paid(txs, beneficiary);

        let mv = MVMemory::new(n, beneficiary, config.granularity);
        let coordinator = Coordinator::new(n);
        let reads: Vec<Mutex<ReadSet>> = (0..n).map(|_| Mutex::new(ReadSet::new())).collect();
        let reverted: Vec<AtomicBool> = (0..n).map(|_| AtomicBool::new(false)).collect();
        let refused: Vec<Mutex<Option<String>>> = (0..n).map(|_| Mutex::new(None)).collect();
        let per_tx: Vec<AtomicU32> = (0..n).map(|_| AtomicU32::new(0)).collect();
        let (executions, aborts, waits, refusals) = (
            AtomicUsize::new(0),
            AtomicUsize::new(0),
            AtomicUsize::new(0),
            AtomicUsize::new(0),
        );
        let limit = execution_limit(n);

        let start = Instant::now();
        std::thread::scope(|scope| {
            for _ in 0..config.threads {
                scope.spawn(|| {
                    coordinator.run_worker(
                        |v| {
                            let view = MVView::new(&mv, base, v.tx);
                            let out =
                                execute(view, txs[v.tx].clone(), &config.block, config.granularity);
                            let wrote_new_location = match out.outcome {
                                Err(EVMError::Database(StateError::Blocked { on })) => {
                                    waits.fetch_add(1, Relaxed);
                                    return Execution::Blocked { on };
                                }
                                Ok(done) => {
                                    reverted[v.tx].store(!done.is_success(), Relaxed);
                                    *refused[v.tx].lock().expect("refused lock") = None;
                                    mv.apply(
                                        v.tx,
                                        v.incarnation,
                                        Some(&done.writes),
                                        &out.served,
                                        base,
                                    )
                                }
                                Err(err) => {
                                    refusals.fetch_add(1, Relaxed);
                                    *refused[v.tx].lock().expect("refused lock") =
                                        Some(format!("{err:?}"));
                                    mv.apply(v.tx, v.incarnation, None, &out.served, base)
                                }
                            };
                            // Stored before the coordinator marks the transaction
                            // executed, so no validation can see a stale read set.
                            *reads[v.tx].lock().expect("reads lock") = out.reads;
                            per_tx[v.tx].fetch_add(1, Relaxed);
                            let total = executions.fetch_add(1, Relaxed) + 1;
                            assert!(
                                total <= limit,
                                "{total} executions for a block of {n}: past the backstop, so the \
                                 store or the coordinator is inconsistent"
                            );
                            Execution::Done { wrote_new_location }
                        },
                        |v| mv.validate(v.tx, &reads[v.tx].lock().expect("reads lock")),
                        |v| {
                            aborts.fetch_add(1, Relaxed);
                            mv.mark_estimates(v.tx);
                        },
                    );
                });
            }
        });
        let wall_clock = start.elapsed();

        for (i, r) in refused.iter().enumerate() {
            if let Some(err) = r.lock().expect("refused lock").as_ref() {
                panic!("transaction {i} could not be executed: {err}");
            }
        }

        let stats = ExecStats {
            transactions: n,
            executions: executions.into_inner(),
            aborts: aborts.into_inner(),
            dependency_waits: waits.into_inner(),
            speculative_refusals: refusals.into_inner(),
            reverted: reverted.iter().filter(|r| r.load(Relaxed)).count(),
            threads: config.threads,
            rounds: 0,
            wall_clock,
            preparation: std::time::Duration::ZERO,
            executions_per_tx: per_tx.into_iter().map(AtomicU32::into_inner).collect(),
        };
        BlockOutcome {
            state: mv.snapshot(base),
            stats,
        }
    }
}
