//! The dependency analysis behind Figure 2's x-axis (D17), and the compute
//! workload behind Figure 3 (D16).

use parevm::sched::Scheduler;
use parevm::workload::{
    analyse, ComputeConfig, ComputeWorkload, Distribution, TransferConfig, TransferWorkload,
};
use parevm::{BaseState, SchedulerConfig, SequentialScheduler, Workload};
use revm::context::{BlockEnv, TxEnv};
use revm::primitives::{Address, TxKind, U256};
use revm::state::AccountInfo;

fn addr(i: u64) -> Address {
    let mut b = [0u8; 20];
    b[0] = 0xA1;
    b[12..].copy_from_slice(&i.to_be_bytes());
    Address::from(b)
}

/// A block built by hand, so the expected dependency structure is known.
fn block(transfers: &[(u64, u64, u64)], gas_price: u128) -> Workload {
    let mut base = BaseState::new();
    for i in 0..64 {
        base.insert_account(
            addr(i),
            AccountInfo {
                balance: U256::from(10u128.pow(20)),
                ..Default::default()
            },
        );
    }
    let txs = transfers
        .iter()
        .map(|&(from, to, nonce)| {
            TxEnv::builder()
                .caller(addr(from))
                .kind(TxKind::Call(addr(to)))
                .value(U256::from(1))
                .gas_limit(21_000)
                .gas_price(gas_price)
                .nonce(nonce)
                .build()
                .unwrap()
        })
        .collect();
    Workload {
        name: "hand-built".into(),
        seed: 0,
        base,
        txs,
    }
}

#[test]
fn empty_block_has_no_dependencies() {
    let p = analyse(&block(&[], 0), &BlockEnv::default());
    assert_eq!((p.transactions, p.dependent, p.critical_path), (0, 0, 0));
    assert_eq!(p.density(), 0.0);
}

#[test]
fn disjoint_transfers_are_independent() {
    let p = analyse(
        &block(&[(0, 1, 0), (2, 3, 0), (4, 5, 0), (6, 7, 0)], 0),
        &BlockEnv::default(),
    );
    assert_eq!(p.dependent, 0);
    assert_eq!(p.critical_path, 1);
    assert_eq!(p.parallelism_ceiling(), 4.0);
}

#[test]
fn one_sender_is_one_chain() {
    let p = analyse(
        &block(&[(0, 1, 0), (0, 2, 1), (0, 3, 2), (0, 4, 3)], 0),
        &BlockEnv::default(),
    );
    assert_eq!(p.dependent, 3);
    assert_eq!(
        p.critical_path, 4,
        "every transaction reads the nonce its predecessor wrote"
    );
}

#[test]
fn recipient_reuse_creates_a_dependency() {
    // tx 1 sends *from* the account tx 0 paid, so it reads tx 0's write.
    let p = analyse(
        &block(&[(0, 1, 0), (1, 2, 0), (3, 4, 0)], 0),
        &BlockEnv::default(),
    );
    assert_eq!(p.dependent, 1);
    assert_eq!(p.critical_path, 2);
}

/// With fees on, every transaction writes the beneficiary. If it were counted,
/// disjoint transfers would form one chain; D4 says it must not be.
#[test]
fn beneficiary_is_excluded() {
    let p = analyse(
        &block(&[(0, 1, 0), (2, 3, 0), (4, 5, 0)], 1_000_000_000),
        &BlockEnv::default(),
    );
    assert_eq!(p.dependent, 0);
    assert_eq!(p.critical_path, 1);
}

/// The E9 finding, reproduced by the read-set analysis: at a 1:1 account ratio
/// "uniform" is heavily dependent; at the new 100:1 default it is not.
#[test]
fn account_ratio_decides_uniform_density() {
    let dense = TransferWorkload::generate(
        &TransferConfig {
            accounts: 2_000,
            transactions: 2_000,
            recipients: Distribution::Uniform,
            ..Default::default()
        },
        7,
    );
    let sparse = TransferWorkload::generate(
        &TransferConfig {
            accounts: 200_000,
            transactions: 2_000,
            ..Default::default()
        },
        7,
    );
    let d = analyse(&dense, &BlockEnv::default()).density();
    let s = analyse(&sparse, &BlockEnv::default()).density();
    assert!(
        d > 0.6,
        "1:1 uniform should be heavily dependent, measured {d:.3}"
    );
    assert!(s < 0.05, "100:1 uniform should be sparse, measured {s:.3}");
}

#[test]
fn compute_transactions_execute_and_do_real_work() {
    for payload in [0, 1_024, 32_768] {
        let w = ComputeWorkload::generate(
            &ComputeConfig {
                accounts: 1_000,
                transactions: 50,
                payload,
            },
            3,
        );
        let out =
            SequentialScheduler::new().execute_block(&w.txs, &w.base, &SchedulerConfig::default());
        assert_eq!(
            out.stats.reverted, 0,
            "payload {payload}: sha256 calls must succeed"
        );
        for tx in &w.txs {
            assert!(tx.gas_limit < parevm::workload::compute::TX_GAS_CAP);
        }
    }
}

/// E10's diagnostic error was refused transactions posing as fast ones. Gas
/// used must grow with the payload, or the work is not being done.
#[test]
fn compute_gas_grows_with_payload() {
    let gas = |payload| {
        let w = ComputeWorkload::generate(
            &ComputeConfig {
                accounts: 10,
                transactions: 1,
                payload,
            },
            0,
        );
        let out = parevm::exec::execute(
            &w.base,
            w.txs[0].clone(),
            &BlockEnv::default(),
            parevm::Granularity::Slot,
        );
        out.outcome.expect("must not be refused").gas_used()
    };
    let (small, large) = (gas(1_024), gas(32_768));
    assert!(
        large > small * 10,
        "gas {small} at 1 KB vs {large} at 32 KB"
    );
}

#[test]
#[should_panic(expected = "above the EIP-7825 cap")]
fn compute_refuses_payloads_over_the_gas_cap() {
    ComputeWorkload::generate(
        &ComputeConfig {
            accounts: 10,
            transactions: 1,
            payload: 1 << 20,
        },
        0,
    );
}
