//! M3: the static scheduler must reproduce sequential state exactly. In debug
//! builds it also asserts, per transaction, that the declared access set
//! covered everything the transaction actually read and wrote.

use parevm::sched::static_sched::levels;
use parevm::sched::Scheduler;
use parevm::workload::{
    profile, ComputeConfig, ComputeWorkload, ContractConfig, ContractKind, ContractWorkload,
    Distribution, TransferConfig, TransferWorkload,
};
use parevm::{
    assert_agree, Granularity, SchedulerConfig, SequentialScheduler, StaticScheduler, Workload,
};
use revm::context::BlockEnv;

fn all(seed: u64) -> Vec<Workload> {
    let mut out: Vec<Workload> = [
        ContractKind::Erc20 {
            recipients: Distribution::Uniform,
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
                accounts: 40,
                transactions: 120,
            },
            seed,
        )
    })
    .collect();
    for d in [
        Distribution::Uniform,
        Distribution::Zipf { s: 1.2 },
        Distribution::Zipf { s: 2.0 },
    ] {
        out.push(TransferWorkload::generate(
            &TransferConfig {
                accounts: 50,
                transactions: 150,
                recipients: d,
                gas_price: if seed.is_multiple_of(2) {
                    0
                } else {
                    1_000_000_000
                },
                ..Default::default()
            },
            seed,
        ));
    }
    out.push(ComputeWorkload::generate(
        &ComputeConfig {
            accounts: 20,
            transactions: 100,
            payload: 64,
        },
        seed,
    ));
    out
}

#[test]
fn agrees_with_sequential() {
    for seed in 0..10 {
        for w in all(seed) {
            for threads in [1, 2, 4, 8] {
                for granularity in [Granularity::Slot, Granularity::Account] {
                    assert_agree(
                        &SequentialScheduler::new(),
                        &StaticScheduler::new(),
                        &w,
                        &SchedulerConfig {
                            threads,
                            granularity,
                            ..Default::default()
                        },
                    );
                }
            }
        }
    }
}

/// NFT minting is one chain: every mint gets its own level.
#[test]
fn nft_mint_needs_one_level_per_transaction() {
    let w = ContractWorkload::generate(
        &ContractConfig {
            kind: ContractKind::NftMint,
            accounts: 1_000,
            transactions: 200,
        },
        1,
    );
    let plan = levels(&profile(
        &w.txs,
        &w.base,
        &BlockEnv::default(),
        Granularity::Slot,
    ));
    assert_eq!(plan.len(), 200);
}

/// Transfers among a large population are nearly independent: a handful of
/// levels holds the whole block.
#[test]
fn sparse_transfers_need_few_levels() {
    let w = TransferWorkload::generate(
        &TransferConfig {
            accounts: 100_000,
            transactions: 500,
            ..Default::default()
        },
        2,
    );
    let plan = levels(&profile(
        &w.txs,
        &w.base,
        &BlockEnv::default(),
        Granularity::Slot,
    ));
    assert!(plan.len() <= 4, "{} levels", plan.len());
    assert_eq!(plan.iter().map(Vec::len).sum::<usize>(), 500);
}

/// Levels respect write-after-read: a later writer never shares a level with,
/// or precedes, an earlier reader of the same location.
#[test]
fn levels_respect_every_conflict_kind() {
    use parevm::workload::AccessSet;
    use parevm::Key;
    use revm::primitives::Address;
    let k = Key::Basic(Address::with_last_byte(0xAA));
    let read = AccessSet {
        reads: [k].into(),
        writes: Default::default(),
    };
    let write = AccessSet {
        reads: Default::default(),
        writes: [k].into(),
    };
    // read, then write: WAR — the writer must come after.
    assert_eq!(
        levels(&[read.clone(), write.clone()]),
        vec![vec![0], vec![1]]
    );
    // write, then read: RAW.
    assert_eq!(
        levels(&[write.clone(), read.clone()]),
        vec![vec![0], vec![1]]
    );
    // write, write: WAW.
    assert_eq!(
        levels(&[write.clone(), write.clone()]),
        vec![vec![0], vec![1]]
    );
    // read, read: no conflict.
    assert_eq!(levels(&[read.clone(), read.clone()]), vec![vec![0, 1]]);
}

#[test]
fn preparation_is_recorded_separately() {
    let w = &all(0)[4];
    let out = StaticScheduler::new().execute_block(&w.txs, &w.base, &SchedulerConfig::default());
    assert!(out.stats.preparation > std::time::Duration::ZERO);
    assert_eq!(out.stats.aborts, 0);
    assert_eq!(out.stats.executions, w.txs.len());
}

#[test]
#[ignore = "full M3 gate; run with --release -- --ignored"]
fn full_gate() {
    for seed in 0..500 {
        for w in all(seed) {
            for threads in [2, 4, 6, 8, 12] {
                assert_agree(
                    &SequentialScheduler::new(),
                    &StaticScheduler::new(),
                    &w,
                    &SchedulerConfig {
                        threads,
                        ..Default::default()
                    },
                );
            }
        }
    }
}
