# Experiments

What we measure, on what, and what would make a result trustworthy.

---

## 1. What the brief asks for

> "Compare throughput and execution time against sequential processing."
> "Produce a report explaining when parallel execution helps and when it does not."

The second sentence is the one that carries marks. A speedup curve is expected;
a rigorous account of the failure regime is what distinguishes the project.

---

## 2. Independent variables

| Variable | Levels |
| --- | --- |
| Scheduler | sequential, static, block-stm |
| Thread count | 1, 2, 4, 8, 16 |
| Workload | erc20-random, erc20-zipf, nft-mint, amm-swap |
| Conflict parameter | Zipf `s` swept across ~8 points |
| Batch size | 1000 transactions (fixed); 100/1000/10000 as a secondary sweep |

Sequential at 1 thread is the baseline for every speedup figure.

---

## 3. Workloads

Conflict density is the axis the whole study turns on. All four are written in
Solidity, compiled with Foundry, and executed as real bytecode.

| Workload | Contract | Conflict profile | Expected result |
| --- | --- | --- | --- |
| `erc20-random` | Standard ERC-20 | Senders and recipients drawn uniformly from a large account set — collisions rare | Near-linear speedup; the optimistic case |
| `erc20-zipf` | Same contract | Recipients drawn from a Zipf distribution; `s` tunes hot-account skew | The interesting middle; speedup should degrade smoothly as `s` rises |
| `nft-mint` | ERC-721 with `totalSupply` counter | Every transaction read-modify-writes one slot | Speedup collapses to ≈1.0 or below |
| `amm-swap` | Constant-product pool | Every transaction touches the same two reserves | Same collapse, different mechanism |

`erc20-zipf` is the workload that produces the headline conflict-rate figure, so
it deserves the most parameter points.

### Why NFT mint matters most

It is the cleanest possible demonstration that parallel execution has a ceiling
set by data dependencies, not by cores. Under `nft-mint`, every transaction
depends on its predecessor, the critical path equals the batch length, and the
theoretical speedup is exactly 1.0. Any *measured* speedup below 1.0 is pure
abort-and-retry overhead — and quantifying that overhead is the most valuable
number in the report.

Run this one properly. It is not a throwaway control.

---

## 4. Metrics

### Primary

- **Speedup** — sequential wall-clock ÷ parallel wall-clock, same batch, same machine
- **Throughput** — transactions per second
- **Abort rate** — aborted executions ÷ total executions
- **Re-execution count** — total incarnations beyond the first

### Secondary

- **Per-transaction abort distribution**, not just the aggregate rate. This is
  what answers the brief's "explain when rollback happens" — a long tail on a
  few transactions tells a different story than uniform low-level aborting, and
  under `erc20-zipf` we expect aborts to concentrate on transactions touching
  hot accounts.
- **Critical path length** — the longest chain of true dependencies in the
  batch, computed offline. This is the theoretical speedup ceiling, and plotting
  measured speedup against it shows how much of the gap is scheduler overhead
  versus inherent serialisation.
- **Mean and p99 wall-clock per transaction**
- **Peak memory** of `MVMemory` — multi-version storage is not free, and the
  memory cost of speculation is a legitimate trade-off to report

---

## 5. Headline figures

Two figures carry the presentation. Build everything else around them.

**Figure 1 — Speedup × thread count.** One line per workload, threads on a log-2
x-axis, with the linear-speedup diagonal drawn for reference. Tells the whole
story at a glance: `erc20-random` tracks the diagonal, `nft-mint` is flat at 1.

**Figure 2 — Speedup × conflict rate.** `erc20-zipf` with `s` swept, at fixed
thread count, with abort rate on a secondary axis. Shows the degradation curve
and, critically, the crossover point where parallel execution becomes *worse*
than sequential.

Supporting figures: abort-rate distribution, measured versus critical-path-bound
speedup, batch-size scaling.

---

## 6. Measurement hygiene

Numbers that cannot be reproduced are worth nothing, and a grader who cannot
reproduce them will assume the worst.

- Record machine spec, CPU model, physical core count, and whether SMT is on
- Fix the thread pool size explicitly; never rely on rayon's default
- Discard warmup iterations; report median of at least 5 runs with min/max
- Seed every workload generator and commit the seeds alongside the CSV
- Pin the revm version in the results metadata
- Run on a quiet machine — close everything, disable turbo if the variance is
  unacceptable, and say in the report what you did
- Commit raw CSV, not just figures. Plots are regenerable; measurements are not.

Thread counts above the physical core count will show sublinear or negative
returns for reasons unrelated to conflicts. Note the core count on Figure 1 so
that effect is not misread as a scheduler property.

---

## 7. Correctness checks

Both are milestone gates in [ROADMAP.md](ROADMAP.md), not optional extras.

**Differential testing.** Sequential and parallel execution of an identical
batch must agree slot-for-slot and account-for-account. Run across randomised
seeds, all four workloads, and every thread count, in CI. Target 1000 seeds.
Concurrency bugs are probabilistic; a handful of runs proves nothing.

**Anvil cross-validation.** Replay the same batch serially on Anvil and compare
final balances and storage against our engine. This is what licenses the claim
that we execute *real* EVM semantics rather than a convenient approximation, and
it is the most persuasive slide in the deck.

---

## 8. Bonus: mainnet block replay

Not required by the brief. High value if reached.

Pull transactions from a handful of real mainnet blocks over a public RPC,
replay them through the engine, and measure the **actual available parallelism**
in production traffic. Report critical path length and achieved speedup per
block.

The payoff is a single sentence in the defence — "on real mainnet blocks we
measured an average available parallelism of X" — which moves the project from
"we built a benchmark" to "we measured the world". Expect X to be
unimpressive, and say so: real blocks are dense with DEX and stablecoin activity
that concentrates on a few hot contracts. A low number is a finding, and it
converges with the `nft-mint` result rather than contradicting it.

This is first on the scope-cut list. Do not let it displace M2.
