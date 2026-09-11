# AI Usage Log

The course permits AI coding agents under conditions: **the student remains the
architect, the reviewer and the owner of the work**, and evidence of the process
— prompts used, bugs found and how they were resolved — must be retained.

> Check this against the current course policy document before submission. The
> requirements below reflect our understanding as of 2026-09-11; if the official
> wording differs, the official wording governs.

This file is a graded deliverable. Keep it current — reconstructing it the night
before submission produces something obviously reconstructed.

---

## Ground rules for this project

1. **Nobody merges code they cannot explain.** If you cannot walk a teammate
   through why a function is correct, it does not go in, regardless of who or
   what wrote it.
2. **Architecture decisions are ours.** Design choices belong in
   [DESIGN.md](DESIGN.md) with the reasoning written out. An AI may argue for an
   option; the decision and its justification are the team's.
3. **Log as you go.** One row per meaningful interaction, the same day.
4. **Bugs are evidence, not embarrassment.** A log showing AI-generated code
   that was wrong and how it was caught is stronger evidence of genuine
   engagement than a log where everything worked first time. Record the failures.
5. **Correctness-critical code gets human review regardless of origin.** The
   `MVMemory` read-resolution path and the validation logic especially.

---

## Log

Append rows. Do not edit history.

| Date | Person | Tool | What it was asked to do | Outcome | Review / fix |
| --- | --- | --- | --- | --- | --- |
| 2026-09-11 | — | Claude Code | Compare the six project options against the brief; recommend one | Recommended Option 5; team confirmed | Reviewed against the PDF requirements directly; tech stack decision (Rust + revm over Go) was the team's, against the tool's initial advice |
| 2026-09-11 | — | Claude Code | Draft repo scaffolding and planning docs (README, roadmap, design, experiments, risks) | Drafted | Pending team review — these are plans, not code, and need sign-off at kickoff |
| 2026-09-11 | — | Claude Code | Propose the core architecture (execution path, state-access layering, core types) | Proposed; user accepted with no changes | Architecture was reviewed against R1 before acceptance; the decorator stack exists specifically to make read-set capture unbypassable. Recorded as D7–D10 |
| 2026-09-11 | — | Claude Code | Survey revm versions and pin one | Pinned `=41.0.0` | Reasoning recorded in D11. Chose against latest (43.0.2, two days old, prior patch yanked) |
| 2026-09-11 | — | Claude Code | Decide whether to integrate revm's EIP-7928 BAL support | Split: reframing and derived access sets adopted, canonical BAL output deferred | Delegated by the user on grading grounds. Reasoning in D12 — the brief does not require access lists, and the integration would land in the week the figures are produced |
| 2026-09-11 | — | Claude Code | Write the M0 smoke test against revm 41 | Compiled and ran correctly first attempt | Extended it to test R2 empirically rather than trusting the documented claim — see bug evidence below |
| 2026-09-11 | — | Claude Code | Implement the state-access stack (`types`, `StateView`, `BaseState`, `ReadRecorder`, `SimpleState`) | Implemented on `feat/state-view`, 17 tests passing | D9 was found to be unimplementable as specified and was revised before coding, not worked around silently — see E3 |
| 2026-09-11 | — | Claude Code | Implement the sequential scheduler, workload generator and differential harness | Implemented, 27 tests passing | A generated-workload bug (E5) was caught by a semantic assertion, not by a crash; it would otherwise have contaminated every measurement |
| 2026-09-11 | — | Claude Code | Implement multi-version memory and the round-based scheduler (M2a) | Implemented on `feat/blockstm-rounds`; differential sweep passed first time | First-time success on concurrent code was treated as suspicious, not reassuring. Mutation testing found one untested code path and one missing termination guard — see E8 |
| 2026-09-11 | — | Claude Code | Run the full M2a gate and the watchlist speedup check | Gate passed: 20,000 block comparisons agree. Speedup check failed | Diagnosed rather than tuned: the failure traced to workload design (E9) and per-transaction cost (E10), not to the scheduler. Three decisions raised for the user instead of being taken |
| 2026-09-11 | user | — | Settle O5–O8 | Decided as D14–D17 | The user chose (a) for coarse granularity, adopted mimalloc, made work per transaction a variable, and moved Figure 2 to measured dependency density. Claude's recommendations were accepted on O5–O8; the user added the constraint that D14 stay off M2b's critical path |
| 2026-09-11 | — | Claude Code | Record D14–D17, update EXPERIMENTS, install mimalloc, fix workload defaults | Done | The mimalloc feature was silently not on by default at first — see E11 |

---

## Bug evidence

For each non-trivial defect in AI-generated code, record enough that a reader
can see what went wrong and how it was caught. Keep the full prompt in
`docs/prompts/` if it is long.

### Template

**Bug:** what was wrong
**Where:** file and function
**Origin:** which prompt or interaction produced it
**How it was caught:** test, review, differential failure, debugging session
**Root cause:** why the generated code was wrong
**Fix:** what changed, and who wrote the fix

---

### E1 — Structural mitigation for R1 was weaker than claimed · 2026-09-11

**Claim:** the proposed architecture asserted that making `ReadRecorder` the
only implementor of revm's `DatabaseRef` would turn any missed read path into a
compile error.

**Where:** `docs/DESIGN.md` §2.2, the read-set capture design.

**How it was caught:** reading revm 41's actual trait definition before writing
code against it, rather than working from the assumed shape.

**Root cause:** `DatabaseRef` has five methods, not the four assumed, and
`storage_by_account_id_ref` carries a **default implementation**. A defaulted
method is not a compile error to omit, so the structural guarantee does not hold
where revm supplies defaults. The default happens to delegate to
`self.storage_ref`, which would be logged — but that is a coincidence of the
current implementation, not a guarantee.

**Fix:** all five methods are implemented explicitly on `ReadRecorder`,
including the defaulted one, with a per-method test asserting the read log is
non-empty. Recorded in R1. This is also a second reason the revm version is
pinned: a version bump could silently add another defaulted read method.

---

### E2 — R2 confirmed stronger than documented · 2026-09-11

**Not a bug — a documented assumption that turned out to understate the
problem.** Recorded because the correction changes the design.

**Claim:** the design assumed the block beneficiary is written because
transactions *pay fees* to it.

**How it was caught:** `engine/src/bin/smoke.rs` was extended to run the same
transfer at `gas_price = 0` and at 1 gwei, and to print every account in the
write set, rather than taking the documented behaviour on trust.

**Finding:** the beneficiary appears in the write set **at both gas prices**,
including zero. The effect is unconditional, not fee-dependent.

**Consequence:** the beneficiary exemption (D4) is not an optimisation for
realistic fee conditions — it is required for any block to exhibit parallelism
at all, including in a zero-fee test harness. Had this been discovered later, a
zero-gas-price test workload would have looked like a scheduler bug.

---

### E3 — An accepted decision turned out to be unimplementable · 2026-09-11

**Claim:** D9 specified conflict granularity as an experimental variable, with
the fine setting separating balance reads from nonce reads.

**How it was caught:** writing the `StateView` trait and asking what a read of
an account actually returns, before implementing against the assumed shape.

**Root cause:** revm's `basic_ref` hands back the entire `AccountInfo`. The
database boundary never learns whether the EVM wanted the balance, the nonce or
the code hash, so the two cannot be separated there. Doing it properly needs an
`Inspector` observing opcodes — a different order of cost, and not justified by
what the axis would buy.

**Fix:** the axis was changed to slot-versus-account granularity, which is
implementable in one key-mapping function and is the comparison the brief
actually names. D9 carries the revision and its reasoning rather than being
quietly rewritten. The balance/nonce limitation is documented in `types.rs` and
goes in the report as a stated limitation.

---

### E4 — Test bug that confirmed the commit path · 2026-09-11

**Bug:** `transfers_compose_across_transactions` failed with
`NonceTooLow { tx: 0, state: 1 }`.

**Where:** `engine/tests/stack_integration.rs`, test helper `transfer_tx`.

**Root cause:** the helper always built a transaction with nonce 0, while the
sender's nonce advanced after each commit. The defect was in the test, not the
engine.

**Why it is worth recording:** the failure is positive evidence. It could only
occur if the nonce increment had persisted into `SimpleState` and been read back
by the EVM through `ReadRecorder` — that is, the commit-and-read-through path
works.

**Consequence for the workload generator:** transactions from the same sender
must carry increasing nonces. Getting this wrong fails an entire batch loudly
rather than producing a subtly wrong result, which is the better failure mode,
but the generator must handle it.

---

### E5 — Generated accounts collided with the precompile address range · 2026-09-11

**Bug:** 45 of 300 transfers in a generated workload halted with
`OutOfGas(Precompile)` instead of moving value.

**Where:** `engine/src/workload/transfer.rs`, `account_address`.

**Origin:** the generator addressed account `i` as `i + 1`, which puts the first
accounts at `0x01` through `0x0a` — ecrecover, sha256, ripemd160, identity, the
BN254 and BLS operations, KZG point evaluation.

**How it was caught:** a test asserting that funded transfers never revert.
The assertion failed at 45, and the halt reason named the cause directly.

**Root cause:** a transfer to a precompile does not move value; it *invokes* the
precompile. With a 21000-gas limit and no input, the call halts out of gas.

**Why this one matters more than it looks.** The block still executed. Every
affected transaction committed a nonce and a fee, so nothing crashed and the
throughput numbers looked entirely reasonable — roughly 15% of the workload was
quietly doing something other than what it claimed. Had the "must not revert"
assertion not been written, this would have contaminated every measurement in
the project, and the contamination would have been invisible in the figures.

**Fix:** generated addresses now carry a `0xA1` leading byte, placing them far
outside the precompile range, and `generated_addresses_avoid_precompiles`
asserts it for every account, sender and recipient in a workload.

**Generalisation worth carrying forward:** assert on *semantic* outcomes, not
just on absence of crashes. "It ran" and "it did what I meant" are different
claims, and only the second one is worth measuring.

---

### E6 — Execution discarded the read set on EVM refusal · 2026-09-11

**Bug (caught before it ran):** `exec::execute` returned `Err` without the read
set whenever revm refused a transaction.

**How it was caught:** tracing, before writing the parallel scheduler, what
happens to a sender's second transaction when it runs before the first has
written back.

**Root cause:** under speculative execution, refusal is frequently not a
property of the transaction. The second transaction reads the stale nonce and
revm refuses it with `NonceTooHigh`. Validation needs the read set to see that
the refusal rested on a stale read and schedule a retry. Without it, every
same-sender pair in a block would have been a fatal error — or, had the error
been swallowed, a silently dropped transaction.

**Fix:** `execute` now returns the read set unconditionally alongside a
`Result` outcome. A refusal that survives validation is still fatal, since
sequential execution would refuse it too.

---

### E7 — Account granularity is unsound for validation · 2026-09-11

**Bug (latent, caught before it could run):** under `Granularity::Account`,
`ReadRecorder` coarsens the *key* of a storage read to the account, but records
the *exact* slot's origin.

**How it was caught:** designing validation for coarse mode and asking what
re-resolving `Key::Basic(account)` for a storage read would actually compare.

**Root cause:** re-resolving the coarse key checks only the *latest* writer of
the account below the reader. A transaction can read slot `s` as written by
transaction `i₁`, while a later transaction `i₂` writes a different slot of the
same account. If `i₁` then re-executes and changes `s`, the latest writer is
still `i₂`, validation passes, and the reader has committed a stale value. That
is a correctness hole, not a tuning difference.

**Fix:** `MVMemory::new` refuses account granularity with an explanation, and
a test asserts the refusal. The correct semantics is an open architectural
decision, recorded as O5 in `DECISIONS.md` with three options. The D9 experiment
waits for it.

**Note:** this was introduced when D9 was implemented in the state-access work.
The sequential path never validates, so nothing exercised it; it would have
surfaced only as wrong numbers in the granularity experiment.

---

### E8 — Mutation testing the M2a differential sweep · 2026-09-11

**Context:** the round-based scheduler agreed with sequential execution on every
seed, thread count and conflict level at the first attempt. For concurrent code
that is a reason for suspicion. The differential harness had been shown to catch
a dropped transaction, but not a subtle bug inside the multi-version store.

**Method:** three deliberate bugs were introduced into `mv.rs` one at a time, and
the test suite was run against each.

| Mutation | Before | After |
| --- | --- | --- |
| Validation ignores account reads | Caught — 4 tests failed | Caught |
| Re-execution does not retract stale writes | **Not caught — all passed** | Caught by 3 unit tests |
| A reader resolves its own previous incarnation | **Test run hung forever** | Fails in 0.22 s with a located message |

**Finding 1 — retraction was untested.** In a transfer workload, a
re-execution writes the same locations as its previous incarnation, so the code
that retracts no-longer-written locations never ran. It was correct, but nothing
showed that. Direct unit tests on `MVMemory` now construct shrinking write sets.
End-to-end coverage needs a workload whose write set depends on what it reads —
an ERC-20 transfer that reverts on insufficient balance — and that is now on the
M2a checklist.

**Finding 2 — no termination guard.** The scheduler has a proven bound of `n`
rounds. Without asserting it, a store inconsistency manifests as a benchmark
that never finishes, which is the least informative failure possible. The bound
is now an assertion.

**Generalisation:** a green test suite shows the tests pass, not that they can
fail. For correctness-critical code, check that they can.

---

### E9 — "Uniform" was not a low-conflict workload · 2026-09-11

**Symptom:** the watchlist check — "is the uniform workload showing speedup
above 1?" — failed as soon as M2a ran. Speedup was 0.50x at two threads with a
32–45% abort rate, on a workload meant to have almost no conflicts.

**Diagnosis:** R2 (beneficiary) and R5 (global lock) were checked first, per the
watchlist, and ruled out. Measuring dependency density and critical path length
directly showed the actual cause:

| Workload (10,000 txs) | Depends on an earlier tx | Critical path |
| --- | --- | --- |
| uniform, 10k accounts | 75.4% | 13 |
| uniform, 100k accounts | 17.6% | 4 |
| uniform, 1M accounts | 2.2% | 3 |
| Zipf s=1.0, 10k accounts | 87.1% | 1060 |

With as many accounts as transactions, and each transfer touching two accounts,
every account is touched about twice. "Uniform" said nothing about conflict
density; the account-to-transaction ratio decides it. The design intent in
`EXPERIMENTS.md` was always a large account set — the generator's defaults did
not encode it.

**Confirmation that the scheduler itself was behaving:** rounds tracked the
critical path exactly as the correctness argument predicts — 9–13 rounds against
a critical path of 13 for uniform, 546–929 rounds against 1060 for Zipf.

**Consequence for the experiment design:** the Zipf exponent alone does not
determine conflict density, so it is the wrong x-axis for Figure 2. Proposed:
plot against *measured* dependency density or critical path, computed from the
workload itself. See the open decision in `DECISIONS.md`.

---

### E10 — Plain transfers are too cheap to parallelise · 2026-09-11

**Symptom:** on a genuinely low-conflict workload (1M accounts, 0.6% aborts,
three rounds, theoretical ceiling above 3000x), M2a reached only 1.40x at six
threads.

**Diagnosis, in three steps:**

1. *Isolate the scheduler.* Executing every transaction independently against
   the base state, with no multi-version memory and no coordination at all,
   scaled to only **1.10x** at six threads. The bottleneck was below our code.
2. *Test the allocator.* Swapping macOS's system allocator for mimalloc made
   single-threaded execution 38% faster and raised uncoordinated scaling to
   1.47x. A real factor, but not the whole story.
3. *Vary work per transaction.* Calling the sha256 precompile with a growing
   payload — no compiler needed — turned the scaling curve around:

| Work per tx | Single-thread cost | 6 threads |
| --- | --- | --- |
| plain transfer | 1.17 µs | 1.40x |
| sha256, 1 KB | 3.67 µs | 4.44x |
| sha256, 8 KB | 20 µs | 5.32x |
| sha256, 32 KB | 76 µs | 5.53x (92% of linear on 6 P-cores) |

**Finding:** the parallel substrate scales well. A plain transfer costs about a
microsecond, and fixed per-transaction overhead — building an EVM, allocating,
scheduling — consumes the gain. This is a **third regime in which parallel
execution does not help**, alongside conflict density and heterogeneous cores,
and it is quantified.

**A diagnostic error along the way, recorded because it is instructive:** the
first payload run reported sha256 transactions as *faster* than plain transfers,
and independent of payload size. That is physically impossible, and checking
why showed every one was being refused: a 30M gas limit exceeds the per-
transaction cap of 2²⁴ (EIP-7825). Refusals are cheap, so they looked like fast
executions. Same lesson as E5 — confirm the work being measured is the work
intended — and a new workload constraint: transaction gas limits must stay
below 16,777,216.

All numbers in E9 and E10 are internal diagnostics taken on the development
machine. Per D13 none are reportable until the Anvil gate passes, and per
`EXPERIMENTS.md` §6.1 none above six threads mean anything on this machine.

---

### E11 — The allocator feature was not on by default · 2026-09-11

**Bug:** after installing mimalloc behind a default-on cargo feature, the
default build still used the system allocator.

**Where:** `engine/Cargo.toml`.

**Root cause:** `cargo add --optional` created a `[features]` section on its
own. The edit that was meant to add `default = ["mimalloc"]` checked whether a
`[features]` section already existed, found one, and skipped the insertion. The
build compiled, every test passed, and nothing looked wrong.

**How it was caught:** reading back the manifest after the change rather than
trusting the edit. It was then *confirmed* by the `parevm::ALLOCATOR` constant,
which reports the allocator a build actually runs under: `mimalloc` by default,
`system` with `--no-default-features`.

**Why it would have mattered:** the M2a baseline was the next step. It would
have been recorded under the system allocator while its metadata claimed
mimalloc — an experimental condition misreported in the very table that M2b is
judged against.

**Generalisation:** an experimental condition that cannot be observed at run
time cannot be trusted. Every result file records `ALLOCATOR` from the binary
that produced it, not from what the configuration was meant to be.

---

## Notes for the report

The report should state plainly how AI tooling was used and where the team's own
engineering lies. Candidate framing, to be confirmed once the work is done:

- Design decisions — conflict-detection granularity, the beneficiary exemption,
  the M2a/M2b split — were made by the team and are argued in
  [DESIGN.md](DESIGN.md).
- The correctness apparatus (differential testing, Anvil cross-validation) was
  specified by the team precisely because AI-assisted concurrent code cannot be
  trusted on inspection.
- Where AI-generated code was wrong, the log says so.

That last point is worth more than a claim of flawless output.
