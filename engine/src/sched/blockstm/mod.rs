//! Collaborative Block-STM (M2b).
//!
//! Built in four steps, each committed and reviewed on its own:
//!
//! 1. [`coordinator`] — the task scheduler, driven by fabricated execution.
//! 2. `ESTIMATE` markers and dependency waiting in the multi-version store.
//! 3. Real execution and validation wired to the coordinator.
//! 4. The M2b gate and the comparison against the M2a baseline.
//!
//! Only step 1 exists so far. Nothing here is used by a [`Scheduler`] yet.
//!
//! [`Scheduler`]: crate::sched::Scheduler

pub mod coordinator;

pub use coordinator::{Coordinator, Status, Task};
