# Design

How the engine is put together, and why each piece is shaped the way it is.

---

## 1. Overview

```
 workload generator ──> Vec<Transaction>
                              │
                              ▼
                     ┌────────────────┐
                     │   Scheduler    │  sequential │ static │ block-stm
                     └────────┬───────┘
                              │  dispatches tx to worker threads
                              ▼
                     ┌────────────────┐
                     │  revm instance │  (one per worker)
                     └────────┬───────┘
                              │  reads via DatabaseRef, returns ResultAndState
                              ▼
                     ┌────────────────┐
                     │    MVMemory    │  multi-version store, Arc-shared
                     └────────┬───────┘
                              │  after commit
                              ▼
                        final state ──> differential check vs sequential
                                    ──> cross-check vs Anvil
```

Three schedulers, one execution substrate, one state store. The schedulers are
the experiment; everything else is infrastructure that must simply be correct.

---

## 2. The revm binding

### 2.1 Use `DatabaseRef`, not `Database`

revm's `Database` trait takes `&mut self`. That fights directly with sharing one
state store across worker threads, and the borrow checker will not let you win
that fight cleanly.

Implement **`DatabaseRef`** instead — its methods take `&self` — and wrap it with
revm's `WrapDatabaseRef` adapter to hand the EVM something that satisfies
`Database`. The multi-version store lives behind an `Arc` and is only ever read
through this path. Writes go through the commit path in §4, never through the
EVM's own database handle.

Get this right in M0. Everything downstream depends on the shape of it.

### 2.2 revm does not give you a read set

The write set is free: `ResultAndState.state` hands back, per account, the
touched storage slots with both `original_value` and `present_value`, plus
account-level status flags.

The **read set is not recorded by revm at all**. You must capture it yourself by
instrumenting the `DatabaseRef` implementation: every `basic()`, `storage()`,
`code_by_hash()` and `block_hash()` call appends to a per-execution log
(interior mutability — `RefCell` in a single-threaded execution context, or a
thread-local buffer).

This log is the sole input to Block-STM validation. **A missed read category
produces a silent correctness bug** that appears only under concurrency, only
sometimes. This is the single highest-risk surface in the project, which is why
the differential test is a milestone gate and not a nice-to-have.

Read categories to capture:

| Category | Source call | Versioned at |
| --- | --- | --- |
| Account balance / nonce / code hash | `basic()` | account granularity |
| Storage slot | `storage()` | `(address, slot)` |
| Contract code | `code_by_hash()` | immutable after deploy — safe to cache globally |
| Block hash | `block_hash()` | immutable within a block — not versioned |

### 2.3 Pin the version

revm's API has changed shape across major versions, particularly around EVM
construction. Pin an exact version in `Cargo.toml` and do not upgrade during the
project. An upgrade mid-flight costs half a day to a day of compile errors and
buys nothing the project needs.

---

## 3. Multi-version memory (`MVMemory`)

### 3.1 Shape

Conceptually, for every state location, a map from transaction index to the
value that transaction wrote:

```
MVMemory:
    slots:    DashMap<(Address, U256), BTreeMap<TxIdx, WriteEntry>>
    accounts: DashMap<Address,         BTreeMap<TxIdx, AccountEntry>>

WriteEntry:
    Value(Incarnation, U256)
    Estimate           // writer was aborted; its writes are provisional
```

`BTreeMap` rather than a hash map because the central read operation is
"the highest index strictly below mine", which is an ordered-range query.

### 3.2 Read resolution

Transaction `j` reading location `L`:

1. Find the greatest `i < j` with an entry for `L`.
2. No such `i` → read from the base state snapshot. Record the read as
   `(L, None)`.
3. Entry is `Value(inc, v)` → return `v`. Record the read as `(L, Some(i, inc))`.
4. Entry is `Estimate` → transaction `i` was aborted and will rewrite this
   location. Record a dependency on `i`. In M2a, treat this as an immediate
   abort of `j`. In M2b, suspend `j` and re-schedule it when `i` completes.

Recording the *version* `(i, inc)` rather than the value is what makes
validation cheap and exact.

### 3.3 Validation

Transaction `j` validates by re-resolving every location in its read set. If any
location now resolves to a different `(index, incarnation)` than it did during
execution, `j` is aborted. Identical version ⇒ identical value, so no value
comparison is needed.

### 3.4 Abort and re-execution

On abort of `j`:

1. Mark every entry `j` wrote as `Estimate`. This is what lets later readers
   detect that a rewrite is coming instead of reading a stale value.
2. Increment `j`'s incarnation number.
3. Re-queue `j` for execution.
4. In M2b, lower `validation_idx` to `j+1` — every transaction after `j` may
   have read something `j` is about to change.

**Re-execution must be complete.** Do not reuse the previous incarnation's gas
accounting or partial results: an `SSTORE` from zero to non-zero costs
differently than non-zero to non-zero, so a transaction that reads a different
value legitimately consumes different gas.

---

## 4. Commit and determinism

The block is complete when every transaction has executed and validated without
a subsequent abort. Final state is then materialised by taking, for each
location, the entry with the **highest transaction index** — which is by
construction exactly what sequential execution would have produced.

This is the determinism argument, and it is the question the defence will
certainly ask. The claim is not "we tested it and it matched"; the claim is:

> A transaction's read set records the exact version it read. Validation fails
> unless every read still resolves to that same version. A transaction that
> survives validation therefore observed precisely the state that sequential
> execution at its index would have shown it. Since every transaction must pass
> validation before the block commits, the committed state equals the sequential
> state.

The differential test exists to catch implementation bugs in that argument, not
to substitute for it.

---

## 5. The beneficiary problem

**Every transaction pays gas to the block beneficiary.** That is a balance write
to one shared account on every single transaction, which means a naive conflict
detector marks all transactions as mutually conflicting, and measured speedup is
pinned at 1.0 regardless of workload.

This will look like a scheduler bug. It is not. Budget zero days for debugging
it by knowing about it now.

Two viable treatments:

1. **Disable the reward in revm's config.** There is a configuration flag for
   this, gated behind a cargo feature; the exact name has moved between
   versions, so check it against the version you pinned.
2. **Exempt the beneficiary account from conflict detection** and accumulate
   fees separately, adding them in a single pass at commit.

Prefer (2). It keeps the EVM semantics intact, and it is defensible: the
beneficiary balance is a commutative accumulator, not a read-modify-write
dependency, so treating it as non-conflicting is *correct*, not a shortcut.
Block-STM's authors make the same observation. Say so in the report — it reads
as insight rather than as a workaround.

Other structurally similar hot spots worth noting in the report: ERC-20
`totalSupply` under mint, and any single-pool AMM reserve. Unlike the
beneficiary, those are genuine read-modify-write dependencies and cannot be
exempted. That asymmetry is a good paragraph.

---

## 6. Account-granularity operations

Not everything is slot-granular. These are account-level writes and must be
versioned at account granularity:

- Account creation (`CREATE`, `CREATE2`)
- `SELFDESTRUCT`
- Balance and nonce changes

revm's `Account.status` carries `Touched`, `Created` and `LoadedAsNotExisting`
flags. Commit logic must respect them — merging storage maps alone will produce
wrong results for created and destroyed accounts.

---

## 7. The static scheduler

The contrast case, and the cheaper half of the experiment.

Transactions declare a read/write set up front, in the spirit of Solana's
Sealevel and of EIP-2930 access lists. The scheduler groups transactions with
disjoint declared sets and runs the groups on separate threads. No speculation,
no aborts, no multi-version memory.

### 7.1 This is not a strawman — it is EIP-7928

The obvious objection is that access sets cannot be known ahead of execution on
the EVM: dynamic jumps, `delegatecall`, and storage addresses computed from
calldata all defeat static analysis. That objection is real for *static
analysis*, but it is not the only way to obtain an access set.

**EIP-7928 (Block Access Lists)** proposes exactly this: blocks carry the
read/write sets of their transactions, derived by the block builder during
construction, precisely so that validators can execute in parallel. revm 41
ships support for it — `revm_state::bal`, backed by `alloy_eip7928`.

This reframes the whole comparison. The static scheduler is not a weak
contrast case; it is **a prototype of a pending Ethereum upgrade**. The question
the project answers becomes a live engineering one rather than an academic one:

> Optimistic speculation, which works on Ethereum today, versus declared access
> lists, which work if EIP-7928 ships — which suits EVM workloads better, and
> under what conflict conditions?

### 7.2 Where the access sets come from

Access sets are **derived, not fabricated**. A profiling pass executes the
block once, sequentially, using the same `ReadRecorder` instrumentation the
Block-STM path uses, and each transaction's read and write sets become the
declaration fed to the static scheduler (D23). Sequentially, because that is how
a builder derives an access list — and because executing each transaction
against pre-block state instead would refuse every same-sender follow-up for
its nonce and lose its access set.

This matters for defensibility. A workload generator that emits access sets for
transactions it authored proves nothing — the natural objection is that we
handed ourselves the answer. Deriving them by execution is the same procedure
EIP-7928 specifies for block builders, so the static scheduler receives input of
the same provenance a real EIP-7928 builder would produce.

The honest limitation to state in the report: derivation costs a full execution
pass, so the static scheduler's measured speedup **excludes the cost of
obtaining its own input**. Under EIP-7928 that cost is paid once by the builder
and amortised across every validator, which is the entire argument for the EIP —
but our numbers measure the validator side only, and the report must say so.

Emitting a canonical EIP-7928 `BlockAccessList` via revm's `bal_builder` is a
stretch goal, not a requirement; see [ROADMAP.md](ROADMAP.md).

---

## 8. Layout

```
engine/
  src/
    db/          DatabaseRef impl, read-set instrumentation, base snapshot
    mv/          MVMemory, versions, read/write sets
    sched/       sequential, static, blockstm
    exec/        revm driver, worker pool
    bin/         demo, bench runner
contracts/       Solidity workloads (Foundry)
bench/           batch generation, sweep driver
analysis/        plotting
results/         CSV output (gitignored)
docs/            this
```
