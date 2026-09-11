//! Contract workloads: Solidity from `contracts/`, compiled by Foundry, run as
//! real bytecode.
//!
//! Contracts are placed in base state directly — code plus seeded storage —
//! rather than deployed inside the block: account creation mid-block is out of
//! scope for the multi-version store (DESIGN §6), and deployment would add one
//! transaction every other one depends on. Seeding storage means computing
//! Solidity's mapping slots here, so the layouts documented in each contract
//! are load-bearing; `tests/contracts.rs` checks them by calling the getters.

use super::transfer::account_address;
use super::{Distribution, Sampler, Workload};
use crate::state::BaseState;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use revm::bytecode::Bytecode;
use revm::context::TxEnv;
use revm::primitives::{keccak256, Address, Bytes, TxKind, B256, U256};
use revm::state::AccountInfo;
use std::collections::HashMap;

/// Contract addresses carry a `0xC0` prefix: clear of the precompile range
/// (E5) and distinguishable from generated accounts (`0xA1`) in a diff.
pub const TOKEN: Address = contract_address(1);
pub const COLLECTIBLE: Address = contract_address(2);
pub const POOL: Address = contract_address(3);

const fn contract_address(i: u8) -> Address {
    let mut b = [0u8; 20];
    b[0] = 0xC0;
    b[19] = i;
    Address::new(b)
}

/// Gas for every contract call. Comfortably above the costliest path — a mint
/// writing two fresh slots — and far below the EIP-7825 cap.
const GAS_LIMIT: u64 = 200_000;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum ContractKind {
    /// ERC-20 transfers; recipients drawn from `recipients`.
    Erc20 { recipients: Distribution },
    /// ERC-20 transfers where every account starts with exactly one transfer's
    /// worth of tokens. A sender's second transfer succeeds only if it was paid
    /// earlier in the block, so whether a transaction writes balances or
    /// reverts depends on what it reads — the shape that exercises write
    /// retraction end to end (E8).
    Erc20Tight { recipients: Distribution },
    /// Every transaction mints from one collection: one hot slot.
    NftMint,
    /// Every transaction swaps on one pool: two hot slots.
    AmmSwap,
}

impl ContractKind {
    pub fn label(&self) -> String {
        let dist = |d: &Distribution| match d {
            Distribution::Uniform => "uniform".to_string(),
            Distribution::Zipf { s } => format!("zipf{s}"),
        };
        match self {
            ContractKind::Erc20 { recipients } => format!("erc20-{}", dist(recipients)),
            ContractKind::Erc20Tight { recipients } => format!("erc20tight-{}", dist(recipients)),
            ContractKind::NftMint => "nft-mint".into(),
            ContractKind::AmmSwap => "amm-swap".into(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct ContractConfig {
    pub kind: ContractKind,
    pub accounts: usize,
    pub transactions: usize,
}

pub struct ContractWorkload;

/// Tokens moved per ERC-20 transfer.
const TRANSFER_AMOUNT: u64 = 100;

impl ContractWorkload {
    pub fn generate(config: &ContractConfig, seed: u64) -> Workload {
        assert!(
            config.accounts > 1,
            "a contract workload needs at least two accounts"
        );
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        let addresses: Vec<Address> = (0..config.accounts).map(account_address).collect();

        let mut base = BaseState::new();
        for a in &addresses {
            base.insert_account(
                *a,
                AccountInfo {
                    balance: U256::from(10u128.pow(18)),
                    ..Default::default()
                },
            );
        }

        let senders = Sampler::uniform(config.accounts);
        let mut nonces: HashMap<usize, u64> = HashMap::new();
        let mut next_tx = |from: usize, to: Address, data: Bytes| {
            let nonce = nonces.entry(from).or_insert(0);
            let tx = TxEnv::builder()
                .caller(addresses[from])
                .kind(TxKind::Call(to))
                .data(data)
                .gas_limit(GAS_LIMIT)
                .gas_price(0)
                .nonce(*nonce)
                .build()
                .expect("contract tx env");
            *nonce += 1;
            tx
        };

        let mut txs = Vec::with_capacity(config.transactions);
        match config.kind {
            ContractKind::Erc20 { recipients } | ContractKind::Erc20Tight { recipients } => {
                let tight = matches!(config.kind, ContractKind::Erc20Tight { .. });
                let initial = if tight {
                    TRANSFER_AMOUNT
                } else {
                    1_000_000_000
                };
                deploy(&mut base, TOKEN, &TOKEN_CODE);
                for a in &addresses {
                    base.insert_storage(TOKEN, mapping_slot(word(*a), 0), U256::from(initial));
                }
                base.insert_storage(
                    TOKEN,
                    U256::from(1),
                    U256::from(initial) * U256::from(config.accounts),
                );
                let to_sampler = recipients_sampler(recipients, config.accounts);
                for _ in 0..config.transactions {
                    let from = senders.sample(&mut rng);
                    let mut to = to_sampler.sample(&mut rng);
                    if to == from {
                        to = (to + 1) % config.accounts;
                    }
                    let data = call(
                        "transfer(address,uint256)",
                        &[word(addresses[to]), U256::from(TRANSFER_AMOUNT).into()],
                    );
                    txs.push(next_tx(from, TOKEN, data));
                }
            }
            ContractKind::NftMint => {
                deploy(&mut base, COLLECTIBLE, &COLLECTIBLE_CODE);
                for _ in 0..config.transactions {
                    let from = senders.sample(&mut rng);
                    txs.push(next_tx(from, COLLECTIBLE, call("mint()", &[])));
                }
            }
            ContractKind::AmmSwap => {
                deploy(&mut base, POOL, &POOL_CODE);
                let reserve = U256::from(10u128.pow(15));
                base.insert_storage(POOL, U256::from(0), reserve);
                base.insert_storage(POOL, U256::from(1), reserve);
                for a in &addresses {
                    base.insert_storage(
                        POOL,
                        mapping_slot(word(*a), 2),
                        U256::from(10u128.pow(12)),
                    );
                    base.insert_storage(
                        POOL,
                        mapping_slot(word(*a), 3),
                        U256::from(10u128.pow(12)),
                    );
                }
                for _ in 0..config.transactions {
                    let from = senders.sample(&mut rng);
                    let zero_for_one = rng.random::<bool>();
                    let amount = rng.random_range(1_000u64..1_000_000);
                    let data = call(
                        "swap(bool,uint256)",
                        &[
                            U256::from(zero_for_one as u8).into(),
                            U256::from(amount).into(),
                        ],
                    );
                    txs.push(next_tx(from, POOL, data));
                }
            }
        }

        Workload {
            name: config.kind.label(),
            seed,
            base,
            txs,
        }
    }
}

fn recipients_sampler(d: Distribution, n: usize) -> Sampler {
    match d {
        Distribution::Uniform => Sampler::uniform(n),
        Distribution::Zipf { s } => Sampler::zipf(n, s),
    }
}

static TOKEN_CODE: std::sync::LazyLock<Bytecode> =
    std::sync::LazyLock::new(|| runtime(include_str!("../../assets/Token.runtime.hex")));
static COLLECTIBLE_CODE: std::sync::LazyLock<Bytecode> =
    std::sync::LazyLock::new(|| runtime(include_str!("../../assets/Collectible.runtime.hex")));
static POOL_CODE: std::sync::LazyLock<Bytecode> =
    std::sync::LazyLock::new(|| runtime(include_str!("../../assets/Pool.runtime.hex")));

fn runtime(hex: &str) -> Bytecode {
    let bytes: Vec<u8> = (0..hex.trim().len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex.trim()[i..i + 2], 16).expect("bytecode hex"))
        .collect();
    Bytecode::new_raw(Bytes::from(bytes))
}

/// Places a contract in base state. Nonce 1, as EIP-161 gives every contract.
fn deploy(base: &mut BaseState, at: Address, code: &Bytecode) {
    base.insert_account(
        at,
        AccountInfo {
            balance: U256::ZERO,
            nonce: 1,
            code_hash: code.hash_slow(),
            code: Some(code.clone()),
            ..Default::default()
        },
    );
}

/// A value as one 32-byte ABI word.
pub fn word(a: Address) -> B256 {
    a.into_word()
}

/// Solidity's storage slot for `mapping[key]` where the mapping sits at `slot`:
/// `keccak256(key . slot)`, both as 32-byte words.
pub fn mapping_slot(key: B256, slot: u64) -> U256 {
    let mut buf = [0u8; 64];
    buf[..32].copy_from_slice(key.as_slice());
    buf[32..].copy_from_slice(&U256::from(slot).to_be_bytes::<32>());
    U256::from_be_bytes(keccak256(buf).0)
}

/// ABI-encodes a call: the 4-byte selector of `signature`, then each argument
/// as a word. Enough for the static argument types these contracts take.
pub fn call(signature: &str, args: &[B256]) -> Bytes {
    let mut data = keccak256(signature.as_bytes())[..4].to_vec();
    for a in args {
        data.extend_from_slice(a.as_slice());
    }
    Bytes::from(data)
}
