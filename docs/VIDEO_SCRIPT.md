# Video script — 10 minutes

Narration is the speaker notes in `docs/SC6109_parallel_evm.pptx`, reproduced
here with a time budget. At about 150 words a minute the narration runs 8 min 50 s;
with the one-minute demo after slide 6 the video is just under ten minutes.

**Recording.** Present from PowerPoint or Keynote with the notes visible to the
speaker only. Screen-record the slides and a terminal. If the group splits the
narration, natural hand-over points are after slides 5, 8 and 11.

| Part | Slides | Target |
| --- | --- | --- |
| The question and the system | 1–5 | 3:20 |
| Correctness + live demo | 6 + demo | 1:45 |
| Results | 7–11 | 3:10 |
| The answer, context, close | 12–14 | 1:45 |

---

## Slide 1 — When does parallel EVM execution pay?
*~42 s · ends at 0:42*

Blockchains execute transactions one at a time, even though most transactions in a block never touch the same state. Our project asks the question from the brief directly: when does executing them in parallel actually help, and when does it not? We built a parallel execution engine on real EVM bytecode, with three different schedulers, and measured it on twenty workloads. The two numbers on the right are the answer in miniature: five and a half times faster when transactions are independent and do real work, and less than half the speed of plain sequential execution when every transaction depends on the one before it.

## Slide 2 — The dependency bound says 1,000×. We measured 1.4×.
*~30 s · ends at 1:12*

Here is the puzzle that frames everything. A block of two thousand transfers between random accounts has almost no dependencies — the dependency bound says it could run a thousand times faster. We measured one point four. ERC-20 transfers do better, compute-heavy calls much better, and an NFT mint is actually slower than running it sequentially. Explaining that gap — where the missing speedup goes — is what the rest of the talk is about.

## Slide 3 — One execution path, three schedulers, every read recorded
*~40 s · ends at 1:52*

Everything runs through one execution path. The EVM is revm, embedded as a library, running bytecode compiled from Solidity. Between the EVM and state sits a read recorder — the only type in the code base that the EVM can read through — so every read is logged. That matters because optimistic parallel execution is only correct if every read is re-checked; a read that slips through is a bug that shows up on one run in a few hundred. Below that sit two views of state: multi-version memory for the parallel schedulers, and plain state for the sequential baseline.

## Slide 4 — Speculate in rounds, collaborate, or declare up front
*~45 s · ends at 2:36*

We built three schedulers, because the interesting question is not just whether parallelism helps but which kind. The first runs everything optimistically in rounds: execute all, validate all, re-run what failed. It is simple and provably terminates, but on a chain it re-runs everything above each fix. The second is Block-STM, the scheduler behind Aptos: workers share execution and validation tasks, and a transaction that would read a value about to change is parked until it is ready. The third is static: it derives every transaction's access set the way a block builder would under Ethereum's proposed block access lists, EIP-7928, and runs conflict-free groups in parallel with no speculation at all.

## Slide 5 — A conflict is a changed value — not a touched account
*~41 s · ends at 3:17*

Conflict detection is where correctness and performance meet. A transaction's read set is everything the EVM loaded; its write set is only what it actually changed. That distinction cost us a bug: at first we counted every account the EVM marked as touched, which made every call to the same contract look like a conflict. The block's fee recipient gets special treatment — every transaction pays it, but fees add up in any order, so they are summed at the end. And granularity is a design choice: per slot or per account. On ERC-20 that choice turns out to decide almost everything.

## Slide 6 — Every parallel result is bit-identical to sequential — and checked
*~42 s · ends at 3:59*

Before trusting any speedup, we made sure the answers are right. Every parallel scheduler is compared slot for slot with an independent sequential engine on a thousand random blocks per workload. The sequential engine is itself checked against Anvil, a real Ethereum node, on nine workloads. And we tested the tests: we injected fifteen bugs on purpose; all are caught, though two initially slipped past, which is how we found holes in the tests. Along the way the tests caught three real bugs — including a race that appeared once in a full run and that a stress test then reproduced in half a second.

## Demo — one minute
*~60 s · ends at 4:59*

Cut to a terminal in the repository. Show, don't narrate every line:

```bash
cd engine
cargo test --release --test blockstm_differential   # parallel vs sequential, slot for slot
```

> "Every one of these runs a block through Block-STM and through the sequential
> engine and compares every account and storage slot."

```bash
python3 ../scripts/anvil_crosscheck.py ../results/scratch/crosscheck.json
```

> "And here our sequential engine against a real Ethereum node — nine workloads,
> all agreeing." (Run `cargo run --release --bin bench -- export` once beforehand
> to create the input file, and have Foundry's `anvil` on the PATH.)

```bash
cargo run --release --bin demo
```

> "Finally one block through the engine, so you can see the scale: two thousand
> transactions in a few milliseconds."

## Slide 7 — Three regimes: scales, scales a little, gets slower
*~29 s · ends at 5:28*

Here is speedup against thread count for six representative workloads. Three shapes appear. Independent, compute-heavy calls scale almost linearly — over five times on six cores. Independent but light transactions, like transfers, gain something up to about six threads, then get slower once work lands on the chip's efficiency cores. And dependent workloads — an NFT mint, swaps on one pool — are slower than sequential under every scheduler, at every thread count.

## Slide 8 — Chain length kills speedup — not how many transactions conflict
*~41 s · ends at 6:10*

We placed every workload by its measured dependency density — the fraction of transactions that read something an earlier transaction wrote. Surprisingly, density is not what kills speedup. Blocks where three quarters of transactions depend on another still run at 1.3 times, because the chains are short. What matters is the length of the longest chain: at 432 Block-STM is still ahead; at 1,200 and at 2,000 it falls to under half of sequential speed. And look at the two rows with a chain of 432 — identical dependency structure, but ERC-20 transactions do more work, so they get 1.67 instead of 1.07.

## Slide 9 — A transaction has to do more work than it costs to coordinate
*~43 s · ends at 6:52*

To isolate the second factor we held conflicts at zero and varied how much work each transaction does, by hashing a growing payload. With almost no work — about a microsecond and a half — the speedup is only one and a half. With seventy-eight microseconds of work it is five and a half, ninety-two percent of the six cores. The reason is a roughly fixed cost per transaction for versioning and validation: at one thread Block-STM loses up to forty percent to its own machinery on light transactions. A plain payment is on the wrong side of this curve no matter how independent the block is.

## Slide 10 — The scheduler matters as much as the workload
*~38 s · ends at 7:30*

Same blocks, different schedulers. On a dependency chain, round-based execution confirms one more transaction per round and re-runs everything above it — two hundred and forty-seven executions per transaction on the NFT mint, and a speedup of one hundredth. Block-STM runs each transaction fewer than twice, because a transaction that would read a value about to change is parked until it is ready instead of being run anyway. That single mechanism is what makes optimistic execution survivable under contention. It is also a correctness mechanism: we removed it on purpose and the results went wrong.

## Slide 11 — Per-account conflict detection throws ERC-20 parallelism away
*~28 s · ends at 7:58*

Granularity turned out to be decisive. Every ERC-20 balance is a storage slot inside the token contract. If conflicts are tracked per slot, transfers between different holders are independent and all three schedulers get about three times. If they are tracked per account, every transfer collides in the token contract's account and the speedup disappears. That is exactly why Ethereum's proposed block access lists record storage keys, not just accounts.

## Slide 12 — Parallel execution pays only when all three hold
*~41 s · ends at 8:39*

So, when does parallel execution help? Only when three things hold at once. Chains must be short — a single hot contract is a ceiling no execution engine can remove. Transactions must do real work, enough to pay for coordinating them. And conflicts must be tracked finely, on cores that are really there. Lose the first and parallel execution is actively slower than sequential. In practice that means big gains for busy chains with diverse contract activity, and little or nothing for a block dominated by one popular mint or one busy pool — which is often exactly what congestion looks like.

## Slide 13 — How this maps onto production systems
*~39 s · ends at 9:18*

Finally, how this connects to real systems. Our Block-STM follows the scheduler Aptos uses, on EVM bytecode, and we see the same shape. Solana's model — declaring the accounts a transaction touches — corresponds on the EVM to account-level detection, and our ERC-20 result shows how much that throws away. Ethereum's EIP-7928 proposes block access lists with storage keys; our static scheduler is what a validator could do with them, while the builder pays the cost of producing the list. And account-abstraction bundles concentrate many users into one transaction and one contract, which is the unfavourable case.

## Slide 14 — What we would do next
*~27 s · ends at 9:45*

Three things we would do next. Replay real mainnet blocks, to see where real traffic sits on the axes we measured. Run on a machine with uniform cores, so results above six threads measure the scheduler rather than the chip. And emit real EIP-7928 access lists, measuring the builder's side too. Everything — code, raw data, figures and the full report — is in the repository. Thank you.

---

Total narration: 1314 words.
