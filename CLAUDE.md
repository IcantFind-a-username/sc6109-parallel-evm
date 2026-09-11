# CLAUDE.md

Project instructions for Claude Code working in this repository.

## What this is

A parallel transaction execution engine for EVM workloads — SC6109 course
project, Option 5. Three schedulers (sequential, static, Block-STM) over a real
EVM, benchmarked across a conflict-density axis. Read
[docs/DESIGN.md](docs/DESIGN.md) before touching `engine/`.

## Working agreement

**The user is the architect. Claude implements.** This is not a stylistic
preference — the course grades whether the student directed the work, and the
git history is the evidence.

Consequences:

- **Do not make architecture decisions unilaterally.** If a task requires one —
  a data structure that constrains the design, a trait boundary, a concurrency
  strategy, a dependency — stop, state the options and the trade-off, and ask.
- **Record every decision that was made** in [DECISIONS.md](DECISIONS.md), with
  the reasoning and the alternative that was rejected. One entry, appended, not
  edited.
- **Do not generate large amounts of code at once.** Work milestone by
  milestone, in reviewable pieces. The user must be able to explain every line
  at the defence.
- **Log AI-assisted work** in [docs/AI_USAGE.md](docs/AI_USAGE.md) as it
  happens, including suggestions that were rejected and bugs that were found in
  generated code. That log is a graded deliverable and cannot be reconstructed
  afterwards.

## Hard constraints

These are settled. Do not reopen them without the user explicitly saying so.

1. **Stack is Rust + revm.** The Go / go-ethereum alternative is withdrawn.
2. **revm is pinned to an exact version.** `Cargo.lock` is committed. Never run
   `cargo update` or bump the revm version.
3. **Implement `DatabaseRef`, never `Database`.** `&mut self` does not compose
   with a shared multi-version store. See [docs/DESIGN.md §2.1](docs/DESIGN.md).
4. **The block beneficiary is exempt from conflict detection.** Fees accumulate
   separately and are applied at commit. See [docs/DESIGN.md §5](docs/DESIGN.md).
5. **The differential test and the Anvil cross-validation are milestone gates.**
   They are never cut, deferred, or weakened to make a milestone look green.
6. **Read-set capture is the top correctness risk.** Every `DatabaseRef` method
   that observes state must log its read. Adding a method without logging it is
   a silent correctness bug — see [docs/RISKS.md R1](docs/RISKS.md).

## Milestones

Work to [docs/ROADMAP.md](docs/ROADMAP.md). Each milestone has a gate; do not
start the next one until the current gate passes. Current milestone is stated in
the README's Status section — keep it accurate.

## Conventions

- `cargo fmt` and `cargo clippy` clean before every commit
- No `unwrap()` outside tests and `main`; propagate errors
- Every `unsafe` block carries a comment justifying it (there should be none)
- Benchmarks never run in `cargo test`; they live behind a separate binary
- Use `tx_gas_used()`, not the deprecated `gas_used()` — revm 41 split
  execution gas from state gas (EIP-8037), and `gas_used` is now ambiguous
- Commit messages explain *why*, not *what* — the diff already says what

## Commands

Filled in as they come to exist.

All cargo commands run from `engine/`.

```
cargo test                    # unit + differential tests
cargo run --bin smoke         # M0: revm binding + R2 demonstration
```

## Do not

- Commit the course PDF or any course material (`/*.pdf` is gitignored)
- Commit benchmark CSV without its seed and machine spec alongside
- Read RISE's `pevm` source while implementing — see
  [docs/RISKS.md R9](docs/RISKS.md)
