//! Benchmark runner. Never part of `cargo test`.
//!
//! ```text
//! cargo run --release --bin bench -- final  [out-dir]   # every figure's data
//! cargo run --release --bin bench -- alloc  [out-dir]   # allocator comparison (run twice, see below)
//! cargo run --release --bin bench -- export [file]      # workloads for the Anvil cross-check
//! ```
//!
//! Every parallel run's final state is diffed against the sequential result,
//! outside the timed region, before its timing is kept. A timing taken on a
//! wrong result is not a measurement, and the runner refuses to record one.
//!
//! The allocator is fixed per binary (D15), so the allocator comparison is two
//! invocations: once as built by default, once built with
//! `--no-default-features`. Each writes a file named after its allocator.

use parevm::sched::Scheduler;
use parevm::workload::{
    analyse, ComputeConfig, ComputeWorkload, ContractConfig, ContractKind, ContractWorkload,
    DependencyProfile, Distribution, TransferConfig, TransferWorkload,
};
use parevm::{
    BlockOutcome, BlockStmScheduler, Granularity, RoundScheduler, SchedulerConfig,
    SequentialScheduler, StaticScheduler, Workload,
};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

const SEED: u64 = 2026;
const RUNS: usize = 5;
/// Block size for the main sweep (D24).
const BLOCK: usize = 2_000;
const THREADS: [usize; 6] = [1, 2, 4, 6, 8, 12];

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let dir = |default: &str| {
        args.get(1)
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(default))
    };
    match args.first().map(String::as_str) {
        Some("final") => final_sweep(&dir("../results/final")),
        Some("alloc") => allocator(&dir("../results/final")),
        Some("export") => {
            export::write(&dir("../results/scratch/crosscheck.json"));
        }
        _ => {
            eprintln!("usage: bench final [dir] | alloc [dir] | export [file]");
            std::process::exit(2);
        }
    }
}

/// One workload in the sweep, with its measured dependency structure.
struct Cell {
    workload: Workload,
    family: &'static str,
    param: String,
    accounts: usize,
    profile: DependencyProfile,
}

fn cell(workload: Workload, family: &'static str, param: &str, accounts: usize) -> Cell {
    let profile = analyse(&workload, &SchedulerConfig::default().block);
    Cell {
        workload,
        family,
        param: param.to_string(),
        accounts,
        profile,
    }
}

fn transfer(accounts: usize, recipients: Distribution, n: usize, param: &str) -> Cell {
    let w = TransferWorkload::generate(
        &TransferConfig {
            accounts,
            transactions: n,
            recipients,
            ..Default::default()
        },
        SEED,
    );
    cell(w, "transfer", param, accounts)
}

fn compute(payload: usize, n: usize) -> Cell {
    let accounts = 100 * n;
    let w = ComputeWorkload::generate(
        &ComputeConfig {
            accounts,
            transactions: n,
            payload,
        },
        SEED,
    );
    cell(w, "compute", &format!("{payload}b"), accounts)
}

fn contract(kind: ContractKind, accounts: usize, n: usize) -> Cell {
    let label = kind.label();
    let param = label.split_once('-').map_or("", |(_, p)| p).to_string();
    contract_as(kind, accounts, n, &param)
}

/// A contract workload under an explicit parameter label, for cells that
/// differ only in account ratio.
fn contract_as(kind: ContractKind, accounts: usize, n: usize, param: &str) -> Cell {
    let w = ContractWorkload::generate(
        &ContractConfig {
            kind,
            accounts,
            transactions: n,
        },
        SEED,
    );
    let label = kind.label();
    let family = label.split_once('-').map_or(label.as_str(), |(f, _)| f);
    let family: &'static str = Box::leak(family.to_string().into_boxed_str());
    cell(w, family, param, accounts)
}

/// The main sweep's workloads, spanning the conflict axis (transfers and
/// contracts) and the work-per-transaction axis (compute).
fn main_cells() -> Vec<Cell> {
    let n = BLOCK;
    let erc20 = ContractKind::Erc20 {
        recipients: Distribution::Uniform,
    };
    vec![
        // Uniform recipients across account-to-block ratios r = 100, 10, 4, 2, 1:
        // expected density 1 - (1 - e^(-4/r)) / (4/r), about 0.02, 0.18, 0.37,
        // 0.57 and 0.75, so Figure 2's x-axis has no gap between sparse and
        // dense (the first sweep had one from 0.02 to 0.75).
        transfer(100 * n, Distribution::Uniform, n, "uniform-sparse"),
        transfer(10 * n, Distribution::Uniform, n, "uniform-r10"),
        transfer(4 * n, Distribution::Uniform, n, "uniform-r4"),
        transfer(2 * n, Distribution::Uniform, n, "uniform-r2"),
        transfer(n, Distribution::Uniform, n, "uniform-dense"),
        transfer(n, Distribution::Zipf { s: 0.8 }, n, "zipf0.8"),
        transfer(n, Distribution::Zipf { s: 1.2 }, n, "zipf1.2"),
        transfer(n, Distribution::Zipf { s: 2.0 }, n, "zipf2.0"),
        compute(0, n),
        compute(1_024, n),
        compute(8_192, n),
        compute(32_768, n),
        // The same ratios for ERC-20: the same dependency structure as the
        // transfers above, more work per transaction.
        contract(erc20, 100 * n, n),
        contract_as(erc20, 10 * n, n, "uniform-r10"),
        contract_as(erc20, 4 * n, n, "uniform-r4"),
        contract_as(erc20, 2 * n, n, "uniform-r2"),
        contract(
            ContractKind::Erc20 {
                recipients: Distribution::Zipf { s: 1.2 },
            },
            n,
            n,
        ),
        contract(
            ContractKind::Erc20Tight {
                recipients: Distribution::Zipf { s: 1.0 },
            },
            n / 4,
            n,
        ),
        contract(ContractKind::NftMint, 10 * n, n),
        contract(ContractKind::AmmSwap, 10 * n, n),
    ]
}

fn median<T: Copy + PartialOrd>(mut v: Vec<T>) -> T {
    v.sort_by(|a, b| a.partial_cmp(b).expect("comparable"));
    v[v.len() / 2]
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1e3
}

const HEADER: &str = "experiment,scheduler,workload,param,granularity,accounts,transactions,seed,\
threads,runs,dep_density,critical_path,ceiling,rounds_median,executions_median,aborts_median,\
waits_median,refusals_median,abort_rate_median,wall_ms_median,wall_ms_min,wall_ms_max,\
prep_ms_median,seq_ms_median,speedup_median,verified\n";

/// Runs `scheduler` RUNS times after a warmup, verifies every run against the
/// sequential reference, and appends one CSV row.
#[allow(clippy::too_many_arguments)]
fn measure<S: Scheduler>(
    csv: &mut String,
    experiment: &str,
    scheduler: &S,
    c: &Cell,
    threads: usize,
    granularity: Granularity,
    reference: &BlockOutcome,
    seq_ms: f64,
) {
    let w = &c.workload;
    let config = SchedulerConfig {
        threads,
        granularity,
        ..Default::default()
    };
    let _warmup = scheduler.execute_block(&w.txs, &w.base, &config);
    let runs: Vec<BlockOutcome> = (0..RUNS)
        .map(|_| scheduler.execute_block(&w.txs, &w.base, &config))
        .collect();
    let verified = runs
        .iter()
        .all(|r| r.state.diff(&reference.state).is_empty());
    assert!(
        verified,
        "{} on {} {} at {threads} threads disagreed with sequential; refusing to record a timing",
        scheduler.name(),
        c.family,
        c.param
    );
    let stat = |f: &dyn Fn(&BlockOutcome) -> f64| median(runs.iter().map(f).collect::<Vec<_>>());
    let wall = stat(&|r| ms(r.stats.wall_clock));
    let walls: Vec<f64> = runs.iter().map(|r| ms(r.stats.wall_clock)).collect();
    let g = match granularity {
        Granularity::Slot => "slot",
        Granularity::Account => "account",
    };
    let _ = writeln!(
        csv,
        "{experiment},{},{},{},{g},{},{},{SEED},{threads},{RUNS},{:.6},{},{:.2},{},{},{},{},{},{:.6},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{verified}",
        scheduler.name(),
        c.family,
        c.param,
        c.accounts,
        w.txs.len(),
        c.profile.density(),
        c.profile.critical_path,
        c.profile.parallelism_ceiling(),
        stat(&|r| r.stats.rounds as f64),
        stat(&|r| r.stats.executions as f64),
        stat(&|r| r.stats.aborts as f64),
        stat(&|r| r.stats.dependency_waits as f64),
        stat(&|r| r.stats.speculative_refusals as f64),
        stat(&|r| r.stats.abort_rate()),
        wall,
        walls.iter().cloned().fold(f64::INFINITY, f64::min),
        walls.iter().cloned().fold(0.0, f64::max),
        stat(&|r| ms(r.stats.preparation)),
        seq_ms,
        seq_ms / wall,
    );
    eprintln!(
        "  {:<16} {g:<7} t{threads:<2} {wall:>10.2} ms  {:>6.2}x",
        scheduler.name(),
        seq_ms / wall
    );
}

/// The sequential baseline: warmup, RUNS runs, median; the first run's state is
/// the reference every parallel run is checked against.
fn baseline(csv: &mut String, experiment: &str, c: &Cell) -> (BlockOutcome, f64) {
    let w = &c.workload;
    let config = SchedulerConfig::default();
    let s = SequentialScheduler::new();
    let _warmup = s.execute_block(&w.txs, &w.base, &config);
    let runs: Vec<BlockOutcome> = (0..RUNS)
        .map(|_| s.execute_block(&w.txs, &w.base, &config))
        .collect();
    let seq = median(runs.iter().map(|r| ms(r.stats.wall_clock)).collect());
    let reference = runs.into_iter().next().expect("runs");
    measure(
        csv,
        experiment,
        &s,
        c,
        1,
        Granularity::Slot,
        &reference,
        seq,
    );
    (reference, seq)
}

fn final_sweep(out: &Path) {
    std::fs::create_dir_all(out).expect("create output dir");
    let meta = metadata("final sweep: every figure's data");
    let mut csv = String::from(HEADER);

    // Main sweep: every scheduler, every workload, every thread count.
    for c in main_cells() {
        eprintln!(
            "{} {} — density {:.3}, critical path {}",
            c.family,
            c.param,
            c.profile.density(),
            c.profile.critical_path
        );
        let (reference, seq) = baseline(&mut csv, "main", &c);
        for t in THREADS {
            measure(
                &mut csv,
                "main",
                &RoundScheduler::new(),
                &c,
                t,
                Granularity::Slot,
                &reference,
                seq,
            );
            measure(
                &mut csv,
                "main",
                &BlockStmScheduler::new(),
                &c,
                t,
                Granularity::Slot,
                &reference,
                seq,
            );
            measure(
                &mut csv,
                "main",
                &StaticScheduler::new(),
                &c,
                t,
                Granularity::Slot,
                &reference,
                seq,
            );
        }
    }

    // D9: false conflicts from coarse detection. Only workloads whose accounts
    // hold several slots can differ; transfers are a control that should not.
    let n = BLOCK;
    for c in [
        contract(
            ContractKind::Erc20 {
                recipients: Distribution::Uniform,
            },
            100 * n,
            n,
        ),
        contract(
            ContractKind::Erc20 {
                recipients: Distribution::Zipf { s: 1.2 },
            },
            n,
            n,
        ),
        contract(ContractKind::AmmSwap, 10 * n, n),
        transfer(100 * n, Distribution::Uniform, n, "uniform-sparse"),
    ] {
        eprintln!("granularity: {} {}", c.family, c.param);
        let (reference, seq) = baseline(&mut csv, "granularity", &c);
        for g in [Granularity::Slot, Granularity::Account] {
            measure(
                &mut csv,
                "granularity",
                &RoundScheduler::new(),
                &c,
                6,
                g,
                &reference,
                seq,
            );
            measure(
                &mut csv,
                "granularity",
                &BlockStmScheduler::new(),
                &c,
                6,
                g,
                &reference,
                seq,
            );
            measure(
                &mut csv,
                "granularity",
                &StaticScheduler::new(),
                &c,
                6,
                g,
                &reference,
                seq,
            );
        }
    }

    // Block size: does per-block overhead explain small-block results?
    for size in [500, 2_000, 8_000] {
        for c in [
            contract(
                ContractKind::Erc20 {
                    recipients: Distribution::Uniform,
                },
                100 * size,
                size,
            ),
            compute(1_024, size),
        ] {
            eprintln!("batch {size}: {} {}", c.family, c.param);
            let (reference, seq) = baseline(&mut csv, "batch", &c);
            measure(
                &mut csv,
                "batch",
                &BlockStmScheduler::new(),
                &c,
                6,
                Granularity::Slot,
                &reference,
                seq,
            );
            measure(
                &mut csv,
                "batch",
                &StaticScheduler::new(),
                &c,
                6,
                Granularity::Slot,
                &reference,
                seq,
            );
        }
    }

    std::fs::write(out.join("sweep.csv"), csv).expect("write csv");
    std::fs::write(out.join("sweep.meta.txt"), meta).expect("write metadata");
    eprintln!("wrote {}", out.join("sweep.csv").display());
}

/// A small fixed subset, run once per allocator build (D15).
fn allocator(out: &Path) {
    std::fs::create_dir_all(out).expect("create output dir");
    let meta = metadata("allocator comparison");
    let mut csv = String::from(HEADER);
    let n = BLOCK;
    for c in [
        transfer(100 * n, Distribution::Uniform, n, "uniform-sparse"),
        compute(1_024, n),
        contract(
            ContractKind::Erc20 {
                recipients: Distribution::Uniform,
            },
            100 * n,
            n,
        ),
    ] {
        let (reference, seq) = baseline(&mut csv, "alloc", &c);
        for t in [1, 2, 4, 6] {
            measure(
                &mut csv,
                "alloc",
                &BlockStmScheduler::new(),
                &c,
                t,
                Granularity::Slot,
                &reference,
                seq,
            );
        }
    }
    let name = format!("alloc_{}", parevm::ALLOCATOR);
    std::fs::write(out.join(format!("{name}.csv")), csv).expect("write csv");
    std::fs::write(out.join(format!("{name}.meta.txt")), meta).expect("write metadata");
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

/// Captured before the first run: the commit and dirty flag must describe the
/// code that produced the numbers.
fn metadata(purpose: &str) -> String {
    let dirty = !sh("git", &["status", "--porcelain", "--untracked-files=no"]).is_empty();
    format!(
        "# {purpose}\n\
         #\n\
         # Thread counts above the performance-core count run partly on\n\
         # efficiency cores (EXPERIMENTS.md section 6.1): 1-6 threads are the\n\
         # primary result; 8 and 12 are reported and annotated as such.\n\
         \n\
         date: {}\n\
         git_commit: {}\n\
         git_dirty: {dirty}\n\
         seed: {SEED}\n\
         runs_per_cell: {RUNS} (median reported; one warmup run discarded)\n\
         threads: {THREADS:?}\n\
         block_size_main: {BLOCK}\n\
         verification: every parallel run's final state diffed against sequential before its timing was kept\n\
         timing: thread pools and per-block allocations are built before the timed region for every scheduler; the static scheduler's access-set derivation is excluded and reported as prep_ms\n\
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

/// Workloads exported for cross-validation against Anvil (M1 gate).
///
/// Each carries its base state, its transactions, and what our sequential
/// engine produced: per-transaction success and the final state snapshot.
/// `scripts/anvil_crosscheck.py` replays them on Anvil and compares.
mod export {
    use parevm::exec::execute;
    use parevm::state::SimpleState;
    use parevm::workload::compute::SHA256;
    use parevm::workload::{
        ComputeConfig, ComputeWorkload, ContractConfig, ContractKind, ContractWorkload,
        Distribution, TransferConfig, TransferWorkload,
    };
    use parevm::{Granularity, SchedulerConfig, Workload};
    use revm::context::TxEnv;
    use revm::primitives::{hex, Address, TxKind, U256};
    use std::fmt::Write as _;
    use std::path::Path;

    fn workloads() -> Vec<Workload> {
        let transfer = |recipients, gas_price| {
            TransferWorkload::generate(
                &TransferConfig {
                    accounts: 300,
                    transactions: 300,
                    recipients,
                    gas_price,
                    ..Default::default()
                },
                11,
            )
        };
        let contract = |kind| {
            ContractWorkload::generate(
                &ContractConfig {
                    kind,
                    accounts: 150,
                    transactions: 300,
                },
                11,
            )
        };
        let mut fees = transfer(Distribution::Zipf { s: 1.0 }, 1_000_000_000);
        fees.name = "transfer-fees-1gwei".into();
        vec![
            transfer(Distribution::Uniform, 0),
            transfer(Distribution::Zipf { s: 1.2 }, 0),
            fees,
            ComputeWorkload::generate(
                &ComputeConfig {
                    accounts: 100,
                    transactions: 200,
                    payload: 64,
                },
                11,
            ),
            contract(ContractKind::Erc20 {
                recipients: Distribution::Zipf { s: 1.0 },
            }),
            contract(ContractKind::Erc20Tight {
                recipients: Distribution::Zipf { s: 1.0 },
            }),
            contract(ContractKind::NftMint),
            contract(ContractKind::AmmSwap),
            eip161(),
        ]
    }

    /// Zero-value transfers to addresses that do not exist, and precompile
    /// calls: every one leaves a touched empty account, which EIP-161 deletes.
    fn eip161() -> Workload {
        let mut w = TransferWorkload::generate(
            &TransferConfig {
                accounts: 20,
                transactions: 0,
                ..Default::default()
            },
            11,
        );
        let senders: Vec<Address> = w.base.accounts().map(|(a, _)| *a).collect();
        for (i, from) in senders.iter().enumerate() {
            let mut ghost = [0u8; 20];
            ghost[0] = 0xDE;
            ghost[19] = i as u8;
            let to = if i % 2 == 0 {
                Address::from(ghost)
            } else {
                SHA256
            };
            w.txs.push(
                TxEnv::builder()
                    .caller(*from)
                    .kind(TxKind::Call(to))
                    .value(U256::ZERO)
                    .gas_limit(100_000)
                    .gas_price(0)
                    .nonce(0)
                    .build()
                    .unwrap(),
            );
        }
        w.name = "eip161-empty-accounts".into();
        w
    }

    fn sep(i: usize) -> &'static str {
        if i > 0 {
            ","
        } else {
            ""
        }
    }

    pub fn write(out: &Path) {
        let block = SchedulerConfig::default().block;
        let mut json = format!(
            "{{\"beneficiary\":\"{}\",\"workloads\":[",
            block.beneficiary
        );
        for (wi, w) in workloads().iter().enumerate() {
            let mut state = SimpleState::new(w.base.clone());
            let mut success = Vec::new();
            for (i, tx) in w.txs.iter().enumerate() {
                let done = execute(state.view(), tx.clone(), &block, Granularity::Slot)
                    .outcome
                    .unwrap_or_else(|e| panic!("{} tx {i} refused: {e:?}", w.name));
                success.push(done.is_success());
                state.commit(done.writes);
            }
            let snapshot = parevm::sched::sequential::snapshot(&state);

            let _ = write!(
                json,
                "{}{{\"name\":\"{}\",\"seed\":{},\"accounts\":[",
                sep(wi),
                w.name,
                w.seed
            );
            for (i, (a, info)) in w.base.accounts().enumerate() {
                let code = info
                    .code
                    .as_ref()
                    .map(|c| hex::encode(c.original_bytes()))
                    .unwrap_or_default();
                let _ = write!(
                    json,
                    "{}{{\"address\":\"{a}\",\"balance\":\"{:#x}\",\"nonce\":{},\"code\":\"0x{code}\"}}",
                    sep(i),
                    info.balance,
                    info.nonce
                );
            }
            json.push_str("],\"storage\":[");
            for (i, ((a, slot), v)) in w.base.slots().enumerate() {
                let _ = write!(
                    json,
                    "{}{{\"address\":\"{a}\",\"slot\":\"{slot:#x}\",\"value\":\"{v:#x}\"}}",
                    sep(i)
                );
            }
            json.push_str("],\"txs\":[");
            for (i, tx) in w.txs.iter().enumerate() {
                let TxKind::Call(to) = tx.kind else {
                    panic!("contract creation is not exported")
                };
                let _ = write!(
                    json,
                    "{}{{\"from\":\"{}\",\"to\":\"{to}\",\"value\":\"{:#x}\",\"data\":\"0x{}\",\
                     \"gas\":\"{:#x}\",\"gasPrice\":\"{:#x}\",\"nonce\":\"{:#x}\",\"success\":{}}}",
                    sep(i),
                    tx.caller,
                    tx.value,
                    hex::encode(&tx.data),
                    tx.gas_limit,
                    tx.gas_price,
                    tx.nonce,
                    success[i]
                );
            }
            json.push_str("],\"expected\":{\"accounts\":[");
            for (i, (a, acc)) in snapshot.accounts.iter().enumerate() {
                let _ = write!(
                    json,
                    "{}{{\"address\":\"{a}\",\"balance\":\"{:#x}\",\"nonce\":{}}}",
                    sep(i),
                    acc.balance,
                    acc.nonce
                );
            }
            json.push_str("],\"storage\":[");
            for (i, ((a, slot), v)) in snapshot.storage.iter().enumerate() {
                let _ = write!(
                    json,
                    "{}{{\"address\":\"{a}\",\"slot\":\"{slot:#x}\",\"value\":\"{v:#x}\"}}",
                    sep(i)
                );
            }
            json.push_str("]}}");
            eprintln!("exported {} ({} txs)", w.name, w.txs.len());
        }
        json.push_str("]}");
        if let Some(dir) = out.parent() {
            std::fs::create_dir_all(dir).expect("create export dir");
        }
        std::fs::write(out, json).expect("write export");
        eprintln!("wrote {}", out.display());
    }
}
