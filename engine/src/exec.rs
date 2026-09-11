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
use revm::primitives::Address;
use revm::state::{AccountInfo, EvmState};
use revm::{Context, ExecuteEvm, MainBuilder, MainContext};
use std::collections::HashMap;

/// Why the EVM refused a transaction outright.
///
/// Distinct from a revert: a reverted transaction executed and produced state
/// changes (nonce, fee). A refusal produces none.
pub type ExecError<E> = EVMError<E, InvalidTransaction>;

/// A transaction that the EVM accepted and ran to completion — successfully or
/// not.
pub struct Completed {
    pub result: ExecutionResult,
    pub writes: EvmState,
}

impl Completed {
    pub fn is_success(&self) -> bool {
        self.result.is_success()
    }

    pub fn gas_used(&self) -> u64 {
        self.result.tx_gas_used()
    }
}

/// One execution attempt: what it read, and what came of it.
///
/// The read set is returned **whether or not the EVM accepted the
/// transaction**. Under speculative execution a refusal is often not a property
/// of the transaction at all: a sender's second transaction, run before its
/// first has written back, reads the old nonce and is refused with
/// `NonceTooHigh`. The read set is what lets validation see that the refusal
/// rested on a stale read and schedule a retry. Discarding it on error would
/// make every same-sender pair in a block a fatal error. See E6 in
/// `docs/AI_USAGE.md`.
pub struct Executed<E> {
    pub reads: ReadSet,
    /// Account values the execution was served, for telling writes from
    /// touches (D18).
    pub served: HashMap<Address, Option<AccountInfo>>,
    pub outcome: Result<Completed, ExecError<E>>,
}

/// Executes one transaction against one view of state.
///
/// The view is wrapped in a [`ReadRecorder`], so the returned read set covers
/// every location the EVM touched.
pub fn execute<V>(
    view: V,
    tx: TxEnv,
    block: &BlockEnv,
    granularity: Granularity,
) -> Executed<V::Error>
where
    V: StateView,
    V::Error: DBErrorMarker + core::error::Error,
{
    let recorder = ReadRecorder::new(view, granularity);
    let outcome = {
        let mut evm = Context::mainnet()
            .with_block(block.clone())
            .with_db(WrapDatabaseRef(&recorder))
            .build_mainnet();
        evm.transact(tx).map(|out| Completed {
            result: out.result,
            writes: out.state,
        })
    };
    Executed {
        reads: recorder.take_read_set(),
        served: recorder.take_served_accounts(),
        outcome,
    }
}
