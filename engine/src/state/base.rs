//! The pre-block state snapshot.
//!
//! Immutable for the duration of a block. Every read that no transaction has
//! overwritten falls through to here.

use super::StateView;
use crate::types::ReadOrigin;
use revm::primitives::{Address, HashMap, StorageKey, StorageValue, B256, KECCAK_EMPTY};
use revm::state::{AccountInfo, Bytecode};

/// Errors raised by the state layer.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum StateError {
    /// Code was requested by a hash the snapshot does not know.
    MissingCode(B256),
}

impl core::fmt::Display for StateError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            StateError::MissingCode(h) => write!(f, "no bytecode for hash {h}"),
        }
    }
}

impl core::error::Error for StateError {}

impl revm::database_interface::DBErrorMarker for StateError {}

/// Pre-block state. Built once, then read concurrently by every worker.
#[derive(Clone, Default, Debug)]
pub struct BaseState {
    accounts: HashMap<Address, AccountInfo>,
    storage: HashMap<(Address, StorageKey), StorageValue>,
    code: HashMap<B256, Bytecode>,
    block_hashes: HashMap<u64, B256>,
}

impl BaseState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Inserts an account, registering its code under the code hash if present.
    pub fn insert_account(&mut self, address: Address, info: AccountInfo) {
        if let Some(code) = &info.code {
            if info.code_hash != KECCAK_EMPTY {
                self.code.insert(info.code_hash, code.clone());
            }
        }
        self.accounts.insert(address, info);
    }

    pub fn insert_storage(&mut self, address: Address, index: StorageKey, value: StorageValue) {
        self.storage.insert((address, index), value);
    }

    pub fn insert_block_hash(&mut self, number: u64, hash: B256) {
        self.block_hashes.insert(number, hash);
    }

    pub fn account(&self, address: Address) -> Option<&AccountInfo> {
        self.accounts.get(&address)
    }

    pub fn slot(&self, address: Address, index: StorageKey) -> StorageValue {
        self.storage
            .get(&(address, index))
            .copied()
            .unwrap_or_default()
    }

    pub fn accounts(&self) -> impl Iterator<Item = (&Address, &AccountInfo)> {
        self.accounts.iter()
    }

    pub fn slots(&self) -> impl Iterator<Item = (&(Address, StorageKey), &StorageValue)> {
        self.storage.iter()
    }
}

impl StateView for BaseState {
    type Error = StateError;

    fn basic(&self, address: Address) -> Result<(Option<AccountInfo>, ReadOrigin), Self::Error> {
        Ok((self.accounts.get(&address).cloned(), ReadOrigin::Base))
    }

    fn code_by_hash(&self, code_hash: B256) -> Result<(Bytecode, ReadOrigin), Self::Error> {
        if code_hash == KECCAK_EMPTY {
            return Ok((Bytecode::default(), ReadOrigin::Base));
        }
        self.code
            .get(&code_hash)
            .cloned()
            .map(|c| (c, ReadOrigin::Base))
            .ok_or(StateError::MissingCode(code_hash))
    }

    fn storage(
        &self,
        address: Address,
        index: StorageKey,
    ) -> Result<(StorageValue, ReadOrigin), Self::Error> {
        Ok((self.slot(address, index), ReadOrigin::Base))
    }

    fn block_hash(&self, number: u64) -> Result<(B256, ReadOrigin), Self::Error> {
        Ok((
            self.block_hashes.get(&number).copied().unwrap_or_default(),
            ReadOrigin::Base,
        ))
    }
}
