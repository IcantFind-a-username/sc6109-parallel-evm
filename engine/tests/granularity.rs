//! Account-granularity conflict detection (D14), the D9 experiment's second
//! setting. Coarser detection may abort more — that is the point of measuring
//! it — but it must never change the result.

use parevm::sched::Scheduler;
use parevm::workload::{
    ContractConfig, ContractKind, ContractWorkload, Distribution, TransferConfig, TransferWorkload,
};
use parevm::{
    assert_agree, BlockStmScheduler, Granularity, RoundScheduler, SchedulerConfig,
    SequentialScheduler, Workload,
};

fn config(threads: usize, granularity: Granularity) -> SchedulerConfig {
    SchedulerConfig {
        threads,
        granularity,
        ..Default::default()
    }
}

fn workloads(seed: u64) -> Vec<Workload> {
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
    out.push(TransferWorkload::generate(
        &TransferConfig {
            accounts: 30,
            transactions: 120,
            recipients: Distribution::Zipf { s: 1.0 },
            ..Default::default()
        },
        seed,
    ));
    out
}

/// Both parallel engines, coarse detection, many seeds: the result must still
/// be the sequential result. This is the check E7's unsound version would fail.
#[test]
fn account_granularity_agrees_with_sequential() {
    for seed in 0..10 {
        for w in workloads(seed) {
            for threads in [2, 4, 8] {
                let c = config(threads, Granularity::Account);
                assert_agree(&SequentialScheduler::new(), &RoundScheduler::new(), &w, &c);
                assert_agree(
                    &SequentialScheduler::new(),
                    &BlockStmScheduler::new(),
                    &w,
                    &c,
                );
            }
        }
    }
}

/// Every ERC-20 balance lives in the token contract's storage, so under
/// account granularity every transfer conflicts with every other one through
/// that one account — false conflicts in their purest form. Uniform transfers
/// between a large population are nearly independent at slot granularity.
#[test]
fn account_granularity_turns_erc20_into_one_hot_spot() {
    let w = ContractWorkload::generate(
        &ContractConfig {
            kind: ContractKind::Erc20 {
                recipients: Distribution::Uniform,
            },
            accounts: 100_000,
            transactions: 400,
        },
        3,
    );
    let slot = RoundScheduler::new().execute_block(&w.txs, &w.base, &config(4, Granularity::Slot));
    let account =
        RoundScheduler::new().execute_block(&w.txs, &w.base, &config(4, Granularity::Account));
    assert!(
        slot.stats.rounds <= 4,
        "slot granularity: {} rounds",
        slot.stats.rounds
    );
    assert!(
        account.stats.rounds > 50,
        "account granularity should serialise through the token account: {} rounds",
        account.stats.rounds
    );
    assert!(account.state.diff(&slot.state).is_empty());
}

#[test]
#[ignore = "full gate; run with --release -- --ignored"]
fn account_granularity_full_gate() {
    for seed in 0..300 {
        for w in workloads(seed) {
            for threads in [2, 4, 6, 8, 12] {
                let c = config(threads, Granularity::Account);
                assert_agree(&SequentialScheduler::new(), &RoundScheduler::new(), &w, &c);
                assert_agree(
                    &SequentialScheduler::new(),
                    &BlockStmScheduler::new(),
                    &w,
                    &c,
                );
            }
        }
    }
}

/// Hammers both parallel engines under account granularity with oversubscribed
/// threads, where a value and its account-level version can be read on either
/// side of a concurrent write. Written to reproduce an intermittent failure:
/// a refused transaction surviving validation because it read an old value
/// paired with a new version.
#[test]
#[ignore = "stress; run with --release -- --ignored"]
fn account_granularity_stress() {
    for seed in 0..400 {
        for w in workloads(seed) {
            for threads in [8, 12, 16] {
                let c = config(threads, Granularity::Account);
                assert_agree(&SequentialScheduler::new(), &RoundScheduler::new(), &w, &c);
                assert_agree(
                    &SequentialScheduler::new(),
                    &BlockStmScheduler::new(),
                    &w,
                    &c,
                );
            }
        }
    }
}
