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

/// An account as it exists in state, under EIP-161: an account that is empty —
/// zero nonce, zero balance, no code — does not exist.
///
/// Mainnet has deleted touched empty accounts since Spurious Dragon, and Anvil
/// does the same, so both engines must agree with it or the M1 cross-validation
/// fails. It also decides what a call to a precompile does to state: the
/// precompile's account is absent before and empty after, which is no change at
/// all (E14, D19).
pub fn existing(info: Option<AccountInfo>) -> Option<AccountInfo> {
    info.filter(|i| !i.is_empty())
}

/// Whether two views of an account are the same state (D18).
///
/// Compares balance, nonce and code hash after EIP-161 normalisation. Bytecode
/// is not compared: it is determined by the code hash, and the multi-version
/// store strips it before storing.
pub fn same_account(a: Option<&AccountInfo>, b: Option<&AccountInfo>) -> bool {
    let key =
        |i: Option<&AccountInfo>| existing(i.cloned()).map(|i| (i.balance, i.nonce, i.code_hash));
    key(a) == key(b)
}

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
