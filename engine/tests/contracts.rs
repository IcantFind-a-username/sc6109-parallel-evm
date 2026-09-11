//! Contract workloads: the storage layouts the generator assumes, what each
//! workload actually does, and agreement with the sequential engine.

use parevm::exec::execute;
use parevm::sched::Scheduler;
use parevm::workload::contract::{call, mapping_slot, word, COLLECTIBLE, POOL, TOKEN};
use parevm::workload::{analyse, ContractConfig, ContractKind, ContractWorkload, Distribution};
use parevm::{
    assert_agree, Granularity, RoundScheduler, SchedulerConfig, SequentialScheduler, Workload,
};
use revm::context::{BlockEnv, TxEnv};
use revm::primitives::{Address, TxKind, U256};

fn workload(kind: ContractKind, accounts: usize, transactions: usize, seed: u64) -> Workload {
    ContractWorkload::generate(
        &ContractConfig {
            kind,
            accounts,
            transactions,
        },
        seed,
    )
}

/// Calls a view function against base state and decodes a single uint256.
fn view(w: &Workload, to: Address, data: revm::primitives::Bytes) -> U256 {
    let tx = TxEnv::builder()
        .caller(w.txs[0].caller)
        .kind(TxKind::Call(to))
        .data(data)
        .gas_limit(100_000)
        .gas_price(0)
        .nonce(0)
        .build()
        .unwrap();
    let out = execute(&w.base, tx, &BlockEnv::default(), Granularity::Slot)
        .outcome
        .expect("view call refused");
    let bytes = out
        .result
        .output()
        .expect("view call produced no output")
        .clone();
    U256::from_be_slice(&bytes)
}

/// The generator writes Solidity mapping slots itself. If the layout or the
/// slot arithmetic were wrong, balanceOf would read zero.
#[test]
fn seeded_storage_matches_the_solidity_layout() {
    let erc20 = workload(
        ContractKind::Erc20 {
            recipients: Distribution::Uniform,
        },
        10,
        5,
        1,
    );
    let holder = erc20.txs[0].caller;
    assert_eq!(
        view(&erc20, TOKEN, call("balanceOf(address)", &[word(holder)])),
        U256::from(1_000_000_000u64)
    );
    assert_eq!(
        view(&erc20, TOKEN, call("totalSupply()", &[])),
        U256::from(10_000_000_000u64)
    );

    let pool = workload(ContractKind::AmmSwap, 10, 5, 1);
    assert_eq!(
        view(&pool, POOL, call("reserve0()", &[])),
        U256::from(10u128.pow(15))
    );
    assert_eq!(
        view(
            &pool,
            POOL,
            call("balance1(address)", &[word(pool.txs[0].caller)])
        ),
        U256::from(10u128.pow(12))
    );

    let nft = workload(ContractKind::NftMint, 10, 5, 1);
    assert_eq!(
        view(&nft, COLLECTIBLE, call("totalSupply()", &[])),
        U256::ZERO
    );
}

#[test]
fn erc20_transfers_succeed_and_conserve_supply() {
    let w = workload(
        ContractKind::Erc20 {
            recipients: Distribution::Zipf { s: 1.0 },
        },
        200,
        300,
        3,
    );
    let out =
        SequentialScheduler::new().execute_block(&w.txs, &w.base, &SchedulerConfig::default());
    assert_eq!(
        out.stats.reverted, 0,
        "well-funded transfers must not revert"
    );
    let balances: U256 = out
        .state
        .storage
        .iter()
        .filter(|((a, slot), _)| *a == TOKEN && *slot != U256::from(1))
        .map(|(_, v)| *v)
        .fold(U256::ZERO, |a, b| a + b);
    assert_eq!(
        balances,
        U256::from(200u64 * 1_000_000_000),
        "tokens are moved, never created"
    );
}

/// The tight variant exists to make write sets depend on reads. It only does
/// that job if some transfers revert and some succeed.
#[test]
fn tight_erc20_mixes_reverts_and_successes() {
    let w = workload(
        ContractKind::Erc20Tight {
            recipients: Distribution::Zipf { s: 1.0 },
        },
        50,
        300,
        4,
    );
    let out =
        SequentialScheduler::new().execute_block(&w.txs, &w.base, &SchedulerConfig::default());
    assert!(
        out.stats.reverted > 30,
        "only {} reverts",
        out.stats.reverted
    );
    assert!(
        out.stats.reverted < 270,
        "{} of 300 reverted",
        out.stats.reverted
    );
}

/// NFT minting is the total-conflict workload: every mint depends on the last.
#[test]
fn nft_mint_is_one_chain() {
    let w = workload(ContractKind::NftMint, 1_000, 200, 5);
    let p = analyse(&w, &BlockEnv::default());
    assert_eq!(
        p.critical_path, 200,
        "every mint reads the totalSupply its predecessor wrote"
    );
    let out =
        SequentialScheduler::new().execute_block(&w.txs, &w.base, &SchedulerConfig::default());
    assert_eq!(out.stats.reverted, 0);
    assert_eq!(
        out.state.storage[&(COLLECTIBLE, U256::ZERO)],
        U256::from(200)
    );
}

#[test]
fn amm_swaps_are_one_chain_and_keep_k() {
    let w = workload(ContractKind::AmmSwap, 1_000, 200, 6);
    assert_eq!(analyse(&w, &BlockEnv::default()).critical_path, 200);
    let out =
        SequentialScheduler::new().execute_block(&w.txs, &w.base, &SchedulerConfig::default());
    assert_eq!(out.stats.reverted, 0);
    let r0 = out.state.storage[&(POOL, U256::ZERO)];
    let r1 = out.state.storage[&(POOL, U256::from(1))];
    assert!(
        r0 * r1 >= U256::from(10u128.pow(30)),
        "the constant product must not shrink"
    );
}

/// The contract account is touched by every transaction and changed by none.
/// Before D18 that made every contract workload one chain; ERC-20 transfers
/// between distinct pairs must be independent.
#[test]
fn contract_account_is_not_a_hot_spot() {
    let w = workload(
        ContractKind::Erc20 {
            recipients: Distribution::Uniform,
        },
        100_000,
        500,
        7,
    );
    let p = analyse(&w, &BlockEnv::default());
    assert!(
        p.critical_path <= 4,
        "critical path {} — the contract account is chaining",
        p.critical_path
    );
}

#[test]
fn mapping_slot_is_keccak_of_key_and_slot() {
    let key = word(Address::with_last_byte(1));
    assert_ne!(mapping_slot(key, 0), mapping_slot(key, 1));
}

fn kinds() -> Vec<ContractKind> {
    vec![
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
}

#[test]
fn rounds_agree_with_sequential_on_contracts() {
    for kind in kinds() {
        for seed in 0..8 {
            let w = workload(kind, 40, 120, seed);
            for threads in [2, 4, 8] {
                assert_agree(
                    &SequentialScheduler::new(),
                    &RoundScheduler::new(),
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

#[test]
#[ignore = "full gate sweep; run with --release -- --ignored"]
fn rounds_agree_on_contracts_full_gate() {
    for kind in kinds() {
        for seed in 0..1000 {
            let w = workload(kind, 60, 200, seed);
            for threads in [2, 4, 6, 8, 12] {
                assert_agree(
                    &SequentialScheduler::new(),
                    &RoundScheduler::new(),
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
