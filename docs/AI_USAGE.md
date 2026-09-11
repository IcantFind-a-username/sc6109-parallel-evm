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
