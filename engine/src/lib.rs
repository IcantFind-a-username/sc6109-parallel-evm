//! Parallel transaction execution engine for EVM workloads.
//!
//! Three schedulers — sequential, static, and Block-STM — share one execution
//! path over revm. They differ only in which state view is handed to the EVM
//! and what is done with the result.
//!
//! See `docs/DESIGN.md` for the architecture and `DECISIONS.md` for why it is
//! shaped this way.

pub mod diff;
pub mod exec;
pub mod outcome;
pub mod sched;
pub mod state;
pub mod types;
pub mod workload;

pub use diff::{assert_agree, compare, Comparison};
pub use outcome::{AccountSummary, BlockOutcome, ExecStats, StateSnapshot};
pub use sched::{Scheduler, SchedulerConfig, SequentialScheduler};
pub use state::{BaseState, ReadRecorder, SimpleState, SimpleView, StateError, StateView};
pub use types::{Granularity, Incarnation, Key, ReadOrigin, ReadSet, TxIdx, Version};
pub use workload::{Distribution, TransferConfig, TransferWorkload, Workload};
