//! Parallel transaction execution engine for EVM workloads.
//!
//! Three schedulers — sequential, static, and Block-STM — share one execution
//! path over revm. They differ only in which state view is handed to the EVM
//! and what is done with the result.
//!
//! See `docs/DESIGN.md` for the architecture and `DECISIONS.md` for why it is
//! shaped this way.

pub mod state;
pub mod types;

pub use state::{BaseState, ReadRecorder, SimpleState, SimpleView, StateError, StateView};
pub use types::{Granularity, Incarnation, Key, ReadOrigin, ReadSet, TxIdx, Version};
