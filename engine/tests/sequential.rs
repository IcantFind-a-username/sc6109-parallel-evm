//! The sequential baseline, and the workload generator that feeds it.
//!
//! Everything downstream is measured against these results, so they have to be
//! right before anything parallel is written.

use parevm::sched::Scheduler;
use parevm::workload::{Distribution, TransferConfig, TransferWorkload};
use parevm::{SchedulerConfig, SequentialScheduler};
use revm::primitives::U256;

fn config(accounts: usize, transactions: usize, recipients: Distribution) -> TransferConfig {
    TransferConfig {
        accounts,
        transactions,
        recipients,
        ..Default::default()
    }
}

#[test]
fn workload_generation_is_deterministic() {
    let cfg = config(50, 200, Distribution::Uniform);
    let a = TransferWorkload::generate(&cfg, 42);
    let b = TransferWorkload::generate(&cfg, 42);

    assert_eq!(a.txs.len(), b.txs.len());
    for (i, (x, y)) in a.txs.iter().zip(b.txs.iter()).enumerate() {
        assert_eq!(x.caller, y.caller, "tx {i} sender differs");
        assert_eq!(x.kind, y.kind, "tx {i} recipient differs");
        assert_eq!(x.nonce, y.nonce, "tx {i} nonce differs");
    }
}

#[test]
fn different_seeds_give_different_workloads() {
    let cfg = config(50, 200, Distribution::Uniform);
    let a = TransferWorkload::generate(&cfg, 1);
    let b = TransferWorkload::generate(&cfg, 2);
    assert!(
        a.txs
            .iter()
            .zip(b.txs.iter())
            .any(|(x, y)| x.caller != y.caller),
        "two seeds produced identical senders"
    );
}

#[test]
fn sequential_execution_conserves_value() {
    let cfg = config(100, 500, Distribution::Uniform);
    let workload = TransferWorkload::generate(&cfg, 7);
    let before: U256 = workload
        .base
        .accounts()
        .map(|(_, info)| info.balance)
        .fold(U256::ZERO, |a, b| a + b);

    let outcome = SequentialScheduler::new().execute_block(
        &workload.txs,
        &workload.base,
        &SchedulerConfig::default(),
    );

    let after: U256 = outcome
        .state
        .accounts
        .values()
        .map(|a| a.balance)
        .fold(U256::ZERO, |a, b| a + b);

    assert_eq!(
        before, after,
        "value must be conserved: transfers move balance, they do not create it \
         (gas_price is zero in this workload, so no fees leave the account set)"
    );
}

#[test]
fn every_transaction_executes_exactly_once() {
    let cfg = config(100, 300, Distribution::Uniform);
    let workload = TransferWorkload::generate(&cfg, 11);
    let outcome = SequentialScheduler::new().execute_block(
        &workload.txs,
        &workload.base,
        &SchedulerConfig::default(),
    );

    assert_eq!(outcome.stats.transactions, 300);
    assert_eq!(outcome.stats.executions, 300);
    assert_eq!(outcome.stats.aborts, 0, "sequential execution never aborts");
    assert_eq!(
        outcome.stats.reverted, 0,
        "funded transfers must not revert"
    );
    assert_eq!(outcome.stats.execution_factor(), 1.0);
    assert!(outcome.stats.executions_per_tx.iter().all(|&n| n == 1));
}

#[test]
fn nonces_advance_per_sender() {
    let cfg = config(20, 200, Distribution::Uniform);
    let workload = TransferWorkload::generate(&cfg, 3);
    let outcome = SequentialScheduler::new().execute_block(
        &workload.txs,
        &workload.base,
        &SchedulerConfig::default(),
    );

    let sent: usize = outcome
        .state
        .accounts
        .values()
        .map(|a| a.nonce as usize)
        .sum();
    assert_eq!(
        sent, 200,
        "every transaction advances exactly one sender nonce"
    );
}

#[test]
fn sequential_is_reproducible() {
    let cfg = config(80, 400, Distribution::Zipf { s: 1.0 });
    let workload = TransferWorkload::generate(&cfg, 99);
    let sched = SequentialScheduler::new();
    let a = sched.execute_block(&workload.txs, &workload.base, &SchedulerConfig::default());
    let b = sched.execute_block(&workload.txs, &workload.base, &SchedulerConfig::default());
    assert!(
        a.state.diff(&b.state).is_empty(),
        "the same block executed twice must give the same state"
    );
}

/// The conflict knob has to actually move conflict density, or the headline
/// figure measures nothing.
#[test]
fn zipf_concentrates_recipients() {
    let uniform = TransferWorkload::generate(&config(200, 2_000, Distribution::Uniform), 5);
    let skewed = TransferWorkload::generate(&config(200, 2_000, Distribution::Zipf { s: 1.5 }), 5);

    let hottest = |w: &parevm::Workload| -> usize {
        let mut counts = std::collections::HashMap::new();
        for tx in &w.txs {
            if let revm::primitives::TxKind::Call(to) = tx.kind {
                *counts.entry(to).or_insert(0usize) += 1;
            }
        }
        counts.values().copied().max().unwrap_or(0)
    };

    let u = hottest(&uniform);
    let z = hottest(&skewed);
    assert!(
        z > u * 4,
        "zipf should concentrate far more than uniform: hottest recipient {z} vs {u}"
    );
}

/// A scheduler compared against itself must agree. Proves the harness reports
/// agreement correctly before it is used to judge anything else.
#[test]
fn differential_harness_detects_agreement() {
    let workload = TransferWorkload::generate(&config(50, 200, Distribution::Uniform), 13);
    parevm::assert_agree(
        &SequentialScheduler::new(),
        &SequentialScheduler::new(),
        &workload,
        &SchedulerConfig::default(),
    );
}

/// And it must detect disagreement, or it would pass silently forever. A
/// deliberately broken scheduler that drops the last transaction stands in for
/// a real bug.
#[test]
fn differential_harness_detects_disagreement() {
    use parevm::outcome::BlockOutcome;
    use parevm::state::BaseState;
    use revm::context::TxEnv;

    struct DropsLastTx;
    impl Scheduler for DropsLastTx {
        fn name(&self) -> &'static str {
            "drops-last-tx"
        }
        fn execute_block(
            &self,
            txs: &[TxEnv],
            base: &BaseState,
            config: &SchedulerConfig,
        ) -> BlockOutcome {
            let truncated = &txs[..txs.len().saturating_sub(1)];
            SequentialScheduler::new().execute_block(truncated, base, config)
        }
    }

    let workload = TransferWorkload::generate(&config(50, 100, Distribution::Uniform), 17);
    let comparison = parevm::compare(
        &SequentialScheduler::new(),
        &DropsLastTx,
        &workload,
        &SchedulerConfig::default(),
    );
    assert!(
        !comparison.agrees(),
        "the harness failed to notice a dropped transaction"
    );
    let report = comparison.report(&workload, "sequential", "drops-last-tx");
    assert!(report.contains("seed 17"), "report must locate the failure");
}

/// Generated accounts must not land in the precompile range.
///
/// They did once. Account `i` was addressed as `i + 1`, so the first sixteen
/// accounts were precompiles: `0x01` ecrecover through `0x0a` KZG point
/// evaluation. Transfers to them invoked the precompile rather than moving
/// value, and halted with `OutOfGas(Precompile)` — 15% of the block, silently
/// doing something other than what the workload claimed. See E5.
#[test]
fn generated_addresses_avoid_precompiles() {
    use parevm::workload::is_outside_precompile_range;
    use revm::primitives::TxKind;

    let workload = TransferWorkload::generate(&config(300, 300, Distribution::Uniform), 1);
    for (address, _) in workload.base.accounts() {
        assert!(
            is_outside_precompile_range(address),
            "generated account {address} is in the precompile range"
        );
    }
    for (i, tx) in workload.txs.iter().enumerate() {
        assert!(is_outside_precompile_range(&tx.caller), "tx {i} sender");
        if let TxKind::Call(to) = tx.kind {
            assert!(is_outside_precompile_range(&to), "tx {i} recipient");
        }
    }
}
