//! Value-transfer workloads.
//!
//! Plain ETH transfers need no compiled contract, so this workload runs before
//! Foundry is in the picture. It gives the conflict-rate axis its full range:
//! uniform recipients collide almost never, and a concentrated Zipf
//! distribution turns a handful of accounts into hot spots.

use super::{Sampler, Workload};
use crate::state::BaseState;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use revm::context::TxEnv;
use revm::primitives::{Address, TxKind, U256};
use revm::state::AccountInfo;
use std::collections::HashMap;

/// How recipients are chosen. The knob that sets conflict density.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Distribution {
    /// Every account equally likely. Collisions are rare, so speedup should
    /// approach linear.
    Uniform,
    /// Weighted toward a few accounts by exponent `s`. Sweeping `s` produces
    /// the speedup-versus-conflict-rate figure.
    Zipf { s: f64 },
}

impl Distribution {
    fn sampler(&self, n: usize) -> Sampler {
        match self {
            Distribution::Uniform => Sampler::uniform(n),
            Distribution::Zipf { s } => Sampler::zipf(n, *s),
        }
    }

    fn label(&self) -> String {
        match self {
            Distribution::Uniform => "uniform".to_string(),
            Distribution::Zipf { s } => format!("zipf{s}"),
        }
    }
}

/// Parameters of a transfer workload.
#[derive(Clone, Debug)]
pub struct TransferConfig {
    /// Size of the account population.
    pub accounts: usize,
    /// Transactions to generate.
    pub transactions: usize,
    /// How recipients are chosen.
    pub recipients: Distribution,
    /// Wei transferred per transaction.
    pub value: u128,
    /// Zero keeps fee accounting out of early correctness work. The block
    /// beneficiary is written regardless — see `docs/DESIGN.md` section 5.
    pub gas_price: u128,
}

impl Default for TransferConfig {
    fn default() -> Self {
        Self {
            accounts: 1_000,
            transactions: 1_000,
            recipients: Distribution::Uniform,
            value: 1_000,
            gas_price: 0,
        }
    }
}

/// Generates value-transfer blocks.
pub struct TransferWorkload;

impl TransferWorkload {
    /// Builds a workload from a seed.
    ///
    /// Senders are drawn uniformly whatever the recipient distribution is, so
    /// that the conflict knob moves one thing at a time: concentrating senders
    /// as well would confound the axis.
    pub fn generate(config: &TransferConfig, seed: u64) -> Workload {
        assert!(config.accounts > 0, "a workload needs accounts");

        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        let addresses: Vec<Address> = (0..config.accounts).map(account_address).collect();

        let mut base = BaseState::new();
        let funding = starting_balance(config);
        for address in &addresses {
            base.insert_account(
                *address,
                AccountInfo {
                    balance: funding,
                    nonce: 0,
                    ..Default::default()
                },
            );
        }

        let senders = Sampler::uniform(config.accounts);
        let recipients = config.recipients.sampler(config.accounts);

        // Nonces must increase per sender or the EVM rejects the batch outright
        // with NonceTooLow. See E4 in docs/AI_USAGE.md.
        let mut nonces: HashMap<usize, u64> = HashMap::new();
        let mut txs = Vec::with_capacity(config.transactions);

        for _ in 0..config.transactions {
            let from = senders.sample(&mut rng);
            let mut to = recipients.sample(&mut rng);
            if to == from {
                // A self-transfer is a degenerate case that reads and writes one
                // account and moves nothing. Excluded so the conflict structure
                // stays interpretable.
                to = (to + 1) % config.accounts;
            }

            let nonce = nonces.entry(from).or_insert(0);
            txs.push(
                TxEnv::builder()
                    .caller(addresses[from])
                    .kind(TxKind::Call(addresses[to]))
                    .value(U256::from(config.value))
                    .gas_limit(21_000)
                    .gas_price(config.gas_price)
                    .nonce(*nonce)
                    .build()
                    .expect("transfer tx env"),
            );
            *nonce += 1;
        }

        Workload {
            name: format!("transfer-{}", config.recipients.label()),
            seed,
            base,
            txs,
        }
    }
}

/// Leading byte of every generated account address.
///
/// Generated addresses **must not** collide with the precompile range, which
/// occupies the low addresses from `0x01` upward. A transfer to a precompile
/// does not move value — it invokes the precompile, which then halts with
/// `OutOfGas(Precompile)` on a 21000-gas transfer carrying no input. The
/// transaction still commits a nonce and a fee, so the block executes and the
/// numbers look plausible while a slice of the workload is doing something
/// entirely different from what it claims. See E5 in `docs/AI_USAGE.md`.
///
/// The prefix also makes generated addresses recognisable in a failure message.
const ACCOUNT_PREFIX: u8 = 0xA1;

/// Deterministic address for account `i`, with the index in the low bytes so
/// that failures name an account a human can find.
fn account_address(i: usize) -> Address {
    let mut bytes = [0u8; 20];
    bytes[0] = ACCOUNT_PREFIX;
    bytes[12..20].copy_from_slice(&(i as u64 + 1).to_be_bytes());
    Address::from(bytes)
}

/// Highest address that is or may become a precompile. Generated accounts stay
/// clear of everything at or below this.
const PRECOMPILE_CEILING: u64 = 0xFF;

/// Whether an address is safely outside the precompile range.
pub fn is_outside_precompile_range(address: &Address) -> bool {
    let bytes = address.0 .0;
    if bytes[0] != 0 {
        return true;
    }
    let low = u64::from_be_bytes(bytes[12..20].try_into().expect("8 bytes"));
    bytes[..12].iter().any(|b| *b != 0) || low > PRECOMPILE_CEILING
}

/// Funds every account well past what the block can spend, so that no transfer
/// fails for insufficient balance. A failing transfer would still commit a
/// nonce and a fee, which is valid EVM behaviour but muddies a workload whose
/// purpose is to exhibit a known conflict structure.
fn starting_balance(config: &TransferConfig) -> U256 {
    let spend = config.value.saturating_mul(config.transactions as u128);
    let fees = config
        .gas_price
        .saturating_mul(21_000 * config.transactions as u128);
    U256::from(spend.saturating_add(fees)) + U256::from(1_000_000_000_000_000_000u128)
}
