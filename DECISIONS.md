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

## D7 — Core architecture: one execution path, decorator stack · 2026-09-11

**Decided by:** user, on proposal
**Status:** accepted

All three schedulers share one execution path. They differ only in which
`DatabaseRef` is handed to revm and what they do with the result. State access
is layered:

```
revm → WrapDatabaseRef → ReadRecorder<V: StateView> → V → BaseState
                                                      ├── MVView   (parallel)
                                                      └── SimpleView (sequential)
```

`StateView` is our own private trait. Only `ReadRecorder` implements revm's
`DatabaseRef`, so no read can structurally bypass logging, and all logging lives
in one generic helper.

**Why.** Read-set capture is the top correctness risk (R1). Making it
unbypassable by construction is worth more than discipline.

**Rejected.** Letting each scheduler own its execution loop. Three code paths
means three places for a read-set bug to hide.

---

## D8 — Sequential baseline does not share the multi-version store · 2026-09-11

**Decided by:** user, on proposal
**Status:** accepted

The sequential executor uses `SimpleView` over a plain `HashMap`, written
deliberately simply, with no code shared with `MVMemory`.

**Why.** `MVMemory` driven single-threaded in index order *does* produce
sequential semantics, and reusing it would be elegant. But then a bug in
`MVMemory` corrupts both sides of the differential test identically, the two
agree, and the test reports success. Differential testing only has power when
the two implementations are independent.

**Rejected.** Sharing `MVMemory` for both. Costs ~100 lines to avoid; those 100
lines are what make the M1 and M2 gates mean anything.

---

## D9 — Conflict granularity is an experimental variable · 2026-09-11

**Decided by:** user, on proposal
**Status:** accepted

`Key::Basic` is account-granular by default, with a switch for finer
balance/nonce separation. The two settings are compared as an experiment
measuring false-conflict rate.

**Why.** Account granularity produces false conflicts (tx A writes only nonce,
tx B reads only balance). Cost to make it a variable is an enum and a flag, and
it directly answers the brief's "conflict detection rule based on account
access, storage slot access, or simplified read/write sets".

---

## D10 — Single crate, `parevm`, under `engine/` · 2026-09-11

**Decided by:** user, on proposal
**Status:** accepted

One library crate with modules, plus `src/bin/{demo,bench}.rs`.

**Rejected.** A cargo workspace. Ceremony without benefit at this size.

---

## D11 — revm pinned to `=41.0.0` · 2026-09-11

**Decided by:** Claude, with reasoning stated; user did not object
**Status:** accepted — **reversible cheaply only until M1**

**Why.** At time of pinning, 43.0.2 was two days old and 43.0.1 had been yanked;
42.0.0 had also been yanked. The 41 line has no yanks and three months of
settling, and is recent enough that documentation and examples match.

**Rejected.** Latest (43.0.2) — churn in a line whose previous patch was yanked
is a poor bet against a fixed deadline. Older (40.x) — three patch releases in
that line suggest 40.0.0 shipped with problems.

---

## D12 — EIP-7928 reframing adopted; revm BAL integration deferred · 2026-09-11

**Decided by:** Claude, delegated by the user on grading grounds
**Status:** accepted

revm 41 ships EIP-7928 Block Access List support (`revm_state::bal`, backed by
`alloy_eip7928`). Three things were possible. We take two of them:

1. **Adopted — reframe the static scheduler as an EIP-7928 prototype.** Zero
   code. Turns the static scheduler from a weak contrast case into a prototype
   of a pending Ethereum upgrade, and makes the Block-STM-versus-declared-access-
   lists comparison a live engineering question. See
   [DESIGN.md §7.1](docs/DESIGN.md).
2. **Adopted — derive access sets by profiling, using our own `ReadRecorder`.**
   Near-zero cost, since the machinery exists for M2 anyway. Removes the
   objection that we fabricated the access sets for transactions we authored.
   See [DESIGN.md §7.2](docs/DESIGN.md).
3. **Deferred — emitting canonical EIP-7928 `BlockAccessList` via revm's
   `bal_builder`.** Ranked *below* mainnet block replay on the stretch list, and
   likely not done.

**Why (3) is deferred.** The brief's feature requirements do not mention access
lists; grading follows those five requirements. Item 1 already captures the
narrative value and item 2 already closes the provenance objection, so item 3
buys only format conformance. It would land in M3 — the week the full sweep runs
and the figures are produced — and an unknown-cost integration against a new API
in that week risks the figures, which are the graded core. Mainnet replay
outranks it because an empirical finding about real blocks is worth more than a
serialisation detail.

**Rejected outright.** Using revm's `BalState` as our primary execution
database. It is built for a different purpose and would conflict with the
multi-version store. We borrow the concept, not the implementation.

---

## Open

Decisions not yet made. Move them above when settled.

- **O2 — Multi-version store concrete type.** `DashMap<(Address, U256),
  BTreeMap<TxIdx, WriteEntry>>` is the design sketch; the real choice depends on
  measured contention. Defer until M2a has numbers.
- **O3 — Benchmark machine.** The final sweep must run entirely on one machine.
  See the note on heterogeneous cores in
  [docs/EXPERIMENTS.md](docs/EXPERIMENTS.md).
- **O4 — Core A second seat.** `MVMemory` and the validation path should not be
  reviewed only by their author.
