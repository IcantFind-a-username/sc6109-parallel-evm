//! Round-based optimistic execution (M2a).
//!
//! ```text
//! pending = every transaction
//! loop:
//!     execute every pending transaction in parallel against MVMemory
//!     validate every transaction at or after the lowest pending index
//!     pending = those that failed validation
//!     stop when pending is empty
//! ```
//!
//! # Why it terminates, and why it is correct
//!
//! Execution and validation never overlap: validation runs only after every
//! execution in the round has finished and written. It therefore checks a
//! stable store, which is what makes this variant easy to reason about.
//!
//! A transaction reads only locations written by transactions *before* it. So
//! the lowest-index transaction that fails validation in a round has every
//! predecessor already valid and final; when it re-executes in the next round
//! it reads only final values and must pass. At least one transaction becomes
//! permanently valid per round, so the loop ends within `n` rounds — and when it
//! does, every transaction's reads resolve to final versions, which is the
//! state sequential execution produces. See `docs/DESIGN.md` section 4.
//!
//! # The cost of that simplicity
//!
//! On a block that is one long dependency chain — every NFT mint writing
//! `totalSupply`, every swap hitting one pool — each round confirms exactly one
//! more transaction and re-executes everything after it. Total work is
//! quadratic in the chain length. That is the case M2b's collaborative
//! scheduler and dependency tracking exist to fix, and the gap between the two
//! on a hot-slot workload is itself a result worth reporting.

use super::{Scheduler, SchedulerConfig};
use crate::exec::execute;
use crate::mv::{MVMemory, MVView};
use crate::outcome::{BlockOutcome, ExecStats};
use crate::state::BaseState;
use crate::types::{Incarnation, ReadSet, TxIdx};
use rayon::prelude::*;
use revm::context::TxEnv;
use std::time::Instant;

/// Round-based optimistic scheduler.
#[derive(Clone, Copy, Debug, Default)]
pub struct RoundScheduler;

impl RoundScheduler {
    pub fn new() -> Self {
        Self
    }
}

/// What the latest execution of one transaction left behind.
#[derive(Default)]
struct TxState {
    incarnation: Incarnation,
    reads: ReadSet,
    reverted: bool,
    /// The EVM refused the transaction. Normal mid-flight — a stale nonce read
    /// produces exactly this — but fatal if it survives validation, since then
    /// sequential execution would refuse it too.
    refused: Option<String>,
}

impl Scheduler for RoundScheduler {
    fn name(&self) -> &'static str {
        "blockstm-rounds"
    }

    fn execute_block(
        &self,
        txs: &[TxEnv],
        base: &BaseState,
        config: &SchedulerConfig,
    ) -> BlockOutcome {
        let n = txs.len();
        let beneficiary = config.block.beneficiary;
        super::assert_beneficiary_only_paid(txs, beneficiary);

        // Explicit pool size, never rayon's default: the thread count is an
        // experimental variable and must be exactly what the result file says.
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(config.threads)
            .build()
            .expect("thread pool");

        let mv = MVMemory::new(n, beneficiary, config.granularity);
        let mut state: Vec<TxState> = (0..n).map(|_| TxState::default()).collect();
        let mut stats = ExecStats {
            transactions: n,
            threads: config.threads,
            executions_per_tx: vec![0; n],
            ..Default::default()
        };

        let start = Instant::now();
        let mut pending: Vec<TxIdx> = (0..n).collect();

        while !pending.is_empty() {
            stats.rounds += 1;
            // Proven bound, not a timeout: at least one transaction settles per
            // round (module docs). Exceeding it means the store or validation is
            // broken, and a loud failure beats a hung benchmark. Mutation testing
            // found that without this, a reader resolving its own previous
            // incarnation spins forever instead of failing.
            assert!(
                stats.rounds <= n,
                "round {} exceeds the proven bound of {n}: validation cannot make \
                 progress, which means multi-version memory is inconsistent",
                stats.rounds
            );

            // Execute. Each worker writes its results into MVMemory as soon as
            // it finishes, so later transactions in the same round may already
            // see them — or may not. Either way validation sorts it out.
            let incarnations: Vec<Incarnation> =
                pending.iter().map(|&j| state[j].incarnation).collect();
            let executed: Vec<(TxIdx, TxState)> = pool.install(|| {
                pending
                    .par_iter()
                    .zip(incarnations.par_iter())
                    .map(|(&j, &incarnation)| {
                        let view = MVView::new(&mv, base, j);
                        let out = execute(view, txs[j].clone(), &config.block, config.granularity);
                        let (reverted, refused) = match &out.outcome {
                            Ok(done) => {
                                mv.apply(j, incarnation, Some(&done.writes), &out.served, base);
                                (!done.is_success(), None)
                            }
                            Err(err) => {
                                mv.apply(j, incarnation, None, &out.served, base);
                                (false, Some(format!("{err:?}")))
                            }
                        };
                        (
                            j,
                            TxState {
                                incarnation,
                                reads: out.reads,
                                reverted,
                                refused,
                            },
                        )
                    })
                    .collect()
            });

            stats.executions += executed.len();
            let lowest = pending[0];
            for (j, result) in executed {
                stats.executions_per_tx[j] += 1;
                state[j] = result;
            }

            // Validate. Everything below the lowest pending index was valid last
            // round and nothing it depends on has changed since.
            let aborted: Vec<TxIdx> = pool.install(|| {
                (lowest..n)
                    .into_par_iter()
                    .filter(|&j| !mv.validate(j, &state[j].reads))
                    .collect()
            });

            stats.aborts += aborted.len();
            for &j in &aborted {
                state[j].incarnation += 1;
            }
            pending = aborted;
        }
        stats.wall_clock = start.elapsed();

        for (i, tx) in state.iter().enumerate() {
            if let Some(err) = &tx.refused {
                panic!("transaction {i} could not be executed: {err}");
            }
        }
        stats.reverted = state.iter().filter(|t| t.reverted).count();

        BlockOutcome {
            state: mv.snapshot(base),
            stats,
        }
    }
}
