//! Static scheduling from declared access sets — an EIP-7928 prototype (M3).
//!
//! The block's access sets are derived first, by the same sequential profiling
//! a block builder would do (`workload::profile`, DESIGN §7.2). From them each
//! transaction gets a **level**: one past the highest level of any earlier
//! transaction it conflicts with. Two transactions conflict when one writes a
//! location the other reads or writes — read-after-write, write-after-write,
//! and write-after-read, since a later writer must not overtake an earlier
//! reader.
//!
//! Transactions within a level are pairwise conflict-free, so the level runs in
//! parallel against the state all lower levels committed, and every
//! transaction sees exactly the values sequential execution would have shown
//! it for everything it reads. Then the level's writes are committed and the
//! next level starts. No speculation, no aborts, no multi-version store.
//!
//! The honest limitation, stated where the numbers are produced: the measured
//! time **excludes** deriving the access sets, which costs a full sequential
//! execution. It is recorded separately as `ExecStats::preparation`. Under
//! EIP-7928 that cost is paid once by the builder and shared by every
//! validator, which is the whole case for the EIP; these numbers measure the
//! validator's side only.

use super::{assert_beneficiary_only_paid, Scheduler, SchedulerConfig};
use crate::exec::execute;
use crate::outcome::{BlockOutcome, ExecStats};
use crate::sched::sequential::snapshot;
use crate::state::{BaseState, SimpleState, SimpleView, StateError, StateView};
use crate::types::{Key, ReadOrigin, TxIdx};
use crate::workload::analysis::{profile, AccessSet};
use rayon::prelude::*;
use revm::context::TxEnv;
use revm::primitives::{Address, StorageKey, StorageValue, B256, U256};
use revm::state::{AccountInfo, Bytecode};
use std::collections::HashMap;
use std::time::Instant;

#[derive(Clone, Copy, Debug, Default)]
pub struct StaticScheduler;

impl StaticScheduler {
    pub fn new() -> Self {
        Self
    }
}

/// Assigns each transaction the lowest level after every earlier transaction
/// it conflicts with. Returns the levels in order, each a list of indices.
pub fn levels(sets: &[AccessSet]) -> Vec<Vec<TxIdx>> {
    let mut last_write: HashMap<Key, usize> = HashMap::new();
    let mut last_read: HashMap<Key, usize> = HashMap::new();
    let mut out: Vec<Vec<TxIdx>> = Vec::new();
    for (j, set) in sets.iter().enumerate() {
        let after = |m: &HashMap<Key, usize>, k: &Key| m.get(k).map_or(0, |l| l + 1);
        let level = set
            .reads
            .iter()
            .map(|k| after(&last_write, k))
            .chain(
                set.writes
                    .iter()
                    .map(|k| after(&last_write, k).max(after(&last_read, k))),
            )
            .max()
            .unwrap_or(0);
        for k in &set.writes {
            last_write.insert(*k, level);
        }
        for k in &set.reads {
            let r = last_read.entry(*k).or_insert(level);
            *r = (*r).max(level);
        }
        if out.len() <= level {
            out.resize_with(level + 1, Vec::new);
        }
        out[level].push(j);
    }
    out
}

/// Committed state, with the beneficiary always served from base so each
/// transaction's fee comes out as a delta against it (D4).
struct LevelView<'a> {
    inner: SimpleView<'a>,
    base: &'a BaseState,
    beneficiary: Address,
}

impl StateView for LevelView<'_> {
    type Error = StateError;
    fn basic(&self, address: Address) -> Result<(Option<AccountInfo>, ReadOrigin), Self::Error> {
        if address == self.beneficiary {
            return self.base.basic(address);
        }
        self.inner.basic(address)
    }
    fn code_by_hash(&self, code_hash: B256) -> Result<(Bytecode, ReadOrigin), Self::Error> {
        self.inner.code_by_hash(code_hash)
    }
    fn storage(
        &self,
        address: Address,
        index: StorageKey,
    ) -> Result<(StorageValue, ReadOrigin), Self::Error> {
        self.inner.storage(address, index)
    }
    fn block_hash(&self, number: u64) -> Result<(B256, ReadOrigin), Self::Error> {
        self.inner.block_hash(number)
    }
}

impl Scheduler for StaticScheduler {
    fn name(&self) -> &'static str {
        "static"
    }

    fn execute_block(
        &self,
        txs: &[TxEnv],
        base: &BaseState,
        config: &SchedulerConfig,
    ) -> BlockOutcome {
        let beneficiary = config.block.beneficiary;
        assert_beneficiary_only_paid(txs, beneficiary);

        // The builder's side: derive the access list. Timed separately.
        let prep = Instant::now();
        let sets = profile(txs, base, &config.block, config.granularity);
        let plan = levels(&sets);
        let preparation = prep.elapsed();

        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(config.threads)
            .build()
            .expect("thread pool");
        let mut state = SimpleState::new(base.clone());
        let before_fees = base.account(beneficiary).cloned();
        let mut fees = U256::ZERO;
        let mut beneficiary_touched = false;
        let mut reverted = 0;

        let start = Instant::now();
        for level in &plan {
            let outputs: Vec<_> = pool.install(|| {
                level
                    .par_iter()
                    .map(|&j| {
                        let view = LevelView {
                            inner: state.view(),
                            base,
                            beneficiary,
                        };
                        (
                            j,
                            execute(view, txs[j].clone(), &config.block, config.granularity),
                        )
                    })
                    .collect()
            });
            for (j, out) in outputs {
                let mut done = out
                    .outcome
                    .unwrap_or_else(|e| panic!("transaction {j} could not be executed: {e:?}"));
                #[cfg(debug_assertions)]
                check_declared(
                    j,
                    &sets[j],
                    &out.reads,
                    &done.writes,
                    &state,
                    beneficiary,
                    config.granularity,
                );
                if !done.is_success() {
                    reverted += 1;
                }
                if let Some(acc) = done.writes.remove(&beneficiary) {
                    if acc.is_touched() {
                        beneficiary_touched = true;
                        let base_balance = before_fees.as_ref().map_or(U256::ZERO, |i| i.balance);
                        fees += acc.info.balance - base_balance;
                    }
                }
                state.commit(done.writes);
            }
        }
        if beneficiary_touched {
            let mut info = before_fees.unwrap_or_default();
            info.balance += fees;
            state.set_account(beneficiary, info);
        }
        let wall_clock = start.elapsed();

        let n = txs.len();
        BlockOutcome {
            state: snapshot(&state),
            stats: ExecStats {
                transactions: n,
                executions: n,
                reverted,
                threads: config.threads,
                rounds: plan.len(),
                wall_clock,
                preparation,
                executions_per_tx: vec![1; n],
                ..Default::default()
            },
        }
    }
}

/// In debug builds, proves the declared access set covered what the
/// transaction actually did. Static scheduling is only correct if it does, and
/// the check is what licenses the claim — it is compiled out of benchmarks.
#[cfg(debug_assertions)]
fn check_declared(
    j: TxIdx,
    declared: &AccessSet,
    reads: &crate::types::ReadSet,
    writes: &revm::state::EvmState,
    state: &SimpleState,
    beneficiary: Address,
    granularity: crate::types::Granularity,
) {
    for (k, _) in reads.mutable() {
        assert!(
            *k == Key::Basic(beneficiary) || declared.reads.contains(k),
            "tx {j} read {k:?}, which its declared access set does not list"
        );
    }
    let actual = crate::workload::analysis::write_keys(
        writes,
        |a| state.account(a),
        beneficiary,
        granularity,
    );
    assert!(
        actual.is_subset(&declared.writes),
        "tx {j} wrote {:?}, beyond its declared access set",
        actual.difference(&declared.writes).collect::<Vec<_>>()
    );
}
