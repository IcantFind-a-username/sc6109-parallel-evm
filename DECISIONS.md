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
**Status:** accepted, **revised the same day** — see below

Granularity is a switch comparing two conflict-detection rules, measured for
false-conflict rate:

- `Granularity::Slot` — one key per storage slot. Precise.
- `Granularity::Account` — every slot of an account maps to one key. Coarse.

**Why.** This is the exact axis the brief names: a conflict rule "based on
account access, storage slot access, or simplified read/write sets". Cost is an
enum and one key-mapping function.

**Revision.** As first written, D9 proposed the finer axis of separating balance
reads from nonce reads. **That is not implementable at this layer.** revm's
`basic_ref` returns the whole `AccountInfo`, so the database boundary cannot
observe which field the EVM consumed; distinguishing them would require an
`Inspector` watching opcodes, which is a different order of cost. The axis moved
to slot-versus-account, which is both implementable and the one the brief
actually names. The balance/nonce limitation is documented in `types.rs` and
belongs in the report as a stated limitation.

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

## D13 — Start M2 before the M1 gate passes · 2026-09-11

**Decided by:** user
**Status:** accepted

M2a work begins while M1's gate — agreement with Anvil on three workloads — is
still open, because that gate is blocked on Foundry not being installed.

**Why.** M2 is the hardest milestone and sits on the critical path. Everything
M2a needs from M1 already exists: the sequential baseline, the workload
generator and the differential harness. Waiting would idle the critical path on
an installation step.

**This deviates from the roadmap's own rule** that a milestone does not start
until the previous gate is green, so it is recorded rather than done quietly.
**Condition:** the Anvil cross-validation remains a hard gate. No benchmark
number is reported, in the report or the slides, until it passes — the
differential test proves the parallel engine matches our sequential engine; only
Anvil proves our sequential engine matches the EVM.

**Rejected.** Waiting for Foundry before starting M2.

---

## D14 — Coarse granularity means "a write is also a read" (settles O5) · 2026-09-11

**Decided by:** user
**Status:** accepted — decision only; implementation before M3, off M2b's
critical path

Under `Granularity::Account`, a transaction that writes an account is treated as
having also read **every slot of that account**. The multi-version store keeps
refusing account granularity until this is implemented.

**Why.** This is the semantics of a genuinely account-granular system: you
cannot write part of a unit without having read the unit. It restores the
soundness E7 found missing. Any change by a lower writer invalidates the next
writer above it, whose re-execution changes its own version in turn, so changes
propagate up the chain of writers — which makes checking only the latest writer
below a reader sufficient again. The difference in abort rate between this and
slot mode is exactly the false-conflict measurement D9 set out to make.

**Rejected.** (b) recording and re-checking the full writer set — obviously
sound, but O(writers) per validation and quadratic on a hot account, which is
precisely the workload the experiment most needs. (c) dropping the experiment —
gives up the brief's named axis ("account access, storage slot access") for no
saving that matters.

---

## D15 — mimalloc is the global allocator (settles O6) · 2026-09-11

**Decided by:** user
**Status:** accepted, implemented

Every binary and test in the engine uses mimalloc, installed in the library
crate behind a default-on cargo feature. `--no-default-features` restores the
system allocator.

**Why.** On macOS, the system allocator's multi-threaded contention depresses
every configuration at once and hides the engine's real scaling (E10: mimalloc
made single-threaded execution 38% faster and raised uncoordinated scaling from
1.10x to 1.47x). Installing it in the library, not per binary, guarantees the
sequential baseline and the parallel engines always run under the same
allocator, so speedup ratios stay fair — a binary that forgot the attribute
would otherwise silently measure a different condition. The feature flag exists
so the other condition can be reproduced when the report states which one was
used.

**The allocator is an experimental condition** and the report must say so.

**Rejected.** Keeping the system allocator. It would measure macOS malloc
contention rather than the schedulers.

---

## D16 — Work per transaction is an experimental variable (settles O7) · 2026-09-11

**Decided by:** user
**Status:** accepted

A compute workload calls the sha256 precompile with a tunable payload, so work
per transaction can be swept without Foundry.

**Why.** E10 showed scaling depends as much on how much work each transaction
does as on how often transactions conflict: a plain transfer costs about a
microsecond and fixed per-transaction overhead eats the gain, while 76 µs of
work per transaction reached 92% of linear on six performance cores. That
quantifies a third regime in which parallel execution does not help, alongside
conflict density and heterogeneous cores, at almost no cost.

**Constraint carried from E10.** Transaction gas limits must stay below the
EIP-7825 cap of 2²⁴ = 16,777,216, or revm refuses the transaction and the
refusal masquerades as a fast execution. The generator asserts it.

---

## D17 — Figure 2 plots measured dependency density (settles O8) · 2026-09-11

**Decided by:** user
**Status:** accepted

Figure 2's x-axis is **measured dependency density** — the fraction of
transactions that read a location an earlier transaction in the block wrote —
with critical path length as a secondary axis or companion figure. Both are
computed from each workload's actual read and write sets, not from generator
parameters. Workload defaults are corrected so that "uniform" sits in the
low-conflict regime: the account set is 100× the block size.

**Why.** E9 showed the Zipf exponent is not a monotone, comparable scale for
conflict intensity: at a fixed exponent, density moves from 2% to 75% as the
account-to-transaction ratio changes. Plotting against a generator knob would
make the figure's x-axis mean something different at every point. Measuring the
dependency structure directly makes any two workloads comparable on one axis,
including contract workloads that have no Zipf parameter at all.

**Rejected.** Plotting against the Zipf exponent with the account count held
fixed. Readable, but the axis would be specific to one generator and would not
transfer to NFT mint or AMM workloads.

---

## D18 — A write is a changed value, judged against what was served (settles O9) · 2026-09-11

**Decided by:** user
**Status:** accepted — fix scheduled after M2b step 1

`ReadRecorder` keeps a side log, beside the read set and not part of it, of the
`AccountInfo` it served for each account read. `MVMemory::apply` registers an
account write only when the post-execution value differs from the value that
execution was served. The read set's structure does not change. The dependency
analysis applies the same rule, so the store and Figure 2's x-axis agree on
what a write is.

**Why.** Registering every touched account as a write serialises any workload
that touches a shared account without changing it — the sha256 precompile in
the compute workload, and the contract account in every ERC-20, NFT and AMM
transaction (E12). Telling a write from a touch requires the value that was
read. Comparing against what *this execution* was served is exact and cannot
race: it does not depend on anything another worker does.

**Rejected.** (B) re-deriving the read value from the read-set origin at apply
time — it couples `apply`'s correctness to the store's state at the moment it
runs, and the store is precisely what M2b rebuilds. (C) revm's
`Account::is_changed()` — reliable only on revm's BAL path.

---

## D19 — Both engines follow EIP-161 (settles O10) · 2026-09-12

**Decided by:** Claude — from 2026-09-12 the user delegated the remaining
design decisions (see CLAUDE.md, working agreement)
**Status:** accepted, implemented

An account left empty after execution — zero nonce, zero balance, no code —
does not exist. One function, `state::existing`, applies the rule for
`SimpleState`, `BaseState`, `MVMemory` and the dependency analysis, and
snapshots omit empty accounts.

**Why.** Without it D18 would not fix E12's own case: a precompile call is
served `None` and leaves a touched empty account, which is only "no change" if
empty means absent (E14). It is also mainnet's rule.

*Correction, same day (E15):* this entry originally also claimed the M1 Anvil
cross-validation needed it. It does not — empty and absent accounts are
indistinguishable to every RPC and opcode, so the cross-validation passes either
way. That justification is withdrawn; the one above stands.

**Rejected.** Persisting empty accounts, as revm's `CacheDB` does — keeps the
precompile chain and fails the Anvil gate. Pre-seeding precompile accounts —
hides one symptom.

---

## D20 — Contracts live in base state; compiled bytecode is committed · 2026-09-12

**Decided by:** Claude (delegated)
**Status:** accepted, implemented

The three workload contracts (`contracts/src`) are placed in base state with
their runtime bytecode and pre-seeded storage, not deployed inside the block.
Their compiled runtime bytecode is committed under `engine/assets/`, extracted
by `scripts/extract_bytecode.py`; solc is pinned to 0.8.28.

**Why.** Account creation inside a block is out of scope for the store
(DESIGN §6), and a deployment transaction would be one every other transaction
depends on — a conflict the workload is not meant to have. Committing the
bytecode lets the engine build and test, and CI run, without Foundry.

**Rejected.** Deploying via a first transaction; compiling at build time
through a build script (would make solc a build dependency of the engine).

---

## D21 — Dependency waiting parks tasks; no spinning, no condition variables · 2026-09-12

**Decided by:** Claude (delegated)
**Status:** accepted, implemented

A transaction whose read hits an `ESTIMATE` is parked in the blocking
transaction's dependents list and holds no task. When the blocker next finishes
executing, its dependents are set ready one incarnation on and
`execution_idx` is pulled back to the lowest of them. This is the Block-STM
paper's mechanism.

**Why.** It needs no waiting primitive at all: a parked transaction is simply
not in the queue, and workers keep taking other tasks. Correctness rests on one
ordering, argued in the code and checked by mutation — the blocker's status is
set to `Executed` before its dependents are taken, and `add_dependency` checks
that status under the dependents lock, so a transaction cannot park after the
list was taken (M7: removing the check loses the wakeup and a transaction ends
parked forever).

**Rejected.** Spinning on the blocker's status — burns a core per waiting
transaction, and with more waiters than cores, stalls. A condition variable per
transaction — blocks a worker thread that could be doing other tasks. A
lock-free queue — no benefit at this scale for considerably harder reasoning.

---

## D22 — Block-STM's termination backstop: n² + 16n executions · 2026-09-12

**Decided by:** Claude (delegated)
**Status:** accepted, implemented

The scheduler panics if total executions exceed n² + 16n (at least 1024).

**Why.** There is no proven constant bound to assert, unlike the round
scheduler's n rounds (coordinator docs). Without a backstop, a store
inconsistency is a benchmark that never finishes (E8). Quadratic is far above
any correct run; it is a tripwire, and documented as one rather than as a bound.

**Rejected.** A wall-clock timeout — makes correctness depend on machine speed.

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
