//! Benchmark runner. Never part of `cargo test`.
//!
//! ```text
//! cargo run --release --bin bench -- m2a-baseline [out-dir]
//! ```
//!
//! Every run's final state is checked against the sequential baseline before
//! its timing is kept, outside the timed region. A timing taken on a wrong
//! result is not a measurement.

use parevm::sched::Scheduler;
use parevm::workload::{
    analyse, ComputeConfig, ComputeWorkload, Distribution, TransferConfig, TransferWorkload,
};
use parevm::{RoundScheduler, SchedulerConfig, SequentialScheduler, Workload};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

const SEED: u64 = 2026;
const RUNS: usize = 5;
const THREADS: [usize; 6] = [1, 2, 4, 6, 8, 12];
const TRANSACTIONS: usize = 10_000;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("m2a-baseline") => {
            let out = args
                .get(1)
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("../results"));
            m2a_baseline(&out);
        }
        _ => {
            eprintln!("usage: bench m2a-baseline [out-dir]");
            std::process::exit(2);
        }
    }
}

struct Cell {
    workload: Workload,
    family: &'static str,
    param: String,
    accounts: usize,
}

fn cells() -> Vec<Cell> {
    let mut cells = Vec::new();
    let transfer = |accounts, recipients, family, param: &str| Cell {
        workload: TransferWorkload::generate(
            &TransferConfig {
                accounts,
                transactions: TRANSACTIONS,
                recipients,
                ..Default::default()
            },
            SEED,
        ),
        family,
        param: param.to_string(),
        accounts,
    };
    // Low-conflict uniform at the D17 default ratio, then the 1:1 configurations
    // E9 measured, ending with the Zipf 2.0 block M2b is meant to fix.
    cells.push(transfer(
        1_000_000,
        Distribution::Uniform,
        "transfer",
        "uniform",
    ));
    cells.push(transfer(
        10_000,
        Distribution::Uniform,
        "transfer",
        "uniform",
    ));
    cells.push(transfer(
        10_000,
        Distribution::Zipf { s: 0.8 },
        "transfer",
        "zipf0.8",
    ));
    cells.push(transfer(
        10_000,
        Distribution::Zipf { s: 1.2 },
        "transfer",
        "zipf1.2",
    ));
    cells.push(transfer(
        10_000,
        Distribution::Zipf { s: 2.0 },
        "transfer",
        "zipf2.0",
    ));
    for payload in [0, 1_024, 8_192, 32_768] {
        cells.push(Cell {
            workload: ComputeWorkload::generate(
                &ComputeConfig {
                    accounts: 1_000_000,
                    transactions: TRANSACTIONS,
                    payload,
                },
                SEED,
            ),
            family: "compute",
            param: format!("{payload}b"),
            accounts: 1_000_000,
        });
    }
    cells
}

fn median<T: Copy + Ord>(mut v: Vec<T>) -> T {
    v.sort_unstable();
    v[v.len() / 2]
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1e3
}

fn m2a_baseline(out_dir: &Path) {
    std::fs::create_dir_all(out_dir).expect("create output dir");
    let mut csv = String::from(
        "scheduler,workload,param,accounts,transactions,seed,threads,runs,\
         dep_density,critical_path,rounds_median,executions_median,aborts_median,\
         abort_rate_median,wall_ms_median,wall_ms_min,wall_ms_max,seq_ms_median,\
         speedup_median,verified\n",
    );

    for cell in cells() {
        let w = &cell.workload;
        let block = SchedulerConfig::default().block;
        let profile = analyse(w, &block);
        eprintln!(
            "{} {} ({} accounts): density {:.3}, critical path {}",
            cell.family,
            cell.param,
            cell.accounts,
            profile.density(),
            profile.critical_path
        );

        let seq_cfg = SchedulerConfig::default();
        let _warmup = SequentialScheduler::new().execute_block(&w.txs, &w.base, &seq_cfg);
        let seq_runs: Vec<_> = (0..RUNS)
            .map(|_| SequentialScheduler::new().execute_block(&w.txs, &w.base, &seq_cfg))
            .collect();
        let reference = seq_runs[0].state.clone();
        let seq_times: Vec<Duration> = seq_runs.iter().map(|r| r.stats.wall_clock).collect();
        let seq_median = median(seq_times.clone());

        let row_prefix = |scheduler: &str, threads: usize| {
            format!(
                "{scheduler},{},{},{},{},{SEED},{threads},{RUNS},{:.6},{}",
                cell.family,
                cell.param,
                cell.accounts,
                w.txs.len(),
                profile.density(),
                profile.critical_path
            )
        };

        let _ = writeln!(
            csv,
            "{},1,{},0,0.000000,{:.3},{:.3},{:.3},{:.3},1.0000,true",
            row_prefix("sequential", 1),
            w.txs.len(),
            ms(seq_median),
            ms(*seq_times.iter().min().unwrap()),
            ms(*seq_times.iter().max().unwrap()),
            ms(seq_median),
        );

        for threads in THREADS {
            let cfg = SchedulerConfig {
                threads,
                ..Default::default()
            };
            let _warmup = RoundScheduler::new().execute_block(&w.txs, &w.base, &cfg);
            let runs: Vec<_> = (0..RUNS)
                .map(|_| RoundScheduler::new().execute_block(&w.txs, &w.base, &cfg))
                .collect();
            let verified = runs.iter().all(|r| r.state.diff(&reference).is_empty());
            assert!(
                verified,
                "{} {} at {threads} threads disagreed with sequential; refusing to record a timing",
                cell.family, cell.param
            );

            let times: Vec<Duration> = runs.iter().map(|r| r.stats.wall_clock).collect();
            let wall = median(times.clone());
            // Abort rate as a per-mille integer so the median stays over an Ord type.
            let abort_permille = median(
                runs.iter()
                    .map(|r| (r.stats.abort_rate() * 1e6) as u64)
                    .collect(),
            );
            let _ = writeln!(
                csv,
                "{},{},{},{},{:.6},{:.3},{:.3},{:.3},{:.3},{:.4},{verified}",
                row_prefix("blockstm-rounds", threads),
                median(runs.iter().map(|r| r.stats.rounds).collect()),
                median(runs.iter().map(|r| r.stats.executions).collect()),
                median(runs.iter().map(|r| r.stats.aborts).collect()),
                abort_permille as f64 / 1e6,
                ms(wall),
                ms(*times.iter().min().unwrap()),
                ms(*times.iter().max().unwrap()),
                ms(seq_median),
                seq_median.as_secs_f64() / wall.as_secs_f64(),
            );
            eprintln!(
                "  t{threads:<2}: {:>9.1} ms  speedup {:.2}x  rounds {}",
                ms(wall),
                seq_median.as_secs_f64() / wall.as_secs_f64(),
                median(runs.iter().map(|r| r.stats.rounds).collect())
            );
        }
    }

    std::fs::write(out_dir.join("m2a_baseline.csv"), csv).expect("write csv");
    std::fs::write(out_dir.join("m2a_baseline.meta.txt"), metadata()).expect("write metadata");
    eprintln!("wrote {}", out_dir.join("m2a_baseline.csv").display());
}

fn sh(cmd: &str, args: &[&str]) -> String {
    Command::new(cmd)
        .args(args)
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "unknown".into())
}

fn revm_version() -> String {
    let lock = include_str!("../../Cargo.lock");
    lock.split("[[package]]")
        .find(|p| p.contains("name = \"revm\"\n"))
        .and_then(|p| p.lines().find(|l| l.starts_with("version")))
        .map(|l| {
            l.trim_start_matches("version = ")
                .trim_matches('"')
                .to_string()
        })
        .unwrap_or_else(|| "unknown".into())
}

fn metadata() -> String {
    let dirty = !sh("git", &["status", "--porcelain", "--untracked-files=no"]).is_empty();
    format!(
        "# M2a baseline — internal diagnostic data\n\
         #\n\
         # Per D13, nothing here is reportable until the Anvil cross-validation\n\
         # gate passes. Per EXPERIMENTS.md section 6.1, thread counts above the\n\
         # performance-core count run partly on efficiency cores and do not\n\
         # measure scheduler behaviour alone.\n\
         \n\
         purpose: control group for M2b (ROADMAP M2b gate)\n\
         date: {}\n\
         git_commit: {}\n\
         git_dirty: {dirty}\n\
         seed: {SEED}\n\
         runs_per_cell: {RUNS} (median reported; one warmup run discarded)\n\
         threads: {THREADS:?}\n\
         transactions_per_block: {TRANSACTIONS}\n\
         verification: every parallel run's final state diffed against sequential before its timing was kept\n\
         allocator: {} (parevm::ALLOCATOR, as compiled into this binary)\n\
         revm: {}\n\
         rustc: {}\n\
         cpu: {}\n\
         cores_physical: {}\n\
         cores_performance: {}\n\
         cores_efficiency: {}\n\
         memory_bytes: {}\n\
         os: macOS {}\n",
        sh("date", &["-u", "+%Y-%m-%dT%H:%M:%SZ"]),
        sh("git", &["rev-parse", "HEAD"]),
        parevm::ALLOCATOR,
        revm_version(),
        sh("rustc", &["--version"]),
        sh("sysctl", &["-n", "machdep.cpu.brand_string"]),
        sh("sysctl", &["-n", "hw.physicalcpu"]),
        sh("sysctl", &["-n", "hw.perflevel0.physicalcpu"]),
        sh("sysctl", &["-n", "hw.perflevel1.physicalcpu"]),
        sh("sysctl", &["-n", "hw.memsize"]),
        sh("sw_vers", &["-productVersion"]),
    )
}
