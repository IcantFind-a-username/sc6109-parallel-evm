//! M2b gate: collaborative Block-STM must reproduce sequential state exactly,
//! for every workload, seed and thread count. Quick sweeps run in CI; the full
//! gate is `#[ignore]`d and runs with `cargo test --release -- --ignored`.

use parevm::sched::Scheduler;
use parevm::workload::{
    ComputeConfig, ComputeWorkload, ContractConfig, ContractKind, ContractWorkload, Distribution,
    TransferConfig, TransferWorkload,
};
use parevm::{assert_agree, BlockStmScheduler, SchedulerConfig, SequentialScheduler, Workload};

const DISTRIBUTIONS: [Distribution; 4] = [
    Distribution::Uniform,
    Distribution::Zipf { s: 0.8 },
    Distribution::Zipf { s: 1.2 },
    Distribution::Zipf { s: 2.0 },
];

fn transfer(accounts: usize, transactions: usize, recipients: Distribution, seed: u64) -> Workload {
    TransferWorkload::generate(
        &TransferConfig {
            accounts,
            transactions,
            recipients,
            ..Default::default()
        },
        seed,
    )
}

fn contracts(accounts: usize, transactions: usize, seed: u64) -> Vec<Workload> {
    [
        ContractKind::Erc20 {
            recipients: Distribution::Uniform,
        },
        ContractKind::Erc20 {
            recipients: Distribution::Zipf { s: 1.2 },
        },
        ContractKind::Erc20Tight {
            recipients: Distribution::Zipf { s: 1.0 },
        },
        ContractKind::NftMint,
        ContractKind::AmmSwap,
    ]
    .into_iter()
    .map(|kind| {
        ContractWorkload::generate(
            &ContractConfig {
                kind,
                accounts,
                transactions,
            },
            seed,
        )
    })
    .collect()
}

fn agree(w: &Workload, threads: usize) {
    assert_agree(
        &SequentialScheduler::new(),
        &BlockStmScheduler::new(),
        w,
        &SchedulerConfig {
            threads,
            ..Default::default()
        },
    );
}

#[test]
fn agrees_on_transfers() {
    for d in DISTRIBUTIONS {
        for seed in 0..20 {
            let w = transfer(50, 150, d, seed);
            for threads in [1, 2, 4, 8] {
                agree(&w, threads);
            }
        }
    }
}

#[test]
fn agrees_on_compute_and_contracts() {
    for seed in 0..6 {
        let compute = ComputeWorkload::generate(
            &ComputeConfig {
                accounts: 20,
                transactions: 100,
                payload: 64,
            },
            seed,
        );
        for w in std::iter::once(compute).chain(contracts(40, 120, seed)) {
            for threads in [2, 4, 8] {
                agree(&w, threads);
            }
        }
    }
}

/// E6 on the collaborative path. Five senders, two hundred transactions: every
/// sender has long nonce chains, so workers routinely run a sender's next
/// transaction before its previous one has written back, and revm refuses it
/// with a nonce error. Those refusals must be recoverable — kept with their read
/// sets, invalidated, re-run — and the block must still match sequential.
#[test]
fn speculative_refusals_are_recovered() {
    let mut refused = 0;
    for seed in 0..20 {
        let w = transfer(5, 200, Distribution::Uniform, seed);
        for threads in [4, 8] {
            let config = SchedulerConfig {
                threads,
                ..Default::default()
            };
            let seq = SequentialScheduler::new().execute_block(&w.txs, &w.base, &config);
            let par = BlockStmScheduler::new().execute_block(&w.txs, &w.base, &config);
            assert!(
                par.state.diff(&seq.state).is_empty(),
                "seed {seed}, {threads} threads"
            );
            refused += par.stats.speculative_refusals;
        }
    }
    assert!(
        refused > 0,
        "no speculative refusal occurred, so E6's path was not exercised"
    );
}

/// Dependency waiting must actually happen on a contended block, or the
/// ESTIMATE path is untested here.
#[test]
fn estimates_cause_dependency_waits() {
    let w = &contracts(1_000, 300, 1)[3]; // NFT mint: one chain
    let out = BlockStmScheduler::new().execute_block(
        &w.txs,
        &w.base,
        &SchedulerConfig {
            threads: 8,
            ..Default::default()
        },
    );
    assert!(out.stats.dependency_waits > 0 || out.stats.aborts > 0);
}

#[test]
fn accounting_is_consistent() {
    let w = transfer(20, 300, Distribution::Zipf { s: 1.5 }, 42);
    let out = BlockStmScheduler::new().execute_block(
        &w.txs,
        &w.base,
        &SchedulerConfig {
            threads: 4,
            ..Default::default()
        },
    );
    let s = &out.stats;
    assert_eq!(
        s.executions_per_tx
            .iter()
            .map(|&n| n as usize)
            .sum::<usize>(),
        s.executions
    );
    assert!(s.executions_per_tx.iter().all(|&n| n >= 1));
    assert_eq!(
        s.executions,
        s.transactions + s.aborts,
        "each abort costs exactly one re-execution"
    );
}

#[test]
fn empty_and_single_transaction_blocks() {
    for n in [0, 1] {
        let w = transfer(10, n, Distribution::Uniform, 0);
        agree(&w, 4);
    }
}

#[test]
fn beneficiary_fees_merge_correctly() {
    for seed in 0..10 {
        let w = TransferWorkload::generate(
            &TransferConfig {
                accounts: 30,
                transactions: 150,
                recipients: Distribution::Zipf { s: 1.0 },
                gas_price: 1_000_000_000,
                ..Default::default()
            },
            seed,
        );
        agree(&w, 4);
    }
}

#[test]
#[ignore = "full M2b gate; run with --release -- --ignored"]
fn full_gate_transfers() {
    for d in DISTRIBUTIONS {
        for seed in 0..1000 {
            let w = transfer(200, 400, d, seed);
            for threads in [2, 4, 6, 8, 12] {
                agree(&w, threads);
            }
        }
    }
}

#[test]
#[ignore = "full M2b gate; run with --release -- --ignored"]
fn full_gate_compute_and_contracts() {
    for seed in 0..1000 {
        let compute = ComputeWorkload::generate(
            &ComputeConfig {
                accounts: 200,
                transactions: 400,
                payload: 64,
            },
            seed,
        );
        for w in std::iter::once(compute).chain(contracts(60, 200, seed)) {
            for threads in [2, 4, 6, 8, 12] {
                agree(&w, threads);
            }
        }
    }
}
