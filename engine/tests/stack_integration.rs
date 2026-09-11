//! The whole state-access stack, driven by a real EVM.
//!
//! ```text
//!   revm -> WrapDatabaseRef -> ReadRecorder<SimpleView> -> SimpleState -> BaseState
//! ```
//!
//! If this passes, the architecture in `docs/DESIGN.md` holds: the EVM can read
//! through our layers, every read is captured, and results commit correctly.

use parevm::{BaseState, Granularity, Key, ReadRecorder, SimpleState};
use revm::context::TxEnv;
use revm::database_interface::WrapDatabaseRef;
use revm::primitives::{address, Address, TxKind, U256};
use revm::state::AccountInfo;
use revm::{Context, ExecuteEvm, MainBuilder, MainContext};

const ONE_ETH: u128 = 1_000_000_000_000_000_000;
const ALICE: Address = address!("1111111111111111111111111111111111111111");
const BOB: Address = address!("2222222222222222222222222222222222222222");
/// revm's default block beneficiary.
const BENEFICIARY: Address = Address::ZERO;

fn funded_state() -> SimpleState {
    let mut base = BaseState::new();
    base.insert_account(
        ALICE,
        AccountInfo {
            balance: U256::from(10 * ONE_ETH),
            nonce: 0,
            ..Default::default()
        },
    );
    SimpleState::new(base)
}

/// Builds a transfer from ALICE.
///
/// The nonce is explicit because it must be: revm enforces it, so a workload
/// generator emitting several transactions from one sender has to number them.
/// Getting this wrong fails the whole batch with `NonceTooLow` rather than
/// producing a subtly wrong result.
fn transfer_tx(value: u128, gas_price: u128, nonce: u64) -> TxEnv {
    TxEnv::builder()
        .caller(ALICE)
        .kind(TxKind::Call(BOB))
        .value(U256::from(value))
        .gas_limit(21_000)
        .gas_price(gas_price)
        .nonce(nonce)
        .build()
        .expect("tx env")
}

/// Executes one transaction through the full stack and commits it, returning
/// the read set that was captured.
fn run_one(state: &mut SimpleState, tx: TxEnv, granularity: Granularity) -> parevm::ReadSet {
    let recorder = ReadRecorder::new(state.view(), granularity);
    let mut evm = Context::mainnet()
        .with_db(WrapDatabaseRef(&recorder))
        .build_mainnet();
    let out = evm.transact(tx).expect("transact");
    assert!(out.result.is_success(), "transaction reverted");
    let reads = recorder.take_read_set();
    state.commit(out.state);
    reads
}

#[test]
fn evm_reads_through_the_recorder() {
    let mut state = funded_state();
    let reads = run_one(&mut state, transfer_tx(ONE_ETH, 0, 0), Granularity::Slot);

    assert!(
        !reads.is_empty(),
        "the EVM must have read state through the recorder"
    );
    let keys: Vec<Key> = reads.iter().map(|(k, _)| *k).collect();
    assert!(
        keys.contains(&Key::Basic(ALICE)),
        "the sender's account must be read; captured {keys:?}"
    );
}

#[test]
fn transfer_commits_correctly() {
    let mut state = funded_state();
    run_one(&mut state, transfer_tx(ONE_ETH, 0, 0), Granularity::Slot);

    let alice = state.account(ALICE).expect("alice");
    let bob = state.account(BOB).expect("bob");
    assert_eq!(alice.balance, U256::from(9 * ONE_ETH));
    assert_eq!(alice.nonce, 1, "sender nonce must increment");
    assert_eq!(bob.balance, U256::from(ONE_ETH));
}

#[test]
fn transfers_compose_across_transactions() {
    let mut state = funded_state();
    for nonce in 0..3 {
        run_one(
            &mut state,
            transfer_tx(ONE_ETH, 0, nonce),
            Granularity::Slot,
        );
    }

    let alice = state.account(ALICE).expect("alice");
    assert_eq!(alice.balance, U256::from(7 * ONE_ETH));
    assert_eq!(alice.nonce, 3);
    assert_eq!(
        state.account(BOB).expect("bob").balance,
        U256::from(3 * ONE_ETH)
    );
}

/// R2, asserted rather than merely observed. The beneficiary is written by
/// every transaction, including at zero gas price, so naive conflict detection
/// would serialise any block. See `docs/DESIGN.md` section 5.
#[test]
fn beneficiary_is_written_even_at_zero_gas_price() {
    for gas_price in [0u128, 1_000_000_000] {
        let state = funded_state();
        let recorder = ReadRecorder::new(state.view(), Granularity::Slot);
        let mut evm = Context::mainnet()
            .with_db(WrapDatabaseRef(&recorder))
            .build_mainnet();
        let out = evm
            .transact(transfer_tx(ONE_ETH, gas_price, 0))
            .expect("transact");

        let touched: Vec<Address> = out
            .state
            .iter()
            .filter(|(_, acc)| acc.is_touched())
            .map(|(addr, _)| *addr)
            .collect();

        assert!(
            touched.contains(&BENEFICIARY),
            "at gas_price={gas_price} the beneficiary must appear in the write set; \
             touched {touched:?}"
        );
    }
}

#[test]
fn fees_are_charged_to_the_sender() {
    let mut state = funded_state();
    run_one(
        &mut state,
        transfer_tx(ONE_ETH, 1_000_000_000, 0),
        Granularity::Slot,
    );

    let alice = state.account(ALICE).expect("alice");
    let fee = U256::from(21_000u64) * U256::from(1_000_000_000u64);
    assert_eq!(alice.balance, U256::from(9 * ONE_ETH) - fee);
}

#[test]
fn granularity_does_not_change_execution_results() {
    let mut slot_state = funded_state();
    let mut account_state = funded_state();
    run_one(
        &mut slot_state,
        transfer_tx(ONE_ETH, 0, 0),
        Granularity::Slot,
    );
    run_one(
        &mut account_state,
        transfer_tx(ONE_ETH, 0, 0),
        Granularity::Account,
    );

    assert_eq!(
        slot_state.account(ALICE).expect("alice").balance,
        account_state.account(ALICE).expect("alice").balance,
        "granularity affects conflict detection only, never the result"
    );
}
