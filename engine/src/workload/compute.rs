//! Compute workload: work per transaction as an experimental variable (D16).
//!
//! Each transaction calls the sha256 precompile with a payload of tunable size.
//! No compiler is needed, and the only possible conflicts are sender nonces, so
//! with a large sender set this isolates per-transaction cost from conflict
//! density. E10 found this is what decides whether parallelism pays at all: a
//! plain transfer costs about a microsecond, too little to amortise fixed
//! per-transaction overhead.

use super::{Sampler, Workload};
use crate::state::BaseState;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use revm::context::TxEnv;
use revm::primitives::{Address, Bytes, TxKind, U256};
use revm::state::AccountInfo;
use std::collections::HashMap;

/// The sha256 precompile.
pub const SHA256: Address = Address::with_last_byte(2);

/// Per-transaction gas cap from EIP-7825. revm refuses anything above it, and a
/// refusal is cheap enough to pass for a fast execution — which is exactly how
/// E10's first measurement went wrong.
pub const TX_GAS_CAP: u64 = 1 << 24;

#[derive(Clone, Debug)]
pub struct ComputeConfig {
    /// Sender population. Large, so nonce conflicts stay rare.
    pub accounts: usize,
    pub transactions: usize,
    /// Bytes hashed per transaction. Zero sends an empty call.
    pub payload: usize,
}

impl Default for ComputeConfig {
    fn default() -> Self {
        Self {
            accounts: 100_000,
            transactions: 1_000,
            payload: 1_024,
        }
    }
}

pub struct ComputeWorkload;

impl ComputeWorkload {
    pub fn generate(config: &ComputeConfig, seed: u64) -> Workload {
        assert!(config.accounts > 0, "a workload needs accounts");
        let gas_limit = gas_limit(config.payload);
        assert!(
            gas_limit < TX_GAS_CAP,
            "payload of {} bytes needs a {gas_limit} gas limit, above the EIP-7825 cap of {TX_GAS_CAP}",
            config.payload
        );

        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        let addresses: Vec<Address> = (0..config.accounts)
            .map(super::transfer::account_address)
            .collect();

        let mut base = BaseState::new();
        for address in &addresses {
            base.insert_account(
                *address,
                AccountInfo {
                    balance: U256::from(1_000_000_000_000_000_000u128),
                    ..Default::default()
                },
            );
        }

        // One shared buffer: `Bytes` is reference-counted, so a 32 KB payload on
        // ten thousand transactions costs 32 KB, not 320 MB.
        let data = Bytes::from(vec![0xab; config.payload]);
        let senders = Sampler::uniform(config.accounts);
        let mut nonces: HashMap<usize, u64> = HashMap::new();
        let txs = (0..config.transactions)
            .map(|_| {
                let from = senders.sample(&mut rng);
                let nonce = nonces.entry(from).or_insert(0);
                let tx = TxEnv::builder()
                    .caller(addresses[from])
                    .kind(TxKind::Call(SHA256))
                    .data(data.clone())
                    .gas_limit(gas_limit)
                    .gas_price(0)
                    .nonce(*nonce)
                    .build()
                    .expect("compute tx env");
                *nonce += 1;
                tx
            })
            .collect();

        Workload {
            name: format!("compute-{}b", config.payload),
            seed,
            base,
            txs,
        }
    }
}

/// Generous but bounded: intrinsic cost, calldata at the EIP-7623 floor rate,
/// and the precompile's per-word charge, with headroom.
fn gas_limit(payload: usize) -> u64 {
    let bytes = payload as u64;
    21_000 + bytes * 40 + (bytes / 32 + 1) * 12 + 100_000
}
