//! Differential testing.
//!
//! Runs the same workload through two schedulers and compares the state they
//! produce. This is the mechanism that makes every correctness claim in the
//! project checkable rather than argued.
//!
//! # Why it has to exist before Block-STM does
//!
//! The top risk in this project is an incomplete read set: a location the EVM
//! read but the recorder did not log is one validation will never re-check, so
//! a transaction can observe stale state, pass validation, and commit a wrong
//! block. The failure is probabilistic — a divergence on one slot, at high
//! thread counts, on perhaps one seed in two hundred. Nothing about the code
//! looks wrong. Only a comparison against an independent implementation, run
//! across many seeds, will surface it.
//!
//! See `docs/RISKS.md` R1 and D8 in `DECISIONS.md`.

use crate::outcome::BlockOutcome;
use crate::sched::{Scheduler, SchedulerConfig};
use crate::workload::Workload;

/// What a differential run found.
pub struct Comparison {
    pub left: BlockOutcome,
    pub right: BlockOutcome,
    pub differences: Vec<String>,
}

impl Comparison {
    pub fn agrees(&self) -> bool {
        self.differences.is_empty()
    }

    /// A failure report naming the workload, the seed and the first few
    /// differing locations. A differential failure that only says "states
    /// differ" costs hours to chase; one that names the slot costs minutes.
    pub fn report(&self, workload: &Workload, left: &str, right: &str) -> String {
        let mut out = format!(
            "{left} and {right} disagree on workload '{}' (seed {}): {} difference(s)",
            workload.name,
            workload.seed,
            self.differences.len()
        );
        for d in self.differences.iter().take(10) {
            out.push_str("\n  ");
            out.push_str(d);
        }
        if self.differences.len() > 10 {
            out.push_str(&format!("\n  ... and {} more", self.differences.len() - 10));
        }
        out
    }
}

/// Runs a workload through both schedulers and compares the result.
pub fn compare<L: Scheduler, R: Scheduler>(
    left: &L,
    right: &R,
    workload: &Workload,
    config: &SchedulerConfig,
) -> Comparison {
    let l = left.execute_block(&workload.txs, &workload.base, config);
    let r = right.execute_block(&workload.txs, &workload.base, config);
    let differences = l.state.diff(&r.state);
    Comparison {
        left: l,
        right: r,
        differences,
    }
}

/// Asserts two schedulers agree, panicking with a located report if not.
///
/// Intended for tests and for the CI sweep.
pub fn assert_agree<L: Scheduler, R: Scheduler>(
    left: &L,
    right: &R,
    workload: &Workload,
    config: &SchedulerConfig,
) {
    let comparison = compare(left, right, workload, config);
    assert!(
        comparison.agrees(),
        "{}",
        comparison.report(workload, left.name(), right.name())
    );
}
