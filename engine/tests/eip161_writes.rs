//! D18 and D19 end to end: a write is a changed value, and an empty account
//! does not exist. Regression tests for E12 and E14, built on the workload that
//! exposed them.

use parevm::sched::Scheduler;
use parevm::workload::{analyse, ComputeConfig, ComputeWorkload};
use parevm::{
    assert_agree, BaseState, RoundScheduler, SchedulerConfig, SequentialScheduler, Workload,
};
use revm::context::{BlockEnv, TxEnv};
use revm::primitives::{Address, TxKind, U256};
use revm::state::AccountInfo;

fn compute(accounts: usize, transactions: usize) -> Workload {
    ComputeWorkload::generate(
        &ComputeConfig {
            accounts,
            transactions,
            payload: 64,
        },
        2026,
    )
}

/// Every transaction calls the sha256 precompile. Before the fix this was one
/// dependency chain of length n; now only senders who send twice depend.
#[test]
fn precompile_calls_do_not_chain_in_the_analysis() {
    let w = compute(100_000, 500);
    let p = analyse(&w, &BlockEnv::default());
    assert!(
        p.critical_path <= 3,
        "critical path {} — the precompile chain is back",
        p.critical_path
    );
    assert!(p.density() < 0.02, "density {:.3}", p.density());
}

#[test]
fn precompile_calls_do_not_chain_in_the_scheduler() {
    let w = compute(100_000, 500);
    let out = RoundScheduler::new().execute_block(
        &w.txs,
        &w.base,
        &SchedulerConfig {
            threads: 4,
            ..Default::default()
        },
    );
    assert!(
        out.stats.rounds <= 4,
        "{} rounds — the scheduler is serialised again",
        out.stats.rounds
    );
    assert!(
        out.stats.abort_rate() < 0.05,
        "abort rate {:.3}",
        out.stats.abort_rate()
    );
}

#[test]
fn compute_workload_still_agrees_with_sequential() {
    for seed_threads in [2, 4, 8] {
        let w = compute(50, 200);
        let config = SchedulerConfig {
            threads: seed_threads,
            ..Default::default()
        };
        assert_agree(
            &SequentialScheduler::new(),
            &RoundScheduler::new(),
            &w,
            &config,
        );
    }
}

fn addr(i: u64) -> Address {
    let mut b = [0u8; 20];
    b[0] = 0xA1;
    b[12..].copy_from_slice(&i.to_be_bytes());
    Address::from(b)
}

/// A zero-value transfer to an address that does not exist touches it and
/// leaves it empty. Mainnet deletes it; both engines must treat it as absent.
#[test]
fn touched_empty_accounts_do_not_exist() {
    let mut base = BaseState::new();
    base.insert_account(
        addr(1),
        AccountInfo {
            balance: U256::from(10u128.pow(18)),
            ..Default::default()
        },
    );
    let ghost = addr(999);
    let txs = vec![TxEnv::builder()
        .caller(addr(1))
        .kind(TxKind::Call(ghost))
        .value(U256::ZERO)
        .gas_limit(21_000)
        .gas_price(0)
        .nonce(0)
        .build()
        .unwrap()];
    let w = Workload {
        name: "ghost".into(),
        seed: 0,
        base,
        txs,
    };

    let seq =
        SequentialScheduler::new().execute_block(&w.txs, &w.base, &SchedulerConfig::default());
    assert!(
        !seq.state.accounts.contains_key(&ghost),
        "sequential kept a touched empty account"
    );
    assert!(
        !seq.state.accounts.contains_key(&Address::ZERO),
        "the zero-fee beneficiary is empty and must not exist either"
    );
    assert_agree(
        &SequentialScheduler::new(),
        &RoundScheduler::new(),
        &w,
        &SchedulerConfig {
            threads: 2,
            ..Default::default()
        },
    );
}
