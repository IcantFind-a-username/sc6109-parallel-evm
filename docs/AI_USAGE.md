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
