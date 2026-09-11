//! Core types shared by every scheduler.

use revm::primitives::{Address, StorageKey, B256};

/// Position of a transaction within its block. Defines serial order.
pub type TxIdx = usize;

/// How many times a transaction has been executed. The first execution is 0;
/// each abort bumps it.
pub type Incarnation = u32;

/// Identifies one execution of one transaction.
///
/// Validation compares versions rather than values: if a read still resolves to
/// the same version it resolved to during execution, the value is necessarily
/// unchanged.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct Version {
    pub tx: TxIdx,
    pub incarnation: Incarnation,
}

impl Version {
    pub fn new(tx: TxIdx, incarnation: Incarnation) -> Self {
        Self { tx, incarnation }
    }
}

/// Granularity at which storage conflicts are detected.
///
/// This is an experimental variable, not a tuning knob: the brief asks for a
/// conflict rule "based on account access, storage slot access, or simplified
/// read/write sets", and comparing the two settings measures how many false
/// conflicts the coarser rule introduces.
///
/// Note the axis that is *not* available here. Separating balance reads from
/// nonce reads would be finer still, but revm's `basic_ref` returns the whole
/// `AccountInfo`, so the database boundary cannot tell which field the EVM
/// actually consumed. Distinguishing them would require an `Inspector`
/// observing opcodes. Recorded as a limitation rather than attempted.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Granularity {
    /// One key per storage slot. Precise.
    #[default]
    Slot,
    /// One key per account; every slot of an account collides. Coarse, and the
    /// point of the comparison.
    Account,
}

/// A location in state that a transaction can read or write.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum Key {
    /// Account balance, nonce and code hash, as one unit — see [`Granularity`].
    Basic(Address),
    /// A single storage slot.
    Storage(Address, StorageKey),
    /// Contract code, addressed by hash. Immutable once deployed, so reads of
    /// it never conflict; recorded for completeness and for read-set auditing.
    CodeHash(B256),
    /// A historical block hash. Immutable within a block.
    BlockHash(u64),
}

impl Key {
    /// Maps a storage access to the key used for conflict detection under the
    /// given granularity.
    pub fn storage(address: Address, index: StorageKey, granularity: Granularity) -> Self {
        match granularity {
            Granularity::Slot => Key::Storage(address, index),
            Granularity::Account => Key::Basic(address),
        }
    }

    /// Whether writes to this key can ever occur. Immutable keys are recorded
    /// in read sets but can be skipped during validation.
    pub fn is_mutable(&self) -> bool {
        matches!(self, Key::Basic(_) | Key::Storage(_, _))
    }
}

/// Where a read resolved to.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ReadOrigin {
    /// Fell through to the pre-block snapshot: no earlier transaction in this
    /// block had written the location.
    Base,
    /// Read a value written by an earlier transaction in this block.
    Written(Version),
}

/// Every location a transaction read, and which version it read.
///
/// This is the sole input to validation. A location missing from here is a
/// silent correctness bug — see `docs/RISKS.md` R1.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct ReadSet {
    entries: Vec<(Key, ReadOrigin)>,
}

impl ReadSet {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record(&mut self, key: Key, origin: ReadOrigin) {
        self.entries.push((key, origin));
    }

    pub fn iter(&self) -> impl Iterator<Item = &(Key, ReadOrigin)> {
        self.entries.iter()
    }

    /// Entries that validation must re-check. Immutable locations cannot change
    /// and are skipped.
    pub fn mutable(&self) -> impl Iterator<Item = &(Key, ReadOrigin)> {
        self.entries.iter().filter(|(k, _)| k.is_mutable())
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }
}
