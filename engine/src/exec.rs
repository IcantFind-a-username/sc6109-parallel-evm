//! The single execution path.
//!
//! Every scheduler executes transactions through this function. They differ in
//! which [`StateView`] they supply and what they do with the output — never in
//! how the EVM is driven. One path means one place for an execution bug to
//! live, and it is why the sequential and parallel engines cannot silently
//! diverge in how they call revm.

use crate::state::{ReadRecorder, StateView};
use crate::types::{Granularity, ReadSet};
use revm::context::result::{EVMError, ExecutionResult, InvalidTransaction};
use revm::context::{BlockEnv, TxEnv};
use revm::database_interface::{DBErrorMarker, WrapDatabaseRef};
use revm::state::EvmState;
use revm::{Context, ExecuteEvm, MainBuilder, MainContext};

/// One transaction's output: what happened, what it wrote, what it read.
pub struct Executed {
    pub result: ExecutionResult,
    pub writes: EvmState,
    pub reads: ReadSet,
}

impl Executed {
    pub fn is_success(&self) -> bool {
        self.result.is_success()
    }

    pub fn gas_used(&self) -> u64 {
        self.result.tx_gas_used()
    }
}

/// Why a transaction could not be executed at all.
///
/// Distinct from a revert: a reverted transaction executed successfully and
/// produced state changes (the nonce and fee). An error here means the EVM
/// refused the transaction, which in a generated workload means the workload is
/// malformed — most often a nonce that does not match the sender's state.
pub type ExecError<E> = EVMError<E, InvalidTransaction>;

/// Executes one transaction against one view of state.
///
/// The view is wrapped in a [`ReadRecorder`], so the returned read set covers
/// every location the EVM touched.
pub fn execute<V>(
    view: V,
    tx: TxEnv,
    block: &BlockEnv,
    granularity: Granularity,
) -> Result<Executed, ExecError<V::Error>>
where
    V: StateView,
    V::Error: DBErrorMarker + core::error::Error,
{
    let recorder = ReadRecorder::new(view, granularity);
    let mut evm = Context::mainnet()
        .with_block(block.clone())
        .with_db(WrapDatabaseRef(&recorder))
        .build_mainnet();

    let out = evm.transact(tx)?;
    Ok(Executed {
        result: out.result,
        writes: out.state,
        reads: recorder.take_read_set(),
    })
}
