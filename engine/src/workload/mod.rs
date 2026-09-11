//! Workload generation.
//!
//! Conflict density is the independent variable of the whole study, so these
//! generators exist to produce it on demand and reproducibly.
//!
//! # Reproducibility
//!
//! Every workload is a pure function of its seed. `ChaCha8Rng` is used rather
//! than a thread-local or OS generator because it is specified rather than
//! implementation-defined: the same seed yields the same workload on any
//! machine, any platform, any version. A benchmark number whose input cannot be
//! regenerated is not a measurement.

pub mod analysis;
pub mod compute;
pub mod contract;
pub mod transfer;

pub use analysis::{analyse, profile, AccessSet, DependencyProfile};
pub use compute::{ComputeConfig, ComputeWorkload};
pub use contract::{ContractConfig, ContractKind, ContractWorkload};
pub use transfer::{is_outside_precompile_range, Distribution, TransferConfig, TransferWorkload};

use crate::state::BaseState;
use revm::context::TxEnv;

/// A generated block: the state it starts from, and the transactions to run.
#[derive(Clone, Debug)]
pub struct Workload {
    /// Identifier used in result files and figure labels.
    pub name: String,
    /// The seed that produced this workload. Committed alongside any result.
    pub seed: u64,
    pub base: BaseState,
    pub txs: Vec<TxEnv>,
}

impl Workload {
    pub fn len(&self) -> usize {
        self.txs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.txs.is_empty()
    }
}

/// Samples an index from a population, with a tunable concentration.
///
/// Written out rather than pulled from `rand_distr` so that the exact sampling
/// procedure is visible and can be described in the report — the shape of this
/// distribution is what the conflict-rate figure varies.
#[derive(Clone, Debug)]
pub struct Sampler {
    /// Cumulative distribution over `0..n`. Uniform when the exponent is zero.
    cdf: Vec<f64>,
}

impl Sampler {
    /// Uniform over `n` items.
    pub fn uniform(n: usize) -> Self {
        Self {
            cdf: (1..=n).map(|i| i as f64 / n as f64).collect(),
        }
    }

    /// Zipf over `n` items with exponent `s`: item `k` has weight `1/k^s`.
    ///
    /// `s = 0` is uniform. Larger `s` concentrates mass on the first few items,
    /// which is how a hot account is produced. Around `s = 1` the distribution
    /// matches the skew commonly observed in real token transfer traffic.
    pub fn zipf(n: usize, s: f64) -> Self {
        let weights: Vec<f64> = (1..=n).map(|k| 1.0 / (k as f64).powf(s)).collect();
        let total: f64 = weights.iter().sum();
        let mut acc = 0.0;
        let cdf = weights
            .iter()
            .map(|w| {
                acc += w / total;
                acc
            })
            .collect();
        Self { cdf }
    }

    /// Draws an index in `0..n`.
    pub fn sample(&self, rng: &mut impl rand::Rng) -> usize {
        let u: f64 = rng.random();
        match self.cdf.binary_search_by(|p| p.partial_cmp(&u).unwrap()) {
            Ok(i) => i,
            Err(i) => i.min(self.cdf.len() - 1),
        }
    }
}
