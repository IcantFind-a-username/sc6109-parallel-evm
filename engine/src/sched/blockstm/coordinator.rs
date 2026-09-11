//! The collaborative scheduler at the core of Block-STM (M2b, step 1).
//!
//! This module decides *which* task each worker performs next. It knows nothing
//! about the EVM or about state: executing a transaction and validating one are
//! supplied by the caller. Keeping coordination separate from execution means
//! the protocol below can be tested on its own, with fabricated execution
//! results, before any real transaction runs through it.
//!
//! Follows the scheduler of the Block-STM paper (Gelashvili et al., 2022),
//! including its dependency handling: a transaction whose read hits an
//! `ESTIMATE` is parked on the transaction that wrote it and put back in the
//! queue when that transaction finishes executing. Nothing spins or sleeps
//! waiting for a dependency; a parked transaction simply holds no task.
//!
//! # State
//!
//! - `execution_idx` — next transaction index to hand out for execution.
//! - `validation_idx` — next transaction index to hand out for validation.
//! - Per transaction, an incarnation number and a status:
//!
//! ```text
//!   ReadyToExecute(i) --next_task--> Executing(i) --finish_execution--> Executed(i)
//!          ^                              |                                |
//!          |                       add_dependency              try_validation_abort
//!          |                              v                                v
//!          +------ (i := i + 1) ------ Aborting(i) <-----------------------+
//!            finish_validation, or the blocking transaction's finish_execution
//! ```
//!
//! Workers take whichever of the two indices is lower, so validation of early
//! transactions is always preferred over execution of later ones — the lower a
//! transaction, the more others depend on it being settled.
//!
//! # Task ownership
//!
//! Every task in flight owns exactly one unit of `active_tasks`. A task is
//! acquired when an index is claimed; it is released when the worker finishes
//! with no follow-up task, or *transferred* when `finish_execution` or
//! `finish_validation` hands back a follow-up (validate what was just executed;
//! re-execute what was just aborted). No path releases twice and none leaks —
//! `release` asserts against underflow, and tests assert the count returns to
//! zero. (The paper's pseudocode decrements inside `try_incarnate` as well as in
//! `finish_validation`; here `try_incarnate` never touches the count and every
//! caller accounts for it, which keeps the rule to one sentence.)
//!
//! # Why no task is lost: the `done` check
//!
//! The block is done when both indices have passed the end, no task is in
//! flight, and nothing lowered an index while that was being checked. The last
//! condition is what `decrease_count` is for. `check_done` reads it before and
//! after reading the indices and the active count. Consider a worker that lowers
//! `validation_idx` and then releases its task, racing a checker that reads
//! `validation_idx` *before* the lowering but the active count *after* the
//! release (so sees zero). The lowering incremented `decrease_count` before the
//! release; the checker's second read of `decrease_count` comes after its read
//! of the active count, which is after the release — so it sees the change and
//! declines to finish. All operations are `SeqCst`, which is what makes "before"
//! and "after" a single total order to argue over. Weaker orderings are an
//! optimisation for later, if profiling ever justifies one.
//!
//! # Why it terminates
//!
//! Claimed indices only grow, except when `validation_idx` is lowered — only by
//! an abort or by an execution that wrote a location its previous incarnation
//! did not — or `execution_idx` is lowered to resume parked transactions, which
//! happens once per completed execution of the transaction they waited on. So
//! the number of tasks is finite if the number of incarnations is — and that is
//! a property of validation, not of this module. Transaction 0 reads only
//! pre-block state and can never fail validation. Inductively, once every transaction below `j` has run its final
//! incarnation, `j`'s next incarnation reads only final values and passes. So
//! each transaction aborts finitely often, provided validation is exact — which
//! the multi-version store supplies in step 3, and which the fabricated
//! validators in this module's tests supply by construction.
//!
//! Unlike the round-based scheduler, there is no simple proven bound on rounds
//! or incarnations to assert here: the induction bounds each transaction's
//! aborts by the history of the transactions below it, not by a constant. A
//! store inconsistency therefore shows up as a hang rather than a failed
//! assertion. The tests guard against that with a wall-clock watchdog; the real
//! scheduler bounds total executions instead (see `BlockStmScheduler`).

use crate::types::{Incarnation, TxIdx, Version};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering::SeqCst};
use std::sync::{Mutex, MutexGuard};

/// Where a transaction is in its lifecycle. The incarnation lives alongside it
/// in [`TxState`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Status {
    ReadyToExecute,
    Executing,
    Executed,
    Aborting,
}

#[derive(Debug)]
struct TxState {
    incarnation: Incarnation,
    status: Status,
}

/// Work for one worker.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Task {
    Execute(Version),
    Validate(Version),
}

/// How an execution attempt ended, as reported to [`Coordinator::run_worker`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Execution {
    /// Ran to completion. `wrote_new_location` as for
    /// [`Coordinator::finish_execution`].
    Done { wrote_new_location: bool },
    /// A read hit an `ESTIMATE` written by transaction `on`.
    Blocked { on: TxIdx },
}

/// Shared by every worker executing one block.
pub struct Coordinator {
    len: usize,
    execution_idx: AtomicUsize,
    validation_idx: AtomicUsize,
    decrease_count: AtomicUsize,
    active_tasks: AtomicUsize,
    done: AtomicBool,
    txs: Vec<Mutex<TxState>>,
    /// Transactions parked on each transaction, to be resumed when it next
    /// finishes executing.
    dependents: Vec<Mutex<Vec<TxIdx>>>,
}

impl Coordinator {
    pub fn new(len: usize) -> Self {
        Self {
            len,
            execution_idx: AtomicUsize::new(0),
            validation_idx: AtomicUsize::new(0),
            decrease_count: AtomicUsize::new(0),
            active_tasks: AtomicUsize::new(0),
            done: AtomicBool::new(len == 0),
            txs: (0..len)
                .map(|_| {
                    Mutex::new(TxState {
                        incarnation: 0,
                        status: Status::ReadyToExecute,
                    })
                })
                .collect(),
            dependents: (0..len).map(|_| Mutex::new(Vec::new())).collect(),
        }
    }

    pub fn done(&self) -> bool {
        self.done.load(SeqCst)
    }

    /// The next task, or `None` if there is nothing to hand out right now.
    ///
    /// `None` is not the end: another worker may be about to lower an index.
    /// Callers loop until [`Coordinator::done`].
    pub fn next_task(&self) -> Option<Task> {
        if self.done() {
            return None;
        }
        if self.validation_idx.load(SeqCst) < self.execution_idx.load(SeqCst) {
            self.next_version_to_validate().map(Task::Validate)
        } else {
            self.next_version_to_execute().map(Task::Execute)
        }
    }

    fn next_version_to_execute(&self) -> Option<Version> {
        if self.execution_idx.load(SeqCst) >= self.len {
            self.check_done();
            return None;
        }
        self.acquire();
        let idx = self.execution_idx.fetch_add(1, SeqCst);
        let claimed = self.try_incarnate(idx);
        if claimed.is_none() {
            self.release();
        }
        claimed
    }

    fn next_version_to_validate(&self) -> Option<Version> {
        if self.validation_idx.load(SeqCst) >= self.len {
            self.check_done();
            return None;
        }
        self.acquire();
        let idx = self.validation_idx.fetch_add(1, SeqCst);
        if idx < self.len {
            let tx = self.tx(idx);
            if tx.status == Status::Executed {
                return Some(Version::new(idx, tx.incarnation));
            }
            // Not executed yet, or mid-re-execution: its finish_execution will
            // see validation_idx already past it and arrange its validation.
        }
        self.release();
        None
    }

    /// Moves a ready transaction to executing. Never touches the active count;
    /// callers account for it.
    fn try_incarnate(&self, idx: TxIdx) -> Option<Version> {
        if idx >= self.len {
            return None;
        }
        let mut tx = self.tx(idx);
        if tx.status == Status::ReadyToExecute {
            tx.status = Status::Executing;
            Some(Version::new(idx, tx.incarnation))
        } else {
            None
        }
    }

    /// Records a finished execution. May hand back its validation as a
    /// follow-up task.
    ///
    /// `wrote_new_location` — this incarnation wrote a location its previous
    /// incarnation did not. Then transactions above it that were already
    /// validated may have read past a location this one now writes, and must
    /// all be revalidated. Otherwise only this transaction needs validating.
    pub fn finish_execution(&self, version: Version, wrote_new_location: bool) -> Option<Task> {
        {
            let mut tx = self.tx(version.tx);
            assert!(
                tx.status == Status::Executing && tx.incarnation == version.incarnation,
                "finish_execution for {version:?} but transaction is {:?} at incarnation {}",
                tx.status,
                tx.incarnation
            );
            tx.status = Status::Executed;
        }
        // Status is set before the dependents are taken, and add_dependency
        // checks status under the dependents lock: a transaction parking at
        // this moment either sees Executed and retries at once, or is in the
        // list taken here. It cannot fall between.
        let parked = std::mem::take(&mut *self.dependents(version.tx));
        self.resume(parked);
        if self.validation_idx.load(SeqCst) > version.tx {
            if wrote_new_location {
                self.decrease_validation_idx(version.tx);
            } else {
                return Some(Task::Validate(version));
            }
        }
        self.release();
        None
    }

    /// Claims the right to abort a failed validation. Exactly one of several
    /// concurrent failing validations of the same incarnation wins.
    pub fn try_validation_abort(&self, version: Version) -> bool {
        let mut tx = self.tx(version.tx);
        if tx.status == Status::Executed && tx.incarnation == version.incarnation {
            tx.status = Status::Aborting;
            true
        } else {
            false
        }
    }

    /// Records a finished validation. If it aborted, schedules revalidation of
    /// everything above it and may hand back the re-execution as a follow-up.
    pub fn finish_validation(&self, version: Version, aborted: bool) -> Option<Task> {
        if aborted {
            self.set_ready(version.tx);
            self.decrease_validation_idx(version.tx + 1);
            if self.execution_idx.load(SeqCst) > version.tx {
                if let Some(next) = self.try_incarnate(version.tx) {
                    return Some(Task::Execute(next));
                }
            }
        }
        self.release();
        None
    }

    /// Parks `tx`, whose execution read an `ESTIMATE` written by `blocking`,
    /// until `blocking` next finishes executing.
    ///
    /// Returns `false` if `blocking` has already finished — the `ESTIMATE` has
    /// been replaced since it was read — in which case nothing was parked and
    /// the caller should re-execute the same version straight away. On `true`
    /// the caller's task is released: a parked transaction holds none.
    pub fn add_dependency(&self, tx: TxIdx, blocking: TxIdx) -> bool {
        assert!(
            blocking < tx,
            "tx {tx} blocked on later transaction {blocking}"
        );
        {
            let mut parked = self.dependents(blocking);
            if self.tx(blocking).status == Status::Executed {
                return false;
            }
            let mut state = self.tx(tx);
            assert_eq!(
                state.status,
                Status::Executing,
                "add_dependency on transaction {tx}"
            );
            state.status = Status::Aborting;
            parked.push(tx);
        }
        self.release();
        true
    }

    /// Returns parked transactions to the queue, one incarnation on, and pulls
    /// `execution_idx` back so they are picked up.
    fn resume(&self, parked: Vec<TxIdx>) {
        let Some(&lowest) = parked.iter().min() else {
            return;
        };
        for tx in parked {
            self.set_ready(tx);
        }
        if self.execution_idx.fetch_min(lowest, SeqCst) > lowest {
            self.decrease_count.fetch_add(1, SeqCst);
        }
    }

    /// Drives one worker until the block is done.
    ///
    /// `execute` runs a version and reports whether it wrote a location its
    /// previous incarnation did not. `validate` reports whether a version's
    /// reads still hold. The abort protocol — a failed validation aborts only if
    /// it wins [`Coordinator::try_validation_abort`] — lives here, once, rather
    /// than in every caller.
    ///
    /// `on_abort` runs after a failed validation wins the abort and before the
    /// transaction is re-queued: the moment to mark its writes as estimates.
    pub fn run_worker(
        &self,
        mut execute: impl FnMut(Version) -> Execution,
        mut validate: impl FnMut(Version) -> bool,
        mut on_abort: impl FnMut(Version),
    ) {
        let mut task = None;
        while !self.done() {
            task = match task.take().or_else(|| self.next_task()) {
                Some(Task::Execute(v)) => match execute(v) {
                    Execution::Done { wrote_new_location } => {
                        self.finish_execution(v, wrote_new_location)
                    }
                    // Parked, or — if the blocker finished meanwhile — retried.
                    Execution::Blocked { on } => {
                        if self.add_dependency(v.tx, on) {
                            None
                        } else {
                            Some(Task::Execute(v))
                        }
                    }
                },
                Some(Task::Validate(v)) => {
                    let aborted = !validate(v) && self.try_validation_abort(v);
                    if aborted {
                        on_abort(v);
                    }
                    self.finish_validation(v, aborted)
                }
                None => {
                    std::hint::spin_loop();
                    None
                }
            };
        }
    }

    fn set_ready(&self, idx: TxIdx) {
        let mut tx = self.tx(idx);
        assert_eq!(
            tx.status,
            Status::Aborting,
            "set_ready on transaction {idx}"
        );
        tx.incarnation += 1;
        tx.status = Status::ReadyToExecute;
    }

    fn decrease_validation_idx(&self, target: TxIdx) {
        if self.validation_idx.fetch_min(target, SeqCst) > target {
            self.decrease_count.fetch_add(1, SeqCst);
        }
    }

    fn check_done(&self) {
        let observed = self.decrease_count.load(SeqCst);
        let exec = self.execution_idx.load(SeqCst);
        let val = self.validation_idx.load(SeqCst);
        if exec.min(val) >= self.len
            && self.active_tasks.load(SeqCst) == 0
            && observed == self.decrease_count.load(SeqCst)
        {
            self.done.store(true, SeqCst);
        }
    }

    fn acquire(&self) {
        self.active_tasks.fetch_add(1, SeqCst);
    }

    fn release(&self) {
        let before = self.active_tasks.fetch_sub(1, SeqCst);
        assert!(
            before > 0,
            "active task count underflow: a task was released twice"
        );
    }

    fn tx(&self, idx: TxIdx) -> MutexGuard<'_, TxState> {
        self.txs[idx]
            .lock()
            .expect("transaction state lock poisoned")
    }

    fn dependents(&self, idx: TxIdx) -> MutexGuard<'_, Vec<TxIdx>> {
        self.dependents[idx]
            .lock()
            .expect("dependents lock poisoned")
    }

    /// Final status and incarnation of a transaction. For tests and statistics.
    pub fn status(&self, idx: TxIdx) -> (Status, Incarnation) {
        let tx = self.tx(idx);
        (tx.status, tx.incarnation)
    }

    /// Tasks currently in flight. Zero whenever the block is done.
    pub fn active_tasks(&self) -> usize {
        self.active_tasks.load(SeqCst)
    }
}

#[cfg(test)]
mod tests {
    //! The coordinator driven by fabricated execution and validation.
    //!
    //! A seeded oracle decides, for every transaction, how many incarnations
    //! fail validation before one passes, and whether each execution writes a
    //! new location. That bounds aborts by construction, so any failure to
    //! terminate is the coordinator's fault — caught by a wall-clock watchdog
    //! rather than by hanging the test run (E8).

    use super::*;
    use std::collections::{HashMap, HashSet};
    use std::sync::atomic::AtomicU64;
    use std::sync::{mpsc, Arc};
    use std::time::Duration;

    fn mix(mut x: u64) -> u64 {
        // splitmix64: a fixed, seedable hash so every run is reproducible.
        x = x.wrapping_add(0x9e37_79b9_7f4a_7c15);
        x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        x ^ (x >> 31)
    }

    #[derive(Clone, Copy)]
    struct Oracle {
        seed: u64,
        max_aborts: u32,
        /// Fabricate dependency blocks as well as validation failures.
        blocking: bool,
    }

    impl Oracle {
        fn new(seed: u64, max_aborts: u32) -> Self {
            Self {
                seed,
                max_aborts,
                blocking: false,
            }
        }
        fn blocking(seed: u64, max_aborts: u32) -> Self {
            Self {
                seed,
                max_aborts,
                blocking: true,
            }
        }
        /// Incarnations of `tx` that fail validation before one passes.
        fn aborts(&self, tx: TxIdx) -> u32 {
            (mix(self.seed ^ tx as u64) % (self.max_aborts as u64 + 1)) as u32
        }
        /// Whether this version's execution reads an ESTIMATE left by the
        /// transaction before it — only while that transaction is unfinished,
        /// as a real store behaves. Otherwise a retry after add_dependency
        /// returned false would block again forever.
        fn blocks_on(&self, v: Version, c: &Coordinator) -> Option<TxIdx> {
            let designated = self.blocking
                && v.tx > 0
                && mix(self.seed ^ 0xb10c ^ v.tx as u64).is_multiple_of(3);
            (designated && c.status(v.tx - 1).0 != Status::Executed).then(|| v.tx - 1)
        }
        fn valid(&self, v: Version) -> bool {
            v.incarnation >= self.aborts(v.tx)
        }
        fn wrote_new_location(&self, v: Version) -> bool {
            mix(self.seed.rotate_left(17) ^ (v.tx as u64) << 20 ^ v.incarnation as u64) & 1 == 1
        }
    }

    #[derive(Clone, Copy, Debug)]
    enum Event {
        /// `at` is stamped as the execute closure returns — before the worker
        /// calls `finish_execution`, so before any index it lowers.
        Executed {
            v: Version,
            at: u64,
            wrote_new: bool,
        },
        Validated {
            v: Version,
            at: u64,
            passed: bool,
        },
        Blocked {
            v: Version,
        },
    }

    /// Runs a block of `len` transactions on `threads` workers.
    fn run(len: usize, threads: usize, oracle: Oracle) -> (Arc<Coordinator>, Vec<Event>) {
        let coordinator = Arc::new(Coordinator::new(len));
        let clock = Arc::new(AtomicU64::new(0));
        let (send, recv) = mpsc::channel();

        for _ in 0..threads {
            let (c, clock, send) = (coordinator.clone(), clock.clone(), send.clone());
            std::thread::spawn(move || {
                let events = std::cell::RefCell::new(Vec::new());
                c.run_worker(
                    |v| {
                        if let Some(on) = oracle.blocks_on(v, &c) {
                            events.borrow_mut().push(Event::Blocked { v });
                            return Execution::Blocked { on };
                        }
                        if oracle.blocking {
                            // A fabricated execution is otherwise instant, so a
                            // predecessor has always finished by the time its
                            // successor looks — and the dependency path goes
                            // untested (dependencies_are_actually_exercised
                            // caught exactly that). A few microseconds of work
                            // makes neighbouring executions overlap.
                            for _ in 0..2_000 {
                                std::hint::spin_loop();
                            }
                        }
                        let wrote_new = oracle.wrote_new_location(v);
                        let at = clock.fetch_add(1, SeqCst);
                        events
                            .borrow_mut()
                            .push(Event::Executed { v, at, wrote_new });
                        Execution::Done {
                            wrote_new_location: wrote_new,
                        }
                    },
                    |v| {
                        let at = clock.fetch_add(1, SeqCst);
                        let passed = oracle.valid(v);
                        events.borrow_mut().push(Event::Validated { v, at, passed });
                        passed
                    },
                    |_| {},
                );
                let _ = send.send(events.into_inner());
            });
        }
        drop(send);

        let mut events = Vec::new();
        for _ in 0..threads {
            match recv.recv_timeout(Duration::from_secs(30)) {
                Ok(mut e) => events.append(&mut e),
                Err(_) => panic!(
                    "coordinator did not terminate: len {len}, threads {threads}, seed {}, max_aborts {}",
                    oracle.seed, oracle.max_aborts
                ),
            }
        }
        (coordinator, events)
    }

    /// Checks every property the coordinator promises.
    fn check(len: usize, threads: usize, oracle: Oracle) {
        let (c, events) = run(len, threads, oracle);
        let ctx = format!(
            "len {len}, threads {threads}, seed {}, max_aborts {}",
            oracle.seed, oracle.max_aborts
        );

        assert!(c.done(), "{ctx}: workers exited but done is false");
        assert_eq!(
            c.active_tasks(),
            0,
            "{ctx}: task count did not return to zero"
        );

        // 1. No version is handed out for execution twice.
        let mut seen = HashSet::new();
        let mut incarnations: HashMap<TxIdx, Vec<Incarnation>> = HashMap::new();
        for e in &events {
            if let Event::Executed { v, .. } = e {
                assert!(seen.insert(*v), "{ctx}: {v:?} executed twice");
                incarnations.entry(v.tx).or_default().push(v.incarnation);
            }
        }

        let mut aborted_at: HashMap<Version, u64> = HashMap::new();
        for e in &events {
            if let Event::Validated {
                v,
                at,
                passed: false,
            } = e
            {
                let t = aborted_at.entry(*v).or_insert(*at);
                *t = (*t).min(*at);
            }
        }

        for tx in 0..len {
            // 2. Every transaction ends executed, at exactly the incarnation the
            //    oracle lets pass, having run every incarnation before it.
            let (status, incarnation) = c.status(tx);
            assert_eq!(status, Status::Executed, "{ctx}: tx {tx} ended {status:?}");
            // Parking on a dependency consumes an incarnation, as in the paper,
            // so with dependencies the final incarnation exceeds the abort count.
            if !oracle.blocking {
                assert_eq!(
                    incarnation,
                    oracle.aborts(tx),
                    "{ctx}: tx {tx} final incarnation"
                );
            }
            // Every incarnation up to the final one ran exactly once — to
            // completion, or until it was parked.
            let mut ran = incarnations.remove(&tx).unwrap_or_default();
            ran.extend(events.iter().filter_map(|e| match e {
                Event::Blocked { v } if v.tx == tx => Some(v.incarnation),
                _ => None,
            }));
            ran.sort_unstable();
            ran.dedup();
            assert_eq!(
                ran,
                (0..=incarnation).collect::<Vec<_>>(),
                "{ctx}: tx {tx} incarnations"
            );

            // 3. Its final incarnation was validated and passed.
            let last_pass = events.iter().filter_map(|e| match e {
                Event::Validated {
                    v,
                    at,
                    passed: true,
                } if v.tx == tx && v.incarnation == incarnation => Some(*at),
                _ => None,
            });
            let last_pass = last_pass.max();
            assert!(
                last_pass.is_some(),
                "{ctx}: tx {tx} final incarnation never validated"
            );

            // 4. That validation came after every abort of every transaction
            //    below it. This is the property Block-STM's correctness rests
            //    on at the scheduling level: an abort below you means what you
            //    read may be stale, so you must be checked again.
            //
            //    Only the failing validation that wins try_validation_abort
            //    actually aborts, and the closure cannot see which one that
            //    is. A loser can be stamped *after* the winner lowered
            //    validation_idx, so treating every failure as an abort would
            //    flag correct revalidations as too early. Instead each aborted
            //    version is dated by its *earliest* failing validation: no
            //    later than the winner's stamp, which precedes the lowering.
            //    A conservative lower bound — every correct run satisfies it.
            let latest_abort_below = aborted_at
                .iter()
                .filter(|(v, _)| v.tx < tx)
                .map(|(_, at)| *at)
                .max();
            if let Some(abort) = latest_abort_below {
                assert!(
                    last_pass.unwrap() > abort,
                    "{ctx}: tx {tx} last validated at {} but a lower transaction aborted at {abort}",
                    last_pass.unwrap()
                );
            }

            // 4b. ...and after every execution below it that wrote a location
            //     its previous incarnation had not.
            //
            //     Aborts are not the only event that can make a higher
            //     transaction's reads stale. A re-execution that writes the
            //     *same* locations follows an abort, which 4 already covers, and
            //     (from step 2) leaves ESTIMATE markers that make readers of
            //     those locations fail. A *new* location is different: a higher
            //     transaction may have read straight through it, resolving to a
            //     writer further down, and nothing marked it. Only lowering
            //     validation_idx in finish_execution catches that.
            //
            //     Property 4 alone did not test this. Mutation testing proved it:
            //     making a new-location execution validate only itself, and not
            //     the transactions above it, passed every test.
            //
            //     The stamp is taken as the execute closure returns, before
            //     finish_execution runs. If validation_idx is past the
            //     transaction by then, finish_execution lowers it, and every
            //     higher transaction is handed a validation afterwards. If it is
            //     not, validation has yet to reach the higher transactions at
            //     all. Either way the last validation of each comes later — so,
            //     like 4, a conservative bound every correct run satisfies.
            let latest_new_write_below = events
                .iter()
                .filter_map(|e| match e {
                    Event::Executed {
                        v,
                        at,
                        wrote_new: true,
                    } if v.tx < tx => Some(*at),
                    _ => None,
                })
                .max();
            if let Some(write) = latest_new_write_below {
                assert!(
                    last_pass.unwrap() > write,
                    "{ctx}: tx {tx} last validated at {} but a lower transaction wrote a new \
                     location at {write}",
                    last_pass.unwrap()
                );
            }
        }
    }

    #[test]
    fn empty_block_is_done_immediately() {
        let c = Coordinator::new(0);
        assert!(c.done());
        assert_eq!(c.next_task(), None);
    }

    #[test]
    fn single_thread_without_aborts_executes_each_transaction_once() {
        let oracle = Oracle::new(1, 0);
        let (_, events) = run(50, 1, oracle);
        let executions = events
            .iter()
            .filter(|e| matches!(e, Event::Executed { .. }))
            .count();
        assert_eq!(executions, 50);
        check(50, 1, oracle);
    }

    #[test]
    fn single_transaction() {
        for max_aborts in [0, 3] {
            for threads in [1, 4] {
                check(1, threads, Oracle::new(9, max_aborts));
            }
        }
    }

    #[test]
    fn a_failed_validation_that_loses_the_race_does_not_abort() {
        let c = Coordinator::new(1);
        let Some(Task::Execute(v)) = c.next_task() else {
            panic!("expected execution")
        };
        assert_eq!(c.finish_execution(v, false), None);
        let Some(Task::Validate(v)) = c.next_task() else {
            panic!("expected validation")
        };
        assert!(c.try_validation_abort(v), "first failing validation wins");
        assert!(
            !c.try_validation_abort(v),
            "a second one for the same incarnation loses"
        );
    }

    #[test]
    fn stress_without_aborts() {
        for seed in 0..20 {
            for threads in [2, 4, 8, 16] {
                check(300, threads, Oracle::new(seed, 0));
            }
        }
    }

    #[test]
    fn stress_with_aborts() {
        for seed in 0..40 {
            for threads in [2, 4, 8, 16] {
                for max_aborts in [1, 3] {
                    check(200, threads, Oracle::new(seed, max_aborts));
                }
            }
        }
    }

    /// Sixteen workers on seven transactions: most workers find nothing to do
    /// most of the time, which is where done-detection races live.
    #[test]
    fn heavy_oversubscription() {
        for seed in 0..200 {
            check(7, 16, Oracle::new(seed, 2));
        }
    }

    /// Dependencies on top of aborts: about a third of transactions block on
    /// an unfinished predecessor, are parked, and must be resumed when it
    /// completes — including the race where it finishes between the read and
    /// the parking, which add_dependency must turn into an immediate retry.
    #[test]
    fn stress_with_dependencies() {
        for seed in 0..40 {
            for threads in [2, 4, 8, 16] {
                for max_aborts in [0, 2] {
                    check(200, threads, Oracle::blocking(seed, max_aborts));
                }
            }
        }
    }

    #[test]
    fn dependencies_are_actually_exercised() {
        let (_, events) = run(300, 8, Oracle::blocking(3, 1));
        let blocked = events
            .iter()
            .filter(|e| matches!(e, Event::Blocked { .. }))
            .count();
        assert!(
            blocked > 10,
            "only {blocked} blocks: the dependency path is not being tested"
        );
    }
}
