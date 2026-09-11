//! State access.
//!
//! Everything the EVM reads passes through this module, in one layered stack:
//!
//! ```text
//!   revm  ->  WrapDatabaseRef  ->  ReadRecorder<V>  ->  V: StateView  ->  BaseState
//! ```
//!
//! [`ReadRecorder`] is the only type in the crate that implements revm's
//! `DatabaseRef`, so no read can reach the EVM without being logged.

mod base;
mod recorder;
mod simple;

pub use base::{BaseState, StateError};
pub use recorder::ReadRecorder;
pub use simple::{SimpleState, SimpleView};

use crate::types::ReadOrigin;
use revm::primitives::{Address, StorageKey, StorageValue, B256};
use revm::state::{AccountInfo, Bytecode};

/// A view of state as seen by one transaction.
///
/// Deliberately *not* revm's `DatabaseRef`. Implementors return the origin of
/// each read alongside its value, which is what makes validation a version
/// comparison rather than a value comparison. Wrapping an implementor in
/// [`ReadRecorder`] produces something the EVM can use.
pub trait StateView {
    type Error;

    fn basic(&self, address: Address) -> Result<(Option<AccountInfo>, ReadOrigin), Self::Error>;

    fn code_by_hash(&self, code_hash: B256) -> Result<(Bytecode, ReadOrigin), Self::Error>;

    fn storage(
        &self,
        address: Address,
        index: StorageKey,
    ) -> Result<(StorageValue, ReadOrigin), Self::Error>;

    fn block_hash(&self, number: u64) -> Result<(B256, ReadOrigin), Self::Error>;
}
