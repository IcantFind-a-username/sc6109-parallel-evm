# sc6109-parallel-evm

A parallel transaction execution engine for EVM workloads — SC6109 Blockchain
Scalability, course project Option 5.

Real EVM bytecode (revm 41, Solidity contracts compiled by Foundry), real
threads, three schedulers, and an answer to the brief's question: **when does
parallel execution help, and when does it not?**

**→ [Read the report](docs/REPORT.md)**

## Result in one table

Block-STM on six performance cores (Apple M3 Pro), 2,000-transaction blocks,
every run verified against sequential execution:

| Workload | Speedup |
| --- | --- |
| Compute-heavy calls, independent | **5.6×** (92% of linear) |
| ERC-20 transfers among many holders | **2.8×** |
| Plain ETH transfers, independent | 1.4× — too little work per transaction |
| ERC-20 with conflicts detected per *account* | 0.5× — every transfer collides in the token |
| NFT mint / one AMM pool (a single dependency chain) | **0.43–0.51×** — slower than sequential |

Parallelism pays when transactions are independent, each does real work, and
the cores are real. Lose any one and it stops paying; lose the first and it
costs.

![Speedup by thread count](docs/figures/fig1_speedup_by_threads.png)

## Design

| Scheduler | Model |
| --- | --- |
| **Sequential** | Baseline and definition of correct |
| **M2a — round-based** | Execute all in parallel, validate, re-execute failures, repeat. Simple; quadratic on dependency chains |
| **M2b — Block-STM** | Collaborative scheduler with multi-version memory, ESTIMATE markers and dependency parking (Gelashvili et al.) |
| **M3 — static** | Access sets derived as an EIP-7928 block builder would; conflict-free levels run in parallel |

All share one execution path over revm. Reads are captured at the database
boundary by the only type that implements revm's `DatabaseRef`; conflicts are
detected per storage slot or per account; the block beneficiary is a
commutative accumulator kept out of conflict detection. See
[docs/DESIGN.md](docs/DESIGN.md).

## Correctness

- **Differential testing:** every parallel scheduler agrees slot-for-slot with
  an independent sequential engine on 1,000 seeds per workload at 2–12 threads.
- **Cross-validation:** the sequential engine agrees with an Anvil node on nine
  workloads ([results/anvil_crosscheck.txt](results/anvil_crosscheck.txt)).
- **Mutation testing:** injected bugs in the store and schedulers are caught by
  the suite, which is how two gaps in the tests themselves were found and closed.
- **Benchmarks verify too:** every timed run is diffed against sequential before
  its timing is kept.

## Reproduce

Requires Rust 1.91+; Foundry only to rebuild the contracts or run the Anvil
cross-check (compiled bytecode is committed).

```bash
cd engine
cargo test                                          # unit + differential tests
cargo test --release -- --ignored                   # full gates and stress tests
cargo run --release --bin bench -- final            # results/final/sweep.csv
cargo run --release --bin bench -- alloc            # results/final/alloc_mimalloc.csv
cargo run --release --no-default-features --bin bench -- alloc   # alloc_system.csv
cargo run --release --bin bench -- export           # Anvil cross-check input
python3 ../scripts/anvil_crosscheck.py ../results/scratch/crosscheck.json
python3 ../scripts/plot.py                          # docs/figures/
```

## Layout

```
engine/            Rust crate `parevm`
  src/state/       state views, the read recorder, sequential state
  src/mv.rs        multi-version memory
  src/sched/       sequential, rounds (M2a), blockstm/ (M2b), static_sched (M3)
  src/workload/    transfer, compute, contract generators; dependency analysis
  src/bin/         demo, bench
  tests/           differential, property and regression tests
  assets/          compiled contract bytecode
contracts/         Solidity workloads and forge tests
scripts/           Anvil cross-check, bytecode extraction, plotting
results/           raw benchmark CSV with metadata
docs/              report, design, experiments, roadmap, risks, figures
```

## Documentation

| Doc | Contents |
| --- | --- |
| [docs/REPORT.md](docs/REPORT.md) | The report: design, correctness, results, when parallelism helps |
| [docs/DESIGN.md](docs/DESIGN.md) | Architecture and the determinism argument |
| [docs/EXPERIMENTS.md](docs/EXPERIMENTS.md) | Experimental design and measurement hygiene |
| [DECISIONS.md](DECISIONS.md) | Architecture decisions, with what was rejected |
| [docs/ROADMAP.md](docs/ROADMAP.md) | Milestones and gates |
| [docs/RISKS.md](docs/RISKS.md) | Risks, with symptoms and mitigations |
| [docs/AI_USAGE.md](docs/AI_USAGE.md) | AI usage log |

## Prior work

An independent implementation following the
[Block-STM paper](https://arxiv.org/abs/2203.06871); no existing parallel-EVM
implementation was used.
