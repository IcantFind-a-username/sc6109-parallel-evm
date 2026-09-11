# Roadmap

**Course:** SC6109 — Blockchain Scalability, Option 5
**Deadline:** 2026-10-25
**Planning date:** 2026-09-11 (6.3 weeks / 44 days available)

Deliverables: source code, 10-minute video, slides.

---

## Guiding principle

Correctness gates come before performance work. A speedup number measured on an
engine that does not reproduce sequential state is worthless, and worse, it will
not survive a single question at the defence. Every milestone below is defined
by a **gate** — a check that either passes or does not. Do not start the next
milestone until the current gate is green.

---

## M0 — Skeleton runs · by **2026-09-17**

Get one transaction through a real EVM and one contract compiled. Nothing else.

- [x] Rust workspace created, `revm` pinned to an exact version in `Cargo.toml`
- [ ] Foundry project under `contracts/`, `forge build` produces bytecode
- [x] A value transfer executes through `revm` against our own state stack
      (ERC-20 pending Foundry)
- [x] CI runs `cargo fmt`, `clippy -D warnings` and `cargo test` on push
      (`forge build` pending Foundry)

**Gate:** `cargo run --bin demo` prints a post-transfer balance that is
arithmetically correct.

> Why this is a milestone at all: the revm API has changed shape across major
> versions. Discovering that in week 4 is expensive. Discover it in week 1.

---

## M1 — Sequential baseline, verified · by **2026-09-27**

This is the most important milestone in the project. Everything after it is
addition; if this slips, replan the scope rather than the gate.

- [x] `SequentialExecutor` runs an arbitrary batch of transactions
- [x] State store with account-level and slot-level access
- [x] Workload generator emits a reproducible batch from a seed (transfers;
      ERC-20 pending Foundry)
- [x] **Differential test harness** — compares two executors slot-for-slot
- [ ] **Anvil cross-validation** — same batch replayed serially on Anvil,
      balances and storage compared against our engine

**Gate:** our sequential engine and Anvil agree on final state for at least
three distinct workloads.

---

## M2 — Block-STM correct · by **2026-10-11**

Split into two stages. Stage A is the fallback deliverable: if Stage B is not
done by Oct 8, ship Stage A and say so in the report. A correct round-based
scheduler beats a broken collaborative one.

### M2a — Round-based optimistic execution
Execute all transactions in parallel, validate all, re-execute the aborted set,
repeat until no aborts. Simpler, obviously correct, slightly less efficient.

- [x] Multi-version memory (`MVMemory`), slot granularity
- [x] Read-set capture via an instrumented `DatabaseRef`
- [x] Validation pass
- [x] Abort and re-execution with incarnation numbers
- [x] Termination guard at the proven bound of `n` rounds
- [ ] Differential sweep including a workload whose write set depends on its
      reads (ERC-20 reverting on insufficient balance) — the only end-to-end
      exercise of write retraction; see E8
- [ ] Account-granularity semantics decided (O5) and implemented

**Gate M2a:** differential test passes on 1000 random seeds × 4 workloads ×
{2,4,8,16} threads, in CI.

> **Status 2026-09-11:** passed on transfer workloads — 1000 seeds × 4 conflict
> levels × {2,4,6,8,12} threads, 20,000 block comparisons, all agreeing
> (`cargo test --release -- --ignored`). Thread counts follow EXPERIMENTS §6.1.
> ERC-20 and the other contract workloads join the sweep once Foundry is in.

### M2b — Collaborative scheduler (Block-STM proper)
Atomic `execution_idx` / `validation_idx`, per-transaction status, dependency
tracking with `ESTIMATE` markers, validation preferred at low indices.

**Gate M2b:** same differential test, plus measurable improvement over M2a.

---

## M3 — Static scheduler + full experiment sweep · by **2026-10-18**

- [ ] Access-set profiling pass — derive read/write sets via `ReadRecorder`
      ([DESIGN.md §7.2](DESIGN.md)), not from the workload generator
- [ ] `StaticScheduler` — grouped by declared access sets; framed as an
      EIP-7928 prototype ([DESIGN.md §7.1](DESIGN.md))
- [ ] All four workloads implemented and parameterised
- [ ] Benchmark machine fixed (see [EXPERIMENTS.md §6.1](EXPERIMENTS.md) —
      the M3 Pro's P/E core split constrains usable thread counts)
- [ ] Full sweep executed, raw CSV committed under `results/`
- [ ] Both headline figures generated

**Gate:** the two headline figures exist and are legible — speedup × thread
count, and speedup × conflict rate.

---

## M4 — Report, slides, video · by **2026-10-25**

- [ ] Report written, including the negative result as a first-class finding
- [ ] Slides
- [ ] 10-minute video recorded
- [ ] `docs/AI_USAGE.md` complete and honest
- [ ] README updated with actual results and how to reproduce them

**Gate:** a teammate who did not write the code can follow the README and
reproduce one figure.

---

## Schedule at a glance

| Window | Dates | Focus |
| --- | --- | --- |
| Week 1 | Sep 11 – Sep 17 | M0 skeleton |
| Week 2 | Sep 18 – Sep 27 | M1 sequential + verification |
| Weeks 3–4 | Sep 28 – Oct 11 | M2 Block-STM |
| Week 5 | Oct 12 – Oct 18 | M3 static scheduler + experiments |
| Week 6 | Oct 19 – Oct 25 | M4 writeup, slides, video |

There is roughly four days of slack in total. It is allocated to M2, which is
where it will be needed.

---

## Ownership

Names go in when the team confirms. Roles, not people, for now.

| Role | Scope | Milestones |
| --- | --- | --- |
| **Core A** (×2) | `MVMemory`, Block-STM scheduler, abort/retry | M2 |
| **Integration** | revm binding, static scheduler, Anvil cross-validation | M0, M1, M3 |
| **Workloads** | Solidity contracts, batch generator, conflict parameters | M0, M1, M3 |
| **Experiments** | Benchmark harness, sweep, figures, report, AI log | M3, M4 |

Core A is two people because it is the hardest and most schedule-critical piece,
and because `MVMemory` and the validation path should not be reviewed only by
their author. Pair on those two modules rather than splitting them. The
Experiments role picks up mainnet block replay (see
[EXPERIMENTS.md](EXPERIMENTS.md)) once M3 sweeps are running, to balance load.

**Stack decision is final.** Rust + revm, settled 2026-09-11 on confirmed Rust
capacity. The Go / go-ethereum alternative weighed during planning is withdrawn
— see [RISKS.md R8](RISKS.md). Do not reopen this if M2 gets hard; the
contingency for M2 is the M2a/M2b split, not a rewrite.

---

## Stretch goals, in priority order

Only after M3's gate is green. Both are outside the brief's requirements.

1. **Mainnet block replay** — replay real blocks, report measured available
   parallelism. An empirical finding, and the strongest single sentence
   available for the defence.
2. **Canonical EIP-7928 output** — emit a `BlockAccessList` via revm's
   `bal_builder`. Format conformance only; the EIP-7928 framing and the derived
   access sets (D12) already deliver the substance. Likely not done.

## Scope cuts, in the order we take them

If the schedule slips, cut from the bottom up. Decide early, not on Oct 24.

1. Both stretch goals above
2. M2b collaborative scheduler — ship M2a instead
3. High-thread data points, if the machine cannot produce stable numbers
4. AMM workload — NFT mint alone demonstrates the total-conflict case

**Never cut:** the differential test, the Anvil cross-validation, or the
negative result. Those are the graded core.
