# sc6109-parallel-evm

A parallel transaction execution engine for EVM workloads, built as a course
project for **SC6109 — Blockchain Scalability** (Option 5).

## What this is

Most blockchains execute transactions sequentially, even when the vast majority
of them do not touch the same state. This project builds a scheduler that
detects non-conflicting transactions, executes them in parallel over a real EVM,
and measures **when parallel execution helps — and when it does not**.

The headline result we are after is not only a speedup curve. It is the negative
one: workloads that funnel every transaction through a single hot storage slot
(NFT mints against `totalSupply`, swaps against one AMM pool) degrade to
sequential throughput or worse, because abort-and-retry work is pure overhead.

## Design

Three execution strategies, compared against each other on identical workloads:

| Strategy | Model | Notes |
| --- | --- | --- |
| **Sequential** | Baseline | Single-threaded, defines ground truth |
| **Static scheduling** | Solana Sealevel-style | Transactions declare read/write sets up front; disjoint sets run on separate threads. No aborts, but access sets are often not knowable ahead of time on the EVM |
| **Optimistic (Block-STM)** | Speculative + validate | All transactions run optimistically against multi-version memory; a transaction that read a value later written by a lower-indexed transaction is aborted and re-executed |

Block-STM is the primary contribution. Its correctness requirement is strict:
the final state must be **bit-identical to sequential execution**.

## Stack

- **Execution** — Rust + [revm](https://github.com/bluealloy/revm) as a library, running real EVM bytecode
- **Parallelism** — `rayon` / thread pool over a `DashMap`-backed multi-version store
- **Workloads** — Solidity contracts compiled with Foundry, fed to the engine as bytecode
- **Analysis** — CSV output, plotted with Python/matplotlib

## Workloads

Conflict density is the independent variable:

| Workload | Conflict profile |
| --- | --- |
| ERC-20 transfers, random recipients | Near-zero conflict — expect near-linear speedup |
| ERC-20 transfers, Zipf-distributed hot accounts | Tunable, moderate conflict |
| NFT mint | Every transaction writes `totalSupply` — total conflict |
| AMM swaps against one pool | Total conflict |

## Metrics

- Speedup vs. thread count (1 / 2 / 4 / 8 / 16)
- Speedup vs. conflict rate
- Abort rate and re-execution count, including per-transaction distribution
- Critical path length (theoretical speedup ceiling)
- Mean wall-clock time per transaction

## Correctness

Two independent checks, both required before any benchmark number is trusted:

1. **Differential testing** — sequential and parallel execution of the same
   transaction batch must agree slot-for-slot, over randomized seeds, in CI.
2. **Cross-validation against a real EVM** — the same batch replayed serially on
   Anvil must produce matching balances and state.

## Relationship to prior work

This is an independent implementation following the
[Block-STM paper](https://arxiv.org/abs/2203.06871). It is not derived from
[RISE's `pevm`](https://github.com/risechain/pevm) or any other existing
Rust Block-STM implementation.

## Status

Planning. No implementation yet.

## Layout (planned)

```
engine/      Rust: multi-version memory, schedulers, revm integration
contracts/   Solidity workload contracts (Foundry)
bench/       Workload generation and benchmark harness
analysis/    Plotting scripts
results/     CSV output (gitignored)
docs/        Report, design notes, AI usage log
```
