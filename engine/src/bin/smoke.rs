//! M0 smoke test: prove the revm 41 binding works before building anything on it.
//!
//! Executes plain value transfers through a real EVM and checks the balances
//! afterwards. No contract, so this runs without Foundry.
//!
//! Also demonstrates R2 empirically: with a non-zero gas price, every
//! transaction writes the block beneficiary's balance, which would make all
//! transactions mutually conflicting under naive conflict detection.

use revm::database::{CacheDB, EmptyDB};
use revm::primitives::{Address, TxKind, U256};
use revm::state::AccountInfo;
use revm::{Context, ExecuteEvm, MainBuilder, MainContext};

const ONE_ETH: u128 = 1_000_000_000_000_000_000;

fn transfer(gas_price: u128) {
    let alice = Address::from([0x11; 20]);
    let bob = Address::from([0x22; 20]);

    let mut db = CacheDB::new(EmptyDB::default());
    db.insert_account_info(
        alice,
        AccountInfo {
            balance: U256::from(10 * ONE_ETH),
            nonce: 0,
            ..Default::default()
        },
    );

    let tx = revm::context::TxEnv::builder()
        .caller(alice)
        .kind(TxKind::Call(bob))
        .value(U256::from(ONE_ETH))
        .gas_limit(21_000)
        .gas_price(gas_price)
        .build()
        .expect("tx env");

    let mut evm = Context::mainnet().with_db(db).build_mainnet();
    let out = evm.transact(tx).expect("transact");

    println!("--- gas_price = {gas_price} ---");
    println!("  success  : {}", out.result.is_success());
    println!("  gas used : {}", out.result.tx_gas_used());
    println!(
        "  accounts touched by this one transfer: {}",
        out.state.len()
    );
    let mut addrs: Vec<_> = out.state.iter().collect();
    addrs.sort_by_key(|(a, _)| **a);
    for (addr, acc) in addrs {
        let tag = if *addr == alice {
            "alice"
        } else if *addr == bob {
            "bob"
        } else {
            "BENEFICIARY  <-- R2"
        };
        println!(
            "  {addr:?}  balance={:<22} nonce={}  {tag}",
            acc.info.balance, acc.info.nonce
        );
    }
    println!();
}

fn main() {
    // Zero gas price: the beneficiary is still touched, but with no value moved.
    transfer(0);
    // Realistic gas price: the beneficiary receives fees. Every transaction in a
    // block writes this one account, so naive conflict detection would serialise
    // the entire block. See docs/RISKS.md R2 and docs/DESIGN.md section 5.
    transfer(1_000_000_000);
}
