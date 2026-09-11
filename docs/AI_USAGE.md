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
