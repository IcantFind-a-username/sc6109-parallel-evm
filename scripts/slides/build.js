// Builds docs/SC6109_parallel_evm.pptx. Numbers come from results/final/*.csv
// (see docs/REPORT.md); figures from docs/figures/.
//
//   cd scripts/slides && npm install pptxgenjs@3 && node build.js
//   QA=1 node build.js   # also writes qa.json for scripts/slides/qa.py
const pptxgen = require("pptxgenjs");
const path = require("path");

const ROOT = path.resolve(__dirname, "..", "..");
const FIG = (f) => path.join(ROOT, "docs", "figures", f);

// Palette: the figures' series colours, with Block-STM's orange as the accent.
const C = {
  ink: "161B26", // dark slides
  inkSoft: "232A38",
  paper: "FFFFFF",
  text: "14181F",
  muted: "5A6272",
  mutedDark: "AEB6C8",
  line: "E3E5EA",
  tint: "F3F5F8",
  orange: "EB6834", // M2b Block-STM — the accent
  blue: "2A78D6", // M2a
  aqua: "1BAF7A", // M3
  red: "C8402E",
};
const HEAD = "Calibri";
const BODY = "Calibri";

const pres = new pptxgen();
// QA: record every element so scripts can measure text fit without a renderer.
const QA_LOG = [];
const _addSlide = pres.addSlide.bind(pres);
pres.addSlide = (...a) => {
  const slide = _addSlide(...a);
  const idx = QA_LOG.length;
  QA_LOG.push({ background: null, items: [] });
  for (const m of ["addText", "addShape", "addImage", "addChart", "addTable"]) {
    const orig = slide[m].bind(slide);
    slide[m] = (...args) => {
      const [a0, a1, a2] = args;
      QA_LOG[idx].items.push(JSON.parse(JSON.stringify({ kind: m, a0: typeof a0 === "string" ? a0 : (m === "addTable" ? a0 : (m === "addChart" ? null : a0)), opts: m === "addChart" ? a2 : a1, shape: m === "addShape" ? a0 : undefined })));
      return orig(...args);
    };
  }
  Object.defineProperty(slide, "background", { set(v) { QA_LOG[idx].background = v; slide._bkgd = v; this._bg = v; }, get() { return this._bg; } });
  return slide;
};
pres.layout = "LAYOUT_WIDE"; // 13.333 x 7.5
pres.title = "When does parallel EVM execution pay?";
const W = 13.333;
const M = 0.6; // margin

function title(slide, text, opts = {}) {
  slide.addText(text, {
    x: M, y: 0.45, w: W - 2 * M, h: 0.9, fontFace: HEAD, fontSize: 32, bold: true,
    color: opts.dark ? C.paper : C.text, margin: 0, isTextBox: true, valign: "top",
  });
}
function kicker(slide, text, opts = {}) {
  slide.addText(text.toUpperCase(), {
    x: M, y: 0.2, w: 8, h: 0.3, fontFace: BODY, fontSize: 11, bold: true, charSpacing: 2,
    color: opts.dark ? C.orange : C.orange, margin: 0, isTextBox: true,
  });
}
function body(slide, text, x, y, w, h, opts = {}) {
  slide.addText(text, {
    x, y, w, h, fontFace: BODY, fontSize: opts.size || 16, color: opts.color || C.text,
    margin: 0, isTextBox: true, valign: opts.valign || "top", paraSpaceAfter: 6, bold: !!opts.bold,
    align: opts.align || "left",
  });
}
function stat(slide, big, small, x, y, w, opts = {}) {
  slide.addText(big, {
    x, y, w, h: 0.95, fontFace: HEAD, fontSize: opts.size || 48, bold: true,
    color: opts.color || C.orange, margin: 0, isTextBox: true, valign: "bottom",
  });
  slide.addText(small, {
    x, y: y + 1.0, w, h: 0.7, fontFace: BODY, fontSize: 13, color: opts.muted || C.muted,
    margin: 0, isTextBox: true, valign: "top",
  });
}
function card(slide, x, y, w, h, fill) {
  slide.addShape(pres.shapes.ROUNDED_RECTANGLE, {
    x, y, w, h, rectRadius: 0.12, fill: { color: fill || C.tint }, line: { color: fill || C.tint },
  });
}
function chip(slide, text, x, y, color) {
  slide.addShape(pres.shapes.ROUNDED_RECTANGLE, {
    x, y, w: 0.34, h: 0.34, rectRadius: 0.17, fill: { color }, line: { color },
  });
  slide.addText(text, {
    x, y, w: 0.34, h: 0.34, fontFace: HEAD, fontSize: 12, bold: true, color: C.paper,
    align: "center", valign: "middle", margin: 0, isTextBox: true,
  });
}
function image(slide, file, x, y, w, aspect) {
  slide.addImage({ path: FIG(file), x, y, w, h: w / aspect });
}
function source(slide, text, opts = {}) {
  slide.addText(text, {
    x: M, y: 7.0, w: W - 2 * M, h: 0.3, fontFace: BODY, fontSize: 10,
    color: opts.dark ? C.mutedDark : C.muted, margin: 0, isTextBox: true,
  });
}

// 1 — Title --------------------------------------------------------------
{
  const s = pres.addSlide();
  s.background = { color: C.ink };
  s.addText("SC6109 BLOCKCHAIN SCALABILITY · OPTION 5", {
    x: M, y: 0.9, w: 9, h: 0.35, fontFace: BODY, fontSize: 12, bold: true, charSpacing: 2,
    color: C.orange, margin: 0, isTextBox: true,
  });
  s.addText("When does parallel EVM execution pay?", {
    x: M, y: 1.45, w: 8.2, h: 2.2, fontFace: HEAD, fontSize: 48, bold: true, color: C.paper,
    margin: 0, isTextBox: true, valign: "top",
  });
  s.addText("A Block-STM engine over real EVM bytecode — three schedulers, twenty workloads, and the cases where parallelism makes things slower.", {
    x: M, y: 3.85, w: 7.6, h: 1.2, fontFace: BODY, fontSize: 18, color: C.mutedDark,
    margin: 0, isTextBox: true, valign: "top",
  });
  card(s, 9.2, 1.45, 3.5, 2.05, C.inkSoft);
  s.addText("5.6×", { x: 9.5, y: 1.6, w: 3, h: 1.0, fontFace: HEAD, fontSize: 54, bold: true, color: C.aqua, margin: 0, isTextBox: true });
  s.addText("independent, compute-heavy", { x: 9.5, y: 2.65, w: 3, h: 0.5, fontFace: BODY, fontSize: 13, color: C.mutedDark, margin: 0, isTextBox: true });
  card(s, 9.2, 3.7, 3.5, 2.05, C.inkSoft);
  s.addText("0.43×", { x: 9.5, y: 3.85, w: 3, h: 1.0, fontFace: HEAD, fontSize: 54, bold: true, color: C.orange, margin: 0, isTextBox: true });
  s.addText("one NFT mint, one dependency chain", { x: 9.5, y: 4.9, w: 3, h: 0.5, fontFace: BODY, fontSize: 13, color: C.mutedDark, margin: 0, isTextBox: true });
  s.addText("Block-STM, 6 performance cores, vs. sequential", { x: 9.2, y: 5.9, w: 3.5, h: 0.3, fontFace: BODY, fontSize: 11, color: C.mutedDark, margin: 0, isTextBox: true });
  s.addNotes(
    "Blockchains execute transactions one at a time, even though most transactions in a block never touch the same state. " +
    "Our project asks the question from the brief directly: when does executing them in parallel actually help, and when does it not? " +
    "We built a parallel execution engine on real EVM bytecode, with three different schedulers, and measured it on twenty workloads. " +
    "The two numbers on the right are the answer in miniature: five and a half times faster when transactions are independent and do real work, " +
    "and less than half the speed of plain sequential execution when every transaction depends on the one before it."
  );
}

// 2 — The problem -----------------------------------------------------------
{
  const s = pres.addSlide();
  s.background = { color: C.paper };
  kicker(s, "The question");
  title(s, "The dependency bound says 1,000×. We measured 1.4×.");
  body(s,
    "Take 2,000 ETH transfers between random accounts in a large population. Almost none of them touch the same account: the longest chain of dependencies is two transactions long, so in principle they could run a thousand ways in parallel.\n\n" +
    "On six cores, Block-STM runs that block 1.4 times faster than sequential. It runs a block of ERC-20 transfers 2.8 times faster, compute-heavy calls 5.6 times faster — and an NFT mint at less than half speed.\n\n" +
    "The gap between the bound and the measurement is the subject of this project.",
    M, 1.65, 6.6, 4.6, { size: 17 });
  const col = 7.9;
  const rows = [
    ["1,000×", "dependency ceiling for independent transfers", C.muted],
    ["1.4×", "measured, ETH transfers", C.blue],
    ["2.8×", "measured, ERC-20 transfers", C.aqua],
    ["0.43×", "measured, NFT mint", C.orange],
  ];
  rows.forEach(([big, small, color], i) => {
    const y = 1.6 + i * 1.3;
    card(s, col, y, 4.8, 1.1);
    s.addText(big, { x: col + 0.3, y: y + 0.1, w: 1.9, h: 0.9, fontFace: HEAD, fontSize: 34, bold: true, color, margin: 0, isTextBox: true, valign: "middle" });
    s.addText(small, { x: col + 2.25, y: y + 0.1, w: 2.4, h: 0.9, fontFace: BODY, fontSize: 14, color: C.text, margin: 0, isTextBox: true, valign: "middle" });
  });
  source(s, "Block-STM at 6 threads on an Apple M3 Pro, 2,000-transaction blocks. Ceiling = block size ÷ critical path.");
  s.addNotes(
    "Here is the puzzle that frames everything. A block of two thousand transfers between random accounts has almost no dependencies — " +
    "the dependency bound says it could run a thousand times faster. We measured one point four. " +
    "ERC-20 transfers do better, compute-heavy calls much better, and an NFT mint is actually slower than running it sequentially. " +
    "Explaining that gap — where the missing speedup goes — is what the rest of the talk is about."
  );
}

// 3 — Architecture ----------------------------------------------------------
{
  const s = pres.addSlide();
  s.background = { color: C.paper };
  kicker(s, "What we built");
  title(s, "One execution path, three schedulers, every read recorded");
  const layers = [
    ["revm 41", "real EVM, embedded as a library", C.ink],
    ["ReadRecorder", "the only DatabaseRef in the crate — no read reaches the EVM unlogged", C.orange],
    ["MVView  ·  SimpleView", "multi-version memory for the parallel engines · plain state for the baseline", C.blue],
    ["BaseState", "pre-block snapshot: accounts, storage, compiled contract code", C.muted],
  ];
  layers.forEach(([name, what, color], i) => {
    const y = 1.7 + i * 1.12;
    s.addShape(pres.shapes.ROUNDED_RECTANGLE, { x: M, y, w: 3.6, h: 0.86, rectRadius: 0.1, fill: { color }, line: { color } });
    s.addText(name, { x: M, y, w: 3.6, h: 0.86, fontFace: HEAD, fontSize: 17, bold: true, color: C.paper, align: "center", valign: "middle", margin: 0, isTextBox: true });
    s.addText(what, { x: M + 3.85, y, w: 3.9, h: 0.86, fontFace: BODY, fontSize: 14, color: C.text, valign: "middle", margin: 0, isTextBox: true });
    if (i < layers.length - 1) {
      s.addShape(pres.shapes.LINE, { x: M + 1.8, y: y + 0.86, w: 0, h: 0.26, line: { color: C.muted, width: 1.5 } });
    }
  });
  card(s, 8.75, 1.7, 4.0, 4.3);
  body(s, "Why it is shaped this way", 9.0, 1.9, 3.5, 0.4, { size: 15, bold: true });
  body(s,
    "Block-STM is only correct if every read is re-checked. A read the recorder misses is a silent bug that appears on one seed in hundreds.\n\n" +
    "So reads are captured in one place, by construction — and the sequential baseline goes through the same path, so it is never quietly cheaper than what it is compared against.",
    9.0, 2.4, 3.5, 3.5, { size: 13.5, color: C.text });
  source(s, "Rust · revm 41.0.0 · rayon · Solidity 0.8.28 via Foundry");
  s.addNotes(
    "Everything runs through one execution path. The EVM is revm, embedded as a library, running bytecode compiled from Solidity. " +
    "Between the EVM and state sits a read recorder — the only type in the code base that the EVM can read through — so every read is logged. " +
    "That matters because optimistic parallel execution is only correct if every read is re-checked; a read that slips through is a bug that shows up on one run in a few hundred. " +
    "Below that sit two views of state: multi-version memory for the parallel schedulers, and plain state for the sequential baseline."
  );
}

// 4 — Three schedulers ------------------------------------------------------
{
  const s = pres.addSlide();
  s.background = { color: C.paper };
  kicker(s, "Three ways to find parallelism");
  title(s, "Speculate in rounds, collaborate, or declare up front");
  const cols = [
    ["M2a", "Round-based optimistic", C.blue,
      "Run every pending transaction at once, validate all, re-run the failures, repeat.",
      "Simple; provably ends within n rounds.",
      "On a dependency chain, re-runs everything above each fix: quadratic."],
    ["M2b", "Block-STM", C.orange,
      "Workers share execution and validation tasks; a read of an aborted write parks the reader until the writer finishes.",
      "Degrades gracefully under contention.",
      "Pays for versioning and validation even when nothing conflicts."],
    ["M3", "Static, EIP-7928 style", C.aqua,
      "Derive every transaction's access set as a block builder would, then run conflict-free levels in parallel.",
      "No speculation, no aborts; fastest when conflicts are rare.",
      "One barrier per level of the chain; needs the access list first."],
  ];
  cols.forEach(([tag, name, color, how, good, bad], i) => {
    const x = M + i * 4.1;
    card(s, x, 1.7, 3.85, 4.45);
    chip(s, String(i + 1), x + 0.3, 1.95, color);
    s.addText(tag, { x: x + 0.75, y: 1.95, w: 1.0, h: 0.34, fontFace: HEAD, fontSize: 13, bold: true, color: C.muted, margin: 0, isTextBox: true, valign: "middle" });
    s.addText(name, { x: x + 0.3, y: 2.45, w: 3.3, h: 0.5, fontFace: HEAD, fontSize: 19, bold: true, color: C.text, margin: 0, isTextBox: true });
    body(s, how, x + 0.3, 3.05, 3.3, 1.3, { size: 13.5 });
    body(s, "+ " + good, x + 0.3, 4.4, 3.3, 0.75, { size: 13, color: C.text });
    body(s, "− " + bad, x + 0.3, 5.2, 3.3, 0.8, { size: 13, color: C.muted });
  });
  s.addNotes(
    "We built three schedulers, because the interesting question is not just whether parallelism helps but which kind. " +
    "The first runs everything optimistically in rounds: execute all, validate all, re-run what failed. It is simple and provably terminates, but on a chain it re-runs everything above each fix. " +
    "The second is Block-STM, the scheduler behind Aptos: workers share execution and validation tasks, and a transaction that would read a value about to change is parked until it is ready. " +
    "The third is static: it derives every transaction's access set the way a block builder would under Ethereum's proposed block access lists, EIP-7928, and runs conflict-free groups in parallel with no speculation at all."
  );
}

// 5 — Conflicts ---------------------------------------------------------------
{
  const s = pres.addSlide();
  s.background = { color: C.paper };
  kicker(s, "Deciding what conflicts");
  title(s, "A conflict is a changed value — not a touched account");
  const rows = [
    ["1", C.orange, "Read set: every location the EVM loaded", "Balances, nonces, code hashes, storage slots — captured at the database boundary, each with the version it resolved to. Validation compares versions, not values."],
    ["2", C.blue, "Write set: only what actually changed", "Calling a contract touches its account without changing it. Counting touched as written made every call to one contract conflict with every other — a bug our benchmarks exposed."],
    ["3", C.aqua, "The fee recipient is an accumulator", "Every transaction pays the block beneficiary. Fees add up in any order, so they are kept out of conflict detection and summed at commit."],
  ];
  rows.forEach(([n, color, head, text], i) => {
    const y = 1.75 + i * 1.45;
    chip(s, n, M, y + 0.05, color);
    s.addText(head, { x: M + 0.6, y, w: 6.9, h: 0.45, fontFace: HEAD, fontSize: 18, bold: true, color: C.text, margin: 0, isTextBox: true });
    body(s, text, M + 0.6, y + 0.48, 6.9, 0.9, { size: 13.5, color: C.muted });
  });
  card(s, 8.4, 1.75, 4.35, 4.1);
  body(s, "Granularity is a choice", 8.7, 1.95, 3.8, 0.4, { size: 15, bold: true });
  body(s,
    "Per storage slot: two ERC-20 transfers between different holders are independent.\n\n" +
    "Per account: every ERC-20 balance lives in the token contract's account — so every transfer conflicts with every other.\n\n" +
    "We implemented both and measured the difference (slide 11).",
    8.7, 2.45, 3.8, 3.3, { size: 13.5 });
  s.addNotes(
    "Conflict detection is where correctness and performance meet. A transaction's read set is everything the EVM loaded; its write set is only what it actually changed. " +
    "That distinction cost us a bug: at first we counted every account the EVM marked as touched, which made every call to the same contract look like a conflict. " +
    "The block's fee recipient gets special treatment — every transaction pays it, but fees add up in any order, so they are summed at the end. " +
    "And granularity is a design choice: per slot or per account. On ERC-20 that choice turns out to decide almost everything."
  );
}

// 6 — Correctness -------------------------------------------------------------
{
  const s = pres.addSlide();
  s.background = { color: C.ink };
  kicker(s, "Correctness first", { dark: true });
  title(s, "Every parallel result is bit-identical to sequential — and checked", { dark: true });
  stat(s, "1,000", "seeds per workload, every scheduler, 2–12 threads — slot-for-slot against an independent sequential engine", M, 1.8, 3.7, { color: C.orange, muted: C.mutedDark });
  stat(s, "9 / 9", "workloads where our sequential engine matches an Anvil node: balances, nonces, code, storage, reverts", M + 4.15, 1.8, 3.7, { color: C.aqua, muted: C.mutedDark });
  stat(s, "15", "bugs injected on purpose to test the tests — all caught, after two gaps in the tests were found and closed", M + 8.3, 1.8, 3.7, { color: C.paper, muted: C.mutedDark });
  card(s, M, 4.3, W - 2 * M, 2.35, C.inkSoft);
  body(s, "Real bugs the tests found", M + 0.35, 4.5, 5, 0.4, { size: 15, bold: true, color: C.paper });
  body(s,
    "Generated accounts landed on precompile addresses — 15% of a block silently called ecrecover instead of transferring.\n" +
    "Touched-but-unchanged accounts counted as writes — every contract workload serialised through its own contract.\n" +
    "A read race under account granularity let a stale nonce pass validation — appeared once in a full run, reproduced by a stress test in 0.5 s.",
    M + 0.35, 4.95, W - 2 * M - 0.7, 1.6, { size: 13.5, color: C.mutedDark });
  s.addNotes(
    "Before trusting any speedup, we made sure the answers are right. Every parallel scheduler is compared slot for slot with an independent sequential engine on a thousand random blocks per workload. " +
    "The sequential engine is itself checked against Anvil, a real Ethereum node, on nine workloads. " +
    "And we tested the tests: we injected fifteen bugs on purpose; all are caught, though two initially slipped past, which is how we found holes in the tests. " +
    "Along the way the tests caught three real bugs — including a race that appeared once in a full run and that a stress test then reproduced in half a second."
  );
}

// 7 — Scaling ---------------------------------------------------------------
{
  const s = pres.addSlide();
  s.background = { color: C.paper };
  kicker(s, "Result 1");
  title(s, "Three regimes: scales, scales a little, gets slower");
  image(s, "fig1_speedup_by_threads.png", M, 1.45, 8.9, 1.669);
  body(s, "Independent and heavy", 9.8, 1.7, 3.0, 0.4, { size: 15, bold: true });
  body(s, "compute calls reach 5.2× at 6 threads and keep climbing onto efficiency cores.", 9.8, 2.1, 3.0, 1.0, { size: 13 });
  body(s, "Independent and light", 9.8, 3.2, 3.0, 0.4, { size: 15, bold: true });
  body(s, "ETH and ERC-20 transfers peak at 1.4–3.0× around 4–6 threads, then lose speed on efficiency cores.", 9.8, 3.6, 3.0, 1.0, { size: 13 });
  body(s, "Dependent", 9.8, 4.7, 3.0, 0.4, { size: 15, bold: true });
  body(s, "NFT mint and one AMM pool: every scheduler is slower than sequential.", 9.8, 5.1, 3.0, 1.0, { size: 13 });
  source(s, "Shaded: threads beyond the 6 performance cores. Median of 5 runs; every run verified.");
  s.addNotes(
    "Here is speedup against thread count for six representative workloads. Three shapes appear. " +
    "Independent, compute-heavy calls scale almost linearly — over five times on six cores. " +
    "Independent but light transactions, like transfers, gain something up to about six threads, then get slower once work lands on the chip's efficiency cores. " +
    "And dependent workloads — an NFT mint, swaps on one pool — are slower than sequential under every scheduler, at every thread count."
  );
}

// 8 — Density vs critical path ---------------------------------------------
{
  const s = pres.addSlide();
  s.background = { color: C.paper };
  kicker(s, "Result 2");
  title(s, "Chain length kills speedup — not how many transactions conflict");
  image(s, "fig2_speedup_by_density.png", M, 1.4, 5.8, 1.142);
  const x = 7.1;
  body(s, "Block-STM, 6 threads", x, 1.55, 5.6, 0.4, { size: 14, bold: true, color: C.muted });
  const t = [
    ["Workload", "Density", "Chain", "Speedup"],
    ["transfers, r = 1", "0.75", "11", "1.29×"],
    ["transfers, Zipf 0.8", "0.81", "110", "1.26×"],
    ["transfers, Zipf 1.2", "0.90", "432", "1.07×"],
    ["ERC-20, Zipf 1.2", "0.90", "432", "1.67×"],
    ["transfers, Zipf 2.0", "0.98", "1,217", "0.45×"],
    ["NFT mint", "1.00", "2,000", "0.43×"],
  ];
  s.addTable(t.map((r, i) => r.map((c, j) => ({
    text: c,
    options: {
      bold: i === 0 || (j === 3 && parseFloat(c) < 1), fontFace: BODY, fontSize: 13,
      color: i === 0 ? C.muted : (j === 3 && parseFloat(c) < 1 ? C.orange : C.text),
      align: j === 0 ? "left" : "right", valign: "middle",
      border: [{ type: "none" }, { type: "none" }, { pt: 0.75, color: C.line }, { type: "none" }],
    },
  }))), { x, y: 2.0, w: 5.6, colW: [2.4, 1.0, 1.0, 1.2], rowH: 0.42, margin: [0, 4, 0, 4] });
  body(s, "Same chain, more work per transaction: ERC-20 gets 1.67× where transfers get 1.07×. Density alone does not describe a workload.", x, 5.15, 5.6, 1.0, { size: 13.5 });
  source(s, "Density: fraction of transactions reading an earlier transaction's write, measured from a real execution.");
  s.addNotes(
    "We placed every workload by its measured dependency density — the fraction of transactions that read something an earlier transaction wrote. " +
    "Surprisingly, density is not what kills speedup. Blocks where three quarters of transactions depend on another still run at 1.3 times, because the chains are short. " +
    "What matters is the length of the longest chain: at 432 Block-STM is still ahead; at 1,200 and at 2,000 it falls to under half of sequential speed. " +
    "And look at the two rows with a chain of 432 — identical dependency structure, but ERC-20 transactions do more work, so they get 1.67 instead of 1.07."
  );
}

// 9 — Work per transaction (native chart) ----------------------------------
{
  const s = pres.addSlide();
  s.background = { color: C.paper };
  kicker(s, "Result 3");
  title(s, "A transaction has to do more work than it costs to coordinate");
  const labels = ["1.6 µs (0 B)", "4.1 µs (1 KB)", "21 µs (8 KB)", "78 µs (32 KB)"];
  s.addChart(pres.charts.LINE, [
    { name: "M2a round-based", labels, values: [1.07, 3.11, 5.05, 5.51] },
    { name: "M2b Block-STM", labels, values: [1.61, 3.18, 5.16, 5.58] },
    { name: "M3 static", labels, values: [1.80, 3.58, 5.29, 5.46] },
  ], {
    x: M, y: 1.5, w: 8.0, h: 5.2, chartColors: [C.blue, C.orange, C.aqua], lineSize: 2.5, lineDataSymbolSize: 9,
    catAxisLabelColor: C.muted, valAxisLabelColor: C.muted, catAxisLabelFontSize: 12, valAxisLabelFontSize: 12,
    valAxisMinVal: 0, valAxisMaxVal: 6, valAxisMajorUnit: 1, valGridLine: { color: C.line, size: 1 }, catGridLine: { style: "none" },
    showLegend: true, legendPos: "b", legendFontSize: 12, legendColor: C.text,
    showTitle: true, title: "Speedup at 6 threads vs sequential time per transaction", titleFontSize: 14, titleColor: C.text,
    showValAxisTitle: true, valAxisTitle: "speedup", valAxisTitleColor: C.muted, valAxisTitleFontSize: 12,
  });
  stat(s, "~40%", "of sequential throughput lost to Block-STM's own machinery at one thread, on light transactions", 9.3, 1.7, 3.4, { color: C.orange });
  stat(s, "92%", "of linear on 6 cores once each transaction does ~78 µs of work", 9.3, 3.9, 3.4, { color: C.aqua });
  source(s, "sha256 precompile calls with a growing payload; conflicts held near zero.");
  s.addNotes(
    "To isolate the second factor we held conflicts at zero and varied how much work each transaction does, by hashing a growing payload. " +
    "With almost no work — about a microsecond and a half — the speedup is only one and a half. With seventy-eight microseconds of work it is five and a half, ninety-two percent of the six cores. " +
    "The reason is a roughly fixed cost per transaction for versioning and validation: at one thread Block-STM loses up to forty percent to its own machinery on light transactions. " +
    "A plain payment is on the wrong side of this curve no matter how independent the block is."
  );
}

// 10 — Scheduler design -----------------------------------------------------
{
  const s = pres.addSlide();
  s.background = { color: C.paper };
  kicker(s, "Result 4");
  title(s, "The scheduler matters as much as the workload");
  image(s, "fig4_work_amplification.png", M, 1.45, 8.2, 1.731);
  stat(s, "247×", "executions per transaction on an NFT mint under round-based execution — speedup 0.01×", 9.3, 1.6, 3.4, { color: C.blue });
  stat(s, "1.9×", "under Block-STM on the same block — parking waiting transactions instead of re-running them", 9.3, 3.7, 3.4, { color: C.orange });
  source(s, "6 threads. At low density both run each transaction about once.");
  s.addNotes(
    "Same blocks, different schedulers. On a dependency chain, round-based execution confirms one more transaction per round and re-runs everything above it — two hundred and forty-seven executions per transaction on the NFT mint, and a speedup of one hundredth. " +
    "Block-STM runs each transaction fewer than twice, because a transaction that would read a value about to change is parked until it is ready instead of being run anyway. " +
    "That single mechanism is what makes optimistic execution survivable under contention. It is also a correctness mechanism: we removed it on purpose and the results went wrong."
  );
}

// 11 — Granularity (native chart) -------------------------------------------
{
  const s = pres.addSlide();
  s.background = { color: C.paper };
  kicker(s, "Result 5");
  title(s, "Per-account conflict detection throws ERC-20 parallelism away");
  s.addChart(pres.charts.BAR, [
    { name: "per storage slot", labels: ["M2a round-based", "M2b Block-STM", "M3 static"], values: [2.87, 2.92, 3.10] },
    { name: "per account", labels: ["M2a round-based", "M2b Block-STM", "M3 static"], values: [0.01, 0.50, 0.31] },
  ], {
    x: M, y: 1.5, w: 7.6, h: 5.2, barDir: "col", barGrouping: "clustered", barGapWidthPct: 60,
    chartColors: [C.blue, C.orange], catAxisLabelColor: C.text, valAxisLabelColor: C.muted,
    catAxisLabelFontSize: 13, valAxisLabelFontSize: 12, valAxisMinVal: 0, valAxisMaxVal: 3.5, valAxisMajorUnit: 0.5,
    valGridLine: { color: C.line, size: 1 }, catGridLine: { style: "none" },
    showValue: true, dataLabelPosition: "outEnd", dataLabelFontSize: 12, dataLabelColor: C.text, dataLabelFormatCode: "0.00\"×\"",
    showLegend: true, legendPos: "b", legendFontSize: 12, legendColor: C.text,
    showTitle: true, title: "ERC-20 transfers among many holders, speedup at 6 threads", titleFontSize: 14, titleColor: C.text,
  });
  body(s, "Why", 8.7, 1.7, 4.0, 0.4, { size: 15, bold: true });
  body(s, "Every ERC-20 balance is a storage slot in the token contract. Per slot, transfers between different holders never collide. Per account, they all collide in one account — the token.", 8.7, 2.15, 4.0, 1.9, { size: 13.5 });
  body(s, "So what", 8.7, 4.15, 4.0, 0.4, { size: 15, bold: true });
  body(s, "An access-list design is only as good as its granularity. EIP-7928 lists storage keys; an account-level list would not work for contracts. Plain ETH transfers — accounts without storage — are identical under both, as they should be.", 8.7, 4.6, 4.0, 2.0, { size: 13.5 });
  s.addNotes(
    "Granularity turned out to be decisive. Every ERC-20 balance is a storage slot inside the token contract. " +
    "If conflicts are tracked per slot, transfers between different holders are independent and all three schedulers get about three times. " +
    "If they are tracked per account, every transfer collides in the token contract's account and the speedup disappears. " +
    "That is exactly why Ethereum's proposed block access lists record storage keys, not just accounts."
  );
}

// 12 — When it helps --------------------------------------------------------
{
  const s = pres.addSlide();
  s.background = { color: C.paper };
  kicker(s, "The answer");
  title(s, "Parallel execution pays only when all three hold");
  s.addChart(pres.charts.BAR, [
    { name: "Block-STM, 6 threads", labels: ["compute-heavy, independent", "ERC-20, many holders", "ETH transfers, independent", "ERC-20, per-account detection", "NFT mint (one chain)"], values: [5.58, 2.80, 1.41, 0.50, 0.43] },
  ], {
    x: M, y: 1.5, w: 6.6, h: 5.1, barDir: "bar", chartColors: [C.orange], barGapWidthPct: 45,
    catAxisLabelColor: C.text, valAxisLabelColor: C.muted, catAxisLabelFontSize: 12, valAxisLabelFontSize: 11,
    valAxisMinVal: 0, valAxisMaxVal: 6.5, valGridLine: { color: C.line, size: 1 }, catGridLine: { style: "none" },
    showValue: true, dataLabelPosition: "outEnd", dataLabelFontSize: 12, dataLabelColor: C.text, dataLabelFormatCode: "0.00\"×\"",
    showLegend: false, catAxisOrientation: "maxMin",
    showTitle: true, title: "Speedup over sequential (1.0 = no gain)", titleFontSize: 13, titleColor: C.muted,
  });
  const conds = [
    ["1", "Short dependency chains", "A hot contract is a ceiling no engine removes — only the application can shard it."],
    ["2", "Real work per transaction", "Coordination costs a roughly fixed amount; light transactions never earn it back."],
    ["3", "Fine-grained conflicts, real cores", "Per-slot detection, and no more threads than performance cores for light work."],
  ];
  conds.forEach(([n, head, text], i) => {
    const y = 1.65 + i * 1.6;
    chip(s, n, 7.7, y + 0.05, C.orange);
    s.addText(head, { x: 8.25, y, w: 4.5, h: 0.45, fontFace: HEAD, fontSize: 17, bold: true, color: C.text, margin: 0, isTextBox: true });
    body(s, text, 8.25, y + 0.5, 4.5, 1.0, { size: 13.5, color: C.muted });
  });
  s.addNotes(
    "So, when does parallel execution help? Only when three things hold at once. Chains must be short — a single hot contract is a ceiling no execution engine can remove. " +
    "Transactions must do real work, enough to pay for coordinating them. And conflicts must be tracked finely, on cores that are really there. " +
    "Lose the first and parallel execution is actively slower than sequential. " +
    "In practice that means big gains for busy chains with diverse contract activity, and little or nothing for a block dominated by one popular mint or one busy pool — which is often exactly what congestion looks like."
  );
}

// 13 — Relation to production -----------------------------------------------
{
  const s = pres.addSlide();
  s.background = { color: C.paper };
  kicker(s, "In context");
  title(s, "How this maps onto production systems");
  const rows = [
    ["Aptos · Block-STM", C.orange, "Our M2b follows the Block-STM paper on EVM bytecode instead of Move, and shows the same shape: large gains at low contention, graceful degradation, no collapse."],
    ["Solana · Sealevel", C.blue, "Transactions declare the accounts they touch. On the EVM that is account-level detection — which our ERC-20 result shows discards most of the parallelism."],
    ["Ethereum · EIP-7928", C.aqua, "Builders publish block access lists with storage keys, so validators can schedule statically. Our M3 is that validator; its list costs a full execution, paid once by the builder."],
    ["ERC-4337 bundles", C.muted, "Many users' operations in one transaction through one EntryPoint — concentrating state, which is the unfavourable case for parallel execution."],
  ];
  rows.forEach(([head, color, text], i) => {
    const y = 1.65 + i * 1.28;
    card(s, M, y, W - 2 * M, 1.1);
    s.addShape(pres.shapes.OVAL, { x: M + 0.3, y: y + 0.4, w: 0.3, h: 0.3, fill: { color }, line: { color } });
    s.addText(head, { x: M + 0.85, y, w: 3.2, h: 1.1, fontFace: HEAD, fontSize: 17, bold: true, color: C.text, margin: 0, isTextBox: true, valign: "middle" });
    body(s, text, M + 4.1, y + 0.08, W - 2 * M - 4.4, 0.95, { size: 13.5, valign: "middle" });
  });
  s.addNotes(
    "Finally, how this connects to real systems. Our Block-STM follows the scheduler Aptos uses, on EVM bytecode, and we see the same shape. " +
    "Solana's model — declaring the accounts a transaction touches — corresponds on the EVM to account-level detection, and our ERC-20 result shows how much that throws away. " +
    "Ethereum's EIP-7928 proposes block access lists with storage keys; our static scheduler is what a validator could do with them, while the builder pays the cost of producing the list. " +
    "And account-abstraction bundles concentrate many users into one transaction and one contract, which is the unfavourable case."
  );
}

// 14 — Limitations & close --------------------------------------------------
{
  const s = pres.addSlide();
  s.background = { color: C.ink };
  kicker(s, "Limits and next steps", { dark: true });
  title(s, "What we would do next", { dark: true });
  const items = [
    ["Replay real mainnet blocks", "Our workloads span the conflict and work axes by design; real traffic would show where chains actually sit on them."],
    ["A homogeneous machine", "Results above six threads mix in efficiency cores; a server CPU would separate the scheduler from the chip."],
    ["Emit EIP-7928 lists", "Produce canonical block access lists, and measure the builder's side as well as the validator's."],
  ];
  items.forEach(([head, text], i) => {
    const x = M + i * 4.1;
    card(s, x, 1.8, 3.85, 2.45, C.inkSoft);
    s.addText(head, { x: x + 0.3, y: 2.05, w: 3.3, h: 0.5, fontFace: HEAD, fontSize: 18, bold: true, color: C.paper, margin: 0, isTextBox: true, valign: "top" });
    body(s, text, x + 0.3, 2.6, 3.3, 1.5, { size: 13.5, color: C.mutedDark });
  });
  s.addText("Code, data, figures and the full report: github.com/IcantFind-a-username/sc6109-parallel-evm", {
    x: M, y: 4.9, w: W - 2 * M, h: 0.5, fontFace: BODY, fontSize: 15, color: C.paper, margin: 0, isTextBox: true,
  });
  s.addText("Thank you", {
    x: M, y: 5.6, w: 6, h: 0.8, fontFace: HEAD, fontSize: 32, bold: true, color: C.orange, margin: 0, isTextBox: true,
  });
  s.addNotes(
    "Three things we would do next. Replay real mainnet blocks, to see where real traffic sits on the axes we measured. " +
    "Run on a machine with uniform cores, so results above six threads measure the scheduler rather than the chip. " +
    "And emit real EIP-7928 access lists, measuring the builder's side too. " +
    "Everything — code, raw data, figures and the full report — is in the repository. Thank you."
  );
}

if (process.env.QA) require("fs").writeFileSync(path.join(__dirname, "qa.json"), JSON.stringify(QA_LOG));
pres.writeFile({ fileName: path.join(ROOT, "docs", "SC6109_parallel_evm.pptx") }).then((f) => console.log("wrote", f));
