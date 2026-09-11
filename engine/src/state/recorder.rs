//! Read-set capture.
//!
//! The only implementor of revm's `DatabaseRef` in this crate. Every read the
//! EVM performs is logged here before the value reaches it.
//!
//! # Why this type exists
//!
//! revm records no read set. Block-STM validation is nothing but a re-check of
//! the read set, so a read that is not logged is a location validation will
//! never re-check — a transaction can observe stale state, pass validation, and
//! corrupt the block. The failure is probabilistic and appears only under
//! concurrency. See `docs/RISKS.md` R1.
//!
//! Two rules keep that from happening, and both matter:
//!
//! 1. Logging happens in exactly one place, [`ReadRecorder::record`]. No method
//!    below touches `self.log` directly.
//! 2. **Every method of `DatabaseRef` is implemented explicitly, including
//!    those revm provides a default for.** revm 41's
//!    `storage_by_account_id_ref` has a default body that delegates to
//!    `storage_ref`, which would be logged — but that is a property of the
//!    current implementation, not a guarantee, and a defaulted method is not a
//!    compile error to omit. Relying on it once meant the structural protection
//!    was weaker than assumed; see E1 in `docs/AI_USAGE.md`.

use super::StateView;
use crate::types::{Granularity, Key, ReadOrigin, ReadSet};
use core::cell::RefCell;
use revm::database_interface::DatabaseRef;
use revm::primitives::{Address, StorageKey, StorageValue, B256};
use revm::state::{AccountId, AccountInfo, Bytecode};
use std::collections::HashMap;

/// Wraps a [`StateView`] and logs every read against it.
///
/// One recorder per transaction execution. `RefCell` rather than a lock because
/// a recorder is never shared across threads — each worker builds its own.
#[derive(Debug)]
pub struct ReadRecorder<V: StateView> {
    inner: V,
    log: RefCell<ReadSet>,
    /// The account values this execution was served, keyed by address. A side
    /// log beside the read set, never part of it (D18): the store needs them to
    /// tell an account the execution changed from one it merely touched.
    served: RefCell<HashMap<Address, Option<AccountInfo>>>,
    granularity: Granularity,
}

impl<V: StateView> ReadRecorder<V> {
    pub fn new(inner: V, granularity: Granularity) -> Self {
        Self {
            inner,
            log: RefCell::new(ReadSet::new()),
            served: RefCell::new(HashMap::new()),
            granularity,
        }
    }

    /// Takes the accumulated read set, leaving the recorder empty and ready to
    /// be reused for the next incarnation.
    pub fn take_read_set(&self) -> ReadSet {
        core::mem::take(&mut *self.log.borrow_mut())
    }

    /// Takes the account values served to this execution. The first value
    /// served for each address is kept: it is what the execution started from,
    /// and revm caches an account after loading it once.
    pub fn take_served_accounts(&self) -> HashMap<Address, Option<AccountInfo>> {
        core::mem::take(&mut *self.served.borrow_mut())
    }

    pub fn read_set_len(&self) -> usize {
        self.log.borrow().len()
    }

    pub fn granularity(&self) -> Granularity {
        self.granularity
    }

    pub fn into_inner(self) -> V {
        self.inner
    }

    /// The single point at which reads are logged.
    ///
    /// Every `DatabaseRef` method below is a one-line call to this. Adding a
    /// read path without routing it through here is the bug this design exists
    /// to prevent.
    fn record<T>(
        &self,
        key: Key,
        fetch: impl FnOnce(&V) -> Result<(T, ReadOrigin), V::Error>,
    ) -> Result<T, V::Error> {
        let (value, origin) = fetch(&self.inner)?;
        self.log.borrow_mut().record(key, origin);
        Ok(value)
    }
}

impl<V: StateView> DatabaseRef for ReadRecorder<V>
where
    V::Error: revm::database_interface::DBErrorMarker + core::error::Error,
{
    type Error = V::Error;

    fn basic_ref(&self, address: Address) -> Result<Option<AccountInfo>, Self::Error> {
        // Logged through `record` exactly as before; the served value is an
        // additional side log, not a substitute for the read-set entry.
        let info = self.record(Key::Basic(address), |v| v.basic(address))?;
        self.served
            .borrow_mut()
            .entry(address)
            .or_insert_with(|| info.clone().map(|i| i.without_code()));
        Ok(info)
    }

    fn code_by_hash_ref(&self, code_hash: B256) -> Result<Bytecode, Self::Error> {
        self.record(Key::CodeHash(code_hash), |v| v.code_by_hash(code_hash))
    }

    fn storage_ref(
        &self,
        address: Address,
        index: StorageKey,
    ) -> Result<StorageValue, Self::Error> {
        self.record(Key::storage(address, index, self.granularity), |v| {
            v.storage(address, index)
        })
    }

    /// Explicit despite revm supplying a default body. See the module docs.
    fn storage_by_account_id_ref(
        &self,
        address: Address,
        _account_id: AccountId,
        storage_key: StorageKey,
    ) -> Result<StorageValue, Self::Error> {
        self.record(Key::storage(address, storage_key, self.granularity), |v| {
            v.storage(address, storage_key)
        })
    }

    fn block_hash_ref(&self, number: u64) -> Result<B256, Self::Error> {
        self.record(Key::BlockHash(number), |v| v.block_hash(number))
    }
}
