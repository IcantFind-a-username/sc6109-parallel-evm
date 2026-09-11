//! M0/M1 demo: run a generated block through the sequential baseline.
//!
//! Prints what the workload was and what executing it produced. This is the
//! smallest end-to-end path through the engine.

use parevm::sched::Scheduler;
use parevm::workload::{Distribution, TransferConfig, TransferWorkload};
use parevm::{SchedulerConfig, SequentialScheduler};

fn main() {
    for (label, recipients) in [
        ("uniform recipients", Distribution::Uniform),
        ("zipf s=1.0", Distribution::Zipf { s: 1.0 }),
        ("zipf s=2.0", Distribution::Zipf { s: 2.0 }),
    ] {
        let config = TransferConfig {
            accounts: 1_000,
            transactions: 2_000,
            recipients,
            ..Default::default()
        };
        let workload = TransferWorkload::generate(&config, 2026);
        let outcome = SequentialScheduler::new().execute_block(
            &workload.txs,
            &workload.base,
            &SchedulerConfig::default(),
        );

        println!("{label}  [{}, seed {}]", workload.name, workload.seed);
        println!("  transactions : {}", outcome.stats.transactions);
        println!("  executions   : {}", outcome.stats.executions);
        println!("  reverted     : {}", outcome.stats.reverted);
        println!("  wall clock   : {:?}", outcome.stats.wall_clock);
        println!(
            "  throughput   : {:.0} tx/s",
            outcome.stats.throughput_tps()
        );
        println!("  hot recipient: {} transfers", hottest(&workload));
        println!();
    }
}

/// Transfers landing on the single busiest recipient — a crude proxy for the
/// conflict density the schedulers will have to cope with.
fn hottest(workload: &parevm::Workload) -> usize {
    let mut counts = std::collections::HashMap::new();
    for tx in &workload.txs {
        if let revm::primitives::TxKind::Call(to) = tx.kind {
            *counts.entry(to).or_insert(0usize) += 1;
        }
    }
    counts.values().copied().max().unwrap_or(0)
}
