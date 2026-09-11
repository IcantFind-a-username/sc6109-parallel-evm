//! M2a gate: round-based optimistic execution must reproduce sequential state
//! exactly, for every seed, every thread count, every conflict level.
//!
//! Concurrency bugs are probabilistic, so a handful of runs proves nothing.
//! The quick sweep here runs in CI on every push; the full 1000-seed sweep the
//! roadmap gate calls for is `#[ignore]`d and run with
//! `cargo test --release -- --ignored`.

use parevm::sched::Scheduler;
use parevm::workload::{
    ComputeConfig, ComputeWorkload, Distribution, TransferConfig, TransferWorkload,
};
use parevm::{assert_agree, RoundScheduler, SchedulerConfig, SequentialScheduler};

const DISTRIBUTIONS: [Distribution; 4] = [
    Distribution::Uniform,
    Distribution::Zipf { s: 0.8 },
    Distribution::Zipf { s: 1.2 },
    Distribution::Zipf { s: 2.0 },
];

fn sweep(seeds: std::ops::Range<u64>, threads: &[usize], accounts: usize, transactions: usize) {
    for &recipients in &DISTRIBUTIONS {
        let cfg = TransferConfig {
            accounts,
            transactions,
            recipients,
            ..Default::default()
        };
        for seed in seeds.clone() {
            let workload = TransferWorkload::generate(&cfg, seed);
            for &t in threads {
                let config = SchedulerConfig {
                    threads: t,
                    ..Default::default()
                };
                assert_agree(
                    &SequentialScheduler::new(),
                    &RoundScheduler::new(),
                    &workload,
                    &config,
                );
            }
        }
    }
}

#[test]
fn agrees_with_sequential_quick_sweep() {
    sweep(0..25, &[1, 2, 4, 8], 50, 120);
}

/// Few accounts, many transactions: nearly every transaction conflicts with
/// something, and many senders send repeatedly, so stale-nonce refusals are
/// frequent mid-flight. This is the case E6 was about.
#[test]
fn agrees_under_heavy_contention() {
    sweep(100..120, &[2, 4, 8], 5, 200);
}

#[test]
fn single_transaction_block() {
    sweep(0..5, &[1, 4], 10, 1);
}

#[test]
fn empty_block() {
    let workload = TransferWorkload::generate(
        &TransferConfig {
            accounts: 10,
            transactions: 0,
            ..Default::default()
        },
        0,
    );
    let out = RoundScheduler::new().execute_block(
        &workload.txs,
        &workload.base,
        &SchedulerConfig::default(),
    );
    assert_eq!(out.stats.executions, 0);
    assert_eq!(out.stats.rounds, 0);
}

/// With fees switched on, the beneficiary exemption does real work: its final
/// balance must equal the sum sequential execution accumulates one transaction
/// at a time.
#[test]
fn beneficiary_fees_merge_correctly() {
    for seed in 0..10 {
        let workload = TransferWorkload::generate(
            &TransferConfig {
                accounts: 30,
                transactions: 150,
                recipients: Distribution::Zipf { s: 1.0 },
                gas_price: 1_000_000_000,
                ..Default::default()
            },
            seed,
        );
        let config = SchedulerConfig {
            threads: 4,
            ..Default::default()
        };
        assert_agree(
            &SequentialScheduler::new(),
            &RoundScheduler::new(),
            &workload,
            &config,
        );
    }
}

/// Every transaction must end validated exactly once more than it aborted.
#[test]
fn accounting_is_consistent() {
    let workload = TransferWorkload::generate(
        &TransferConfig {
            accounts: 20,
            transactions: 300,
            recipients: Distribution::Zipf { s: 1.5 },
            ..Default::default()
        },
        42,
    );
    let out = RoundScheduler::new().execute_block(
        &workload.txs,
        &workload.base,
        &SchedulerConfig {
            threads: 4,
            ..Default::default()
        },
    );
    let s = &out.stats;
    assert_eq!(s.executions, s.transactions + s.aborts);
    assert_eq!(
        s.executions_per_tx
            .iter()
            .map(|&n| n as usize)
            .sum::<usize>(),
        s.executions
    );
    assert!(s.executions_per_tx.iter().all(|&n| n >= 1));
    assert!(
        s.rounds <= s.transactions,
        "at least one transaction settles per round"
    );
}

#[test]
#[should_panic(expected = "account granularity is unsound")]
fn refuses_account_granularity() {
    let workload = TransferWorkload::generate(&TransferConfig::default(), 0);
    RoundScheduler::new().execute_block(
        &workload.txs,
        &workload.base,
        &SchedulerConfig {
            granularity: parevm::Granularity::Account,
            ..Default::default()
        },
    );
}

/// D16's compute workload goes through the same gate as everything else.
/// Few senders, so nonce chains are common and speculation has work to do.
#[test]
fn agrees_on_compute_workloads() {
    for payload in [0, 1_024, 8_192] {
        for seed in 0..10 {
            let workload = ComputeWorkload::generate(
                &ComputeConfig {
                    accounts: 20,
                    transactions: 80,
                    payload,
                },
                seed,
            );
            for threads in [2, 4, 8] {
                let config = SchedulerConfig {
                    threads,
                    ..Default::default()
                };
                assert_agree(
                    &SequentialScheduler::new(),
                    &RoundScheduler::new(),
                    &workload,
                    &config,
                );
            }
        }
    }
}

#[test]
#[ignore = "full M2a gate sweep; run with --release -- --ignored"]
fn agrees_with_sequential_full_gate() {
    sweep(0..1000, &[2, 4, 6, 8, 12], 200, 400);
}
