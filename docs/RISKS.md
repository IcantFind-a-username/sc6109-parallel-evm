# Risks

Ranked by expected cost. Each entry names the symptom, because the expensive
failures here are the ones that look like something else.

---

## R1 — Incomplete read-set capture · *high impact, high likelihood*

**What happens.** revm records no read set; we instrument the `DatabaseRef` to
capture one ourselves. Miss a category — account balance, code hash, a cold
account load — and Block-STM validation passes transactions that actually read
stale state.

**Symptom.** Parallel and sequential results diverge on maybe 1 run in 200, on
one slot, at high thread counts only. It will look like a race in `MVMemory` or
a `DashMap` misuse, and the team will spend days there.

**Mitigation.** The differential test is an M1 deliverable, before Block-STM
exists, and runs 1000 seeds in CI. Enumerate read categories explicitly against
[DESIGN.md §2.2](DESIGN.md) and assert the log is non-empty for each. When a
divergence appears, suspect the read set first, not the concurrency.

---

## R2 — Beneficiary account serialises everything · *high impact, certain*

**What happens.** Gas fees are paid to the block beneficiary on every
transaction, so every transaction writes one shared account.

**Symptom.** Speedup is exactly 1.0 on every workload, including
`erc20-random`, which should be near-linear. Abort rate near 100%.

**Mitigation.** Known in advance; treatment in [DESIGN.md §5](DESIGN.md).
Exempt the beneficiary from conflict detection and accumulate fees separately.
Verify `erc20-random` shows speedup > 1 as soon as M2a runs — if it does not,
this is the first thing to check.

---

## R3 — M2 overruns · *high impact, medium likelihood*

**What happens.** Block-STM is the hardest component and sits on the critical
path with only ~4 days of slack.

**Symptom.** Oct 8 arrives, the scheduler works but the differential test still
fails intermittently.

**Mitigation.** M2 is pre-split: M2a (round-based) is a complete, correct,
shippable deliverable on its own; M2b (collaborative scheduler) is the upgrade.
**Decision point Oct 8** — if M2a's gate is not green by then, drop M2b and
spend the remaining time on experiments and the report. A correct round-based
scheduler with clean data scores better than a half-finished Block-STM.

---

## R4 — revm API churn · *medium impact, low likelihood if handled*

**What happens.** revm's construction API has changed shape across major
versions; tutorials and Stack Overflow answers target versions that no longer
compile.

**Symptom.** Half a day of type errors after a routine `cargo update`.

**Mitigation.** Pin an exact version in M0. `Cargo.lock` is committed. Nobody
upgrades during the project. When consulting examples, check which version they
target before trusting them.

---

## R5 — Rust concurrency ergonomics · *medium impact, medium likelihood*

**What happens.** `Database` takes `&mut self`, which does not compose with a
shared store. Teams discover this after building around `Database`.

**Symptom.** Escalating `Arc<Mutex<..>>` wrapping, lifetimes that will not
resolve, and eventually a store that is serialised by a global lock — which also
pins speedup at 1.0 and masquerades as R2.

**Mitigation.** Implement `DatabaseRef` from the start ([DESIGN.md §2.1](DESIGN.md)).
Settle this in M0 while it costs an afternoon. If speedup is stuck at 1.0, check
for an accidental global lock before blaming the scheduler.

---

## R6 — Benchmark noise swamps the effect · *medium impact, medium likelihood*

**Symptom.** Run-to-run variance exceeds the differences between schedulers;
figures are not reproducible; the conflict-rate curve is visibly jagged.

**Mitigation.** Measurement hygiene in [EXPERIMENTS.md §6](EXPERIMENTS.md) —
median of ≥5 runs, warmup discarded, quiet machine, committed seeds. Settle on
one benchmark machine early and run the final sweep entirely on it. Do not mix
results from different laptops into one figure.

---

## R7 — Account-granularity operations mishandled · *medium impact, low likelihood*

**What happens.** Account creation and `SELFDESTRUCT` are account-level, not
slot-level. Merging only storage maps loses them.

**Symptom.** Divergence confined to workloads that deploy contracts; balances
correct but an account exists in one run and not the other.

**Mitigation.** [DESIGN.md §6](DESIGN.md). Respect revm's `Account.status`
flags in commit. Include a contract-deploying case in the differential test.

---

## R8 — Rust capacity on the team · *closed 2026-09-11*

**Resolved.** At least one team member has working Rust and concurrency
experience and owns a Core A seat. The Go / go-ethereum fallback considered
during planning is **withdrawn**: the stack is Rust + revm, and revisiting it
mid-project would cost a week for no benefit.

**Residual.** Core A is a two-person role. The second seat is still open. If it
cannot be filled by someone who can review `MVMemory` and the validation path
independently, the exposure moves to R1 — correctness-critical concurrent code
with a single reviewer. Pair on those two modules rather than splitting them.

> Note that Rust proficiency does not reduce R1. A missed read category is a
> specification error, not a language error; the borrow checker cannot see it.
> R1 remains the top risk in this project.

---

## R9 — Perceived overlap with existing implementations · *low impact, easily avoided*

**What happens.** RISE's `pevm` is a well-known Rust Block-STM implementation
built on revm. The resemblance is structural and unavoidable — both follow the
same paper.

**Mitigation.** Already declared in the README. State it again in the report:
independent implementation from the Block-STM paper. Cite the paper. Do not
read `pevm`'s source while implementing; if anyone does consult it, record that
in [AI_USAGE.md](AI_USAGE.md) and cite it as a reference.

---

## Watchlist

Check these at each weekly sync:

- [ ] Is `erc20-random` showing speedup > 1? *(R2, R5)*
- [ ] Has the differential test run since the last merge? *(R1)*
- [ ] Is `Cargo.lock` unchanged? *(R4)*
- [ ] Are we on track for the Oct 8 M2a decision point? *(R3)*
- [ ] Is every committed CSV accompanied by its seed and machine spec? *(R6)*
- [ ] Has anyone other than the author reviewed changes to `MVMemory` or the
      validation path? *(R8 residual)*
