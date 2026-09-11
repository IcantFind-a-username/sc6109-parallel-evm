# Decisions

Architecture decision log. Append only — never edit or delete an entry. If a
decision is reversed, add a new entry that supersedes the old one and say so.

Each entry records what was decided, why, and **what was rejected**. The
rejected alternative is the part that matters: it is what shows the decision was
made rather than defaulted into.

---

## D1 — Option 5: parallel EVM execution engine · 2026-09-11

**Decided by:** team
**Status:** accepted

Six options were available. Chose Option 5.

**Why.** It has no external infrastructure dependency — no chain to deploy, no
testnet, no faucet, no wallet frontend. Every failure mode is in our own code
and therefore fixable. Options 2 and 4 (OP Stack, zkEVM) put four weeks of
devops on the critical path with failure modes we cannot repair. It also
produces quantitative results naturally, and the brief's requirement to explain
"when parallel execution does not help" is a question we can answer concretely
via hot-slot workloads.

**Rejected.** Option 6 (cross-rollup intent router) was the easiest to complete
but has a low ceiling — routing between two simulated rollups on fee and latency
is thin. Option 3 (DA benchmarking) was the closest competitor: lower workload,
explicitly permits simulation, but risks reading as a spreadsheet exercise.

---

## D2 — Rust + revm, not Go + go-ethereum · 2026-09-11

**Decided by:** user
**Status:** accepted, final

Execution substrate is revm embedded as a library.

**Why.** Real EVM bytecode with a production-grade implementation, real
parallelism, and the strongest version of the claim at the defence. The
brief explicitly permits a simplified simulator, so this is above the bar rather
than at it.

**Rejected.** Go + go-ethereum `core/vm` was proposed as a lower-risk path on
the assumption that Rust depth might be thin on the team. That assumption was
wrong — Rust and concurrency experience is confirmed — so the fallback is
withdrawn. **This decision does not get reopened if M2 gets hard**; the
contingency for a hard M2 is D5, not a rewrite.

---

## D3 — Implement `DatabaseRef`, not `Database` · 2026-09-11

**Decided by:** user, on analysis
**Status:** accepted

The state store is exposed to revm through `DatabaseRef` (`&self`), adapted with
`WrapDatabaseRef`, with the multi-version store behind an `Arc`.

**Why.** revm's `Database` trait takes `&mut self`, which cannot be shared
across worker threads without serialising it behind a lock — which would pin
measured speedup at 1.0 and masquerade as a scheduler bug.

**Rejected.** `Arc<Mutex<dyn Database>>`. Correct, but defeats the entire
purpose of the project.

---

## D4 — Beneficiary account exempt from conflict detection · 2026-09-11

**Decided by:** user, on analysis
**Status:** accepted

Gas fees to the block beneficiary do not participate in conflict detection. They
accumulate separately and are applied in one pass at commit.

**Why.** Every transaction pays the beneficiary, so treating that balance as a
normal read-modify-write makes all transactions mutually conflicting and pins
speedup at 1.0 on every workload. More importantly, the exemption is *correct*
rather than a workaround: the beneficiary balance is a commutative accumulator,
not a true data dependency. The Block-STM authors make the same observation.

**Rejected.** Disabling the beneficiary reward via revm's config flag. Simpler,
but it changes EVM semantics and weakens the claim that we execute real EVM
behaviour. The distinction between commutative accumulation and genuine
read-modify-write dependency is also a substantive point for the report, and
using the config flag discards it.

---

## D5 — Split M2 into round-based (M2a) and collaborative (M2b) · 2026-09-11

**Decided by:** user
**Status:** accepted

Block-STM is delivered in two stages with a decision point on 2026-10-08. M2a
executes in rounds (execute all, validate all, re-execute aborted, repeat). M2b
is the full collaborative scheduler with atomic indices and dependency tracking.

**Why.** M2 is the hardest component and sits on the critical path with roughly
four days of slack. M2a is independently correct and shippable, so a schedule
overrun degrades the result rather than destroying it. A correct round-based
scheduler with clean data scores better than a half-finished Block-STM.

**Rejected.** Going straight for the collaborative scheduler. Higher ceiling, no
floor.

---

## D6 — Course code is SC6109; course PDF stays out of the repo · 2026-09-11

**Decided by:** user
**Status:** accepted

Repository and report use SC6109. The assignment description PDF is gitignored
at repo root (`/*.pdf`, scoped so `docs/*.pdf` deliverables still track).

**Why.** The PDF is course material and not ours to redistribute, particularly
if the repository is public.

**Note.** The PDF's own filename says SC6019 and earlier notes recorded SC6019.
The user confirmed 6109. Worth one final check against NTULearn before the
report is submitted.

---

## Open

Decisions not yet made. Move them above when settled.

- **O1 — revm version to pin.** Needs a survey of what the current release
  exposes for `DatabaseRef`, `ResultAndState` and the beneficiary config, then
  a pin that does not move for the rest of the project. Blocks M0.
- **O2 — Multi-version store concrete type.** `DashMap<(Address, U256),
  BTreeMap<TxIdx, WriteEntry>>` is the design sketch; the real choice depends on
  measured contention. Defer until M2a has numbers.
- **O3 — Benchmark machine.** The final sweep must run entirely on one machine.
  See the note on heterogeneous cores in
  [docs/EXPERIMENTS.md](docs/EXPERIMENTS.md).
- **O4 — Core A second seat.** `MVMemory` and the validation path should not be
  reviewed only by their author.
