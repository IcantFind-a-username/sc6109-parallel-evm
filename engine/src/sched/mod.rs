//! Schedulers.
//!
//! Three strategies over one execution path:
//!
//! - [`sequential`] — the baseline, and the definition of correct.
//! - `static_sched` — declared access sets, EIP-7928 style. *(M3)*
//! - [`rounds`] — optimistic execution in rounds, the M2a fallback.
//! - [`blockstm`] — collaborative Block-STM with dependency tracking (M2b).

pub mod blockstm;
pub mod rounds;
pub mod sequential;

pub use blockstm::BlockStmScheduler;
pub use rounds::RoundScheduler;
pub use sequential::SequentialScheduler;

use crate::outcome::BlockOutcome;
use crate::state::BaseState;
use crate::types::Granularity;
use revm::context::{BlockEnv, TxEnv};
use revm::primitives::{Address, TxKind};

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

/// The beneficiary exemption (D4) holds only while transactions do nothing to
/// the beneficiary but pay it. A transaction sending from or to it would read
/// its balance, which the parallel engines serve from base state.
pub(crate) fn assert_beneficiary_only_paid(txs: &[TxEnv], beneficiary: Address) {
    for (i, tx) in txs.iter().enumerate() {
        assert!(
            tx.caller != beneficiary && tx.kind != TxKind::Call(beneficiary),
            "tx {i} sends from or to the block beneficiary; the exemption (D4) assumes \
             transactions only ever pay it"
        );
    }
}
