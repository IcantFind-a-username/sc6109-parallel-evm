//! Schedulers.
//!
//! Three strategies over one execution path:
//!
//! - [`sequential`] — the baseline, and the definition of correct.
//! - `static_sched` — declared access sets, EIP-7928 style. *(M3)*
//! - [`rounds`] — optimistic execution in rounds, the M2a fallback.
//! - `blockstm` — collaborative Block-STM with dependency tracking. *(M2b)*

pub mod blockstm;
pub mod rounds;
pub mod sequential;

pub use rounds::RoundScheduler;
pub use sequential::SequentialScheduler;

use crate::outcome::BlockOutcome;
use crate::state::BaseState;
use crate::types::Granularity;
use revm::context::{BlockEnv, TxEnv};

/// Configuration shared by every scheduler.
#[derive(Clone, Debug)]
pub struct SchedulerConfig {
    pub threads: usize,
    pub granularity: Granularity,
    pub block: BlockEnv,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            threads: 1,
            granularity: Granularity::default(),
            block: BlockEnv::default(),
        }
    }
}

/// Executes a block of transactions.
///
/// Implementors must produce the state sequential execution would have produced
/// — that is the whole correctness requirement, and the differential test is
/// what enforces it.
pub trait Scheduler {
    fn name(&self) -> &'static str;

    fn execute_block(
        &self,
        txs: &[TxEnv],
        base: &BaseState,
        config: &SchedulerConfig,
    ) -> BlockOutcome;
}
