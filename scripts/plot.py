#!/usr/bin/env python3
"""Figures for the report and slides, from results/final/*.csv.

    python3 scripts/plot.py            # writes docs/figures/*.png

Conventions (the dataviz method): one y-axis per panel — a second measure gets
its own panel, never a second scale; the three parallel schedulers keep fixed
categorical slots everywhere (validated all-pairs for colour-vision
deficiency) plus a distinct marker shape each, so identity never rests on
colour alone; text is ink, never series colour; thread counts above the six
performance cores are shaded and labelled, because they measure the machine as
much as the scheduler (EXPERIMENTS.md section 6.1).
"""
import csv
import pathlib

import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt  # noqa: E402

ROOT = pathlib.Path(__file__).resolve().parent.parent
RESULTS = ROOT / "results" / "final"
OUT = ROOT / "docs" / "figures"

SURFACE, INK, INK2, GRID = "#fcfcfb", "#0b0b0b", "#52514e", "#e4e3df"
SCHED = {  # fixed slots: blue, orange, aqua
    "blockstm-rounds": ("M2a round-based", "#2a78d6", "o"),
    "blockstm": ("M2b Block-STM", "#eb6834", "s"),
    "static": ("M3 static (EIP-7928)", "#1baf7a", "^"),
}
PAIR = ("#2a78d6", "#eb6834")  # two-series charts: slots 1 and 2
P_CORES = 6

plt.rcParams.update({
    "figure.facecolor": SURFACE, "axes.facecolor": SURFACE, "savefig.facecolor": SURFACE,
    "axes.edgecolor": GRID, "axes.labelcolor": INK2, "axes.titlecolor": INK,
    "axes.titlesize": 11, "axes.titleweight": "semibold", "axes.labelsize": 9.5,
    "xtick.color": INK2, "ytick.color": INK2, "xtick.labelsize": 8.5, "ytick.labelsize": 8.5,
    "axes.grid": True, "grid.color": GRID, "grid.linewidth": 0.8, "grid.linestyle": "-",
    "axes.spines.top": False, "axes.spines.right": False,
    "legend.frameon": False, "legend.fontsize": 8.5, "font.size": 9.5,
    "lines.linewidth": 2, "lines.markersize": 6.5, "lines.solid_capstyle": "round",
})


def load(name):
    with open(RESULTS / name) as f:
        rows = list(csv.DictReader(f))
    for r in rows:
        for k in ("threads", "transactions", "accounts", "critical_path"):
            r[k] = int(r[k])
        for k in ("dep_density", "ceiling", "speedup_median", "wall_ms_median", "seq_ms_median",
                  "abort_rate_median", "executions_median", "aborts_median", "prep_ms_median",
                  "rounds_median", "waits_median"):
            r[k] = float(r[k])
    return rows


def label(r):
    return f"{r['workload']} {r['param']}".replace("erc20tight", "erc20-tight").strip()


def shade_ecores(ax, xmax=12.5):
    ax.axvspan(P_CORES + 0.5, xmax, color=GRID, alpha=0.45, lw=0, zorder=0)
    ax.text(P_CORES + 0.7, 0.97, "efficiency cores", transform=ax.get_xaxis_transform(),
            fontsize=7.5, color=INK2, va="top")


def legend_below(fig, ax, n):
    handles, labels = ax.get_legend_handles_labels()
    fig.legend(handles, labels, loc="lower center", ncol=n, bbox_to_anchor=(0.5, -0.01))


def save(fig, name):
    OUT.mkdir(parents=True, exist_ok=True)
    fig.savefig(OUT / name, dpi=200, bbox_inches="tight")
    plt.close(fig)
    print("wrote", (OUT / name).relative_to(ROOT))


def fig1(rows):
    """Speedup against thread count, one panel per workload."""
    picks = [("transfer", "uniform-sparse"), ("compute", "8192b"), ("erc20", "uniform"),
             ("transfer", "zipf1.2"), ("nft", "mint"), ("amm", "swap")]
    fig, axes = plt.subplots(2, 3, figsize=(10.5, 6.2), sharex=True)
    for ax, (w, p) in zip(axes.flat, picks):
        cell = [r for r in rows if r["experiment"] == "main" and r["workload"] == w and r["param"] == p]
        if not cell:
            continue
        shade_ecores(ax)
        ax.axhline(1, color=INK2, lw=1, zorder=1)
        top = max(r["speedup_median"] for r in cell)
        for key, (name, color, marker) in SCHED.items():
            pts = sorted((r["threads"], r["speedup_median"]) for r in cell if r["scheduler"] == key)
            ax.plot(*zip(*pts), color=color, marker=marker, label=name, zorder=3,
                    markeredgecolor=SURFACE, markeredgewidth=1.5)
        c = cell[0]
        ax.set_title(f"{label(c)} · density {c['dep_density']:.2f}", loc="left", fontsize=10)
        ax.set_xticks([1, 2, 4, 6, 8, 12])
        ax.set_ylim(0, max(1.5, top * 1.15))
    for ax in axes[-1]:
        ax.set_xlabel("threads")
    for ax in axes[:, 0]:
        ax.set_ylabel("speedup over sequential")
    fig.suptitle("Speedup by thread count — the line at 1.0 is the sequential baseline",
                 x=0.01, ha="left", fontsize=12, fontweight="semibold", color=INK)
    fig.tight_layout(rect=(0, 0.05, 1, 0.97))
    legend_below(fig, axes.flat[0], 3)
    save(fig, "fig1_speedup_by_threads.png")


def fig2(rows):
    """Speedup and abort rate against measured dependency density, six threads."""
    main = [r for r in rows if r["experiment"] == "main" and r["threads"] == 6]
    fig, (a, b) = plt.subplots(2, 1, figsize=(8, 7), sharex=True, height_ratios=(3, 2))
    a.axhline(1, color=INK2, lw=1, zorder=1)
    for key, (name, color, marker) in SCHED.items():
        pts = [(r["dep_density"], r["speedup_median"]) for r in main if r["scheduler"] == key]
        a.scatter(*zip(*pts), s=48, color=color, marker=marker, label=name, zorder=3,
                  edgecolor=SURFACE, linewidth=1.5)
        if key != "static":
            pts = [(r["dep_density"], r["abort_rate_median"]) for r in main if r["scheduler"] == key]
            b.scatter(*zip(*pts), s=48, color=color, marker=marker, zorder=3,
                      edgecolor=SURFACE, linewidth=1.5)
    a.set_yscale("log")
    a.set_ylabel("speedup over sequential (log)")
    a.set_title("Speedup falls with measured dependency density (6 threads)", loc="left")
    for r in main:
        if r["scheduler"] == "blockstm" and (r["workload"] in ("nft", "amm") or r["param"] in ("uniform-sparse", "8192b")):
            dy = -11 if r["workload"] == "nft" else 4
            a.annotate(label(r), (r["dep_density"], r["speedup_median"]), textcoords="offset points",
                       xytext=(6, dy), fontsize=7.5, color=INK2)
    b.set_ylabel("abort rate")
    b.set_xlabel("measured dependency density — fraction of transactions reading an earlier write")
    b.set_title("…and aborts rise with it (the static scheduler never aborts)", loc="left")
    b.set_ylim(0, 1.02)
    a.legend(loc="upper center", bbox_to_anchor=(0.5, -0.04), ncol=3)
    fig.tight_layout()
    save(fig, "fig2_speedup_by_density.png")


def fig2b(rows):
    """Speedup against the dependency ceiling: how much of the bound is reached."""
    main = [r for r in rows if r["experiment"] == "main" and r["threads"] == 6]
    fig, ax = plt.subplots(figsize=(8, 4.8))
    xs = sorted({r["ceiling"] for r in main})
    ax.plot(xs, [min(x, P_CORES) for x in xs], color=INK2, lw=1, label="bound: min(ceiling, 6 cores)")
    for key, (name, color, marker) in SCHED.items():
        pts = [(r["ceiling"], r["speedup_median"]) for r in main if r["scheduler"] == key]
        ax.scatter(*zip(*pts), s=48, color=color, marker=marker, label=name, zorder=3,
                   edgecolor=SURFACE, linewidth=1.5)
    ax.set_xscale("log")
    ax.set_yscale("log")
    ax.set_xlabel("parallelism ceiling — block size ÷ critical path (log)")
    ax.set_ylabel("speedup (log)")
    ax.set_title("How close each scheduler gets to the dependency bound (6 threads)", loc="left")
    ax.legend(loc="upper left")
    fig.tight_layout()
    save(fig, "fig2b_speedup_vs_ceiling.png")


def fig3(rows):
    """Speedup against work per transaction, compute workload."""
    comp = [r for r in rows if r["experiment"] == "main" and r["workload"] == "compute"]
    fig, ax = plt.subplots(figsize=(8, 4.6))
    ax.axhline(1, color=INK2, lw=1, zorder=1)
    ax.axhline(P_CORES, color=GRID, lw=1.2, zorder=1)
    ax.text(0.99, P_CORES, "6 performance cores", transform=ax.get_yaxis_transform(), ha="right",
            va="bottom", fontsize=7.5, color=INK2)
    for key, (name, color, marker) in SCHED.items():
        pts = sorted((r["seq_ms_median"] * 1000 / r["transactions"], r["speedup_median"])
                     for r in comp if r["scheduler"] == key and r["threads"] == 6)
        ax.plot(*zip(*pts), color=color, marker=marker, label=name, zorder=3,
                markeredgecolor=SURFACE, markeredgewidth=1.5)
    ax.set_xscale("log")
    ticks = [1, 2, 5, 10, 20, 50, 100]
    ax.set_xticks(ticks, [str(t) for t in ticks])
    ax.minorticks_off()
    ax.set_xlabel("sequential time per transaction, µs (log) — sha256 payload 0 B to 32 KB")
    ax.set_ylabel("speedup over sequential")
    ax.set_title("Parallelism pays only once transactions do real work (6 threads, low conflict)", loc="left")
    ax.legend(loc="upper left")
    fig.tight_layout()
    save(fig, "fig3_speedup_by_work.png")


def fig4(rows):
    """Work amplification: executions per transaction, rounds versus Block-STM."""
    main = [r for r in rows if r["experiment"] == "main" and r["threads"] == 6]
    # Ties on density are broken by name: set order varies between runs with
    # string hash randomisation, which made the figure non-reproducible.
    cells = sorted({label(r) for r in main},
                   key=lambda l: (next(r["dep_density"] for r in main if label(r) == l), l))
    fig, ax = plt.subplots(figsize=(8, 5.4))
    h = 0.36
    for i, (key, color) in enumerate((("blockstm-rounds", PAIR[0]), ("blockstm", PAIR[1]))):
        vals = [next(r["executions_median"] / r["transactions"] for r in main
                     if label(r) == c and r["scheduler"] == key) for c in cells]
        ys = [j + (i - 0.5) * h for j in range(len(cells))]
        ax.barh(ys, vals, height=h * 0.92, color=color, label=SCHED[key][0], zorder=3)
        for y, v in zip(ys, vals):
            if v >= 2:
                ax.text(v * 1.08, y, f"{v:,.0f}×", va="center", fontsize=7.5, color=INK2)
    ax.set_yticks(range(len(cells)), cells)
    ax.set_xscale("log")
    ax.set_xlabel("executions per transaction (log) — 1 means nothing was re-executed")
    ax.set_title("Round-based execution re-runs dependency chains quadratically; Block-STM does not",
                 loc="left")
    ax.legend(loc="lower right")
    ax.grid(axis="y", visible=False)
    fig.tight_layout()
    save(fig, "fig4_work_amplification.png")


def fig5(rows):
    """Slot versus account granularity: the false conflicts of coarse detection."""
    g = [r for r in rows if r["experiment"] == "granularity" and r["scheduler"] != "sequential"]
    cells = list(dict.fromkeys(label(r) for r in g))
    fig, axes = plt.subplots(1, len(cells), figsize=(11, 3.8), sharey=True)
    for ax, c in zip(axes, cells):
        h = 0.36
        keys = list(SCHED)
        for i, (gran, color) in enumerate((("slot", PAIR[0]), ("account", PAIR[1]))):
            vals = [next(r["speedup_median"] for r in g if label(r) == c and r["scheduler"] == k
                         and r["granularity"] == gran) for k in keys]
            xs = [j + (i - 0.5) * h for j in range(len(keys))]
            ax.bar(xs, vals, width=h * 0.92, color=color, label=f"{gran} granularity", zorder=3)
        ax.axhline(1, color=INK2, lw=1, zorder=4)
        ax.set_xticks(range(len(keys)), ["M2a", "M2b", "M3"])
        ax.set_title(c, loc="left")
        ax.grid(axis="x", visible=False)
    axes[0].set_ylabel("speedup at 6 threads")
    fig.suptitle("Account-level conflict detection turns every ERC-20 transfer into a conflict",
                 x=0.01, ha="left", fontsize=12, fontweight="semibold", color=INK)
    fig.tight_layout(rect=(0, 0.07, 1, 0.95))
    legend_below(fig, axes[0], 2)
    save(fig, "fig5_granularity.png")


def fig6(rows):
    """Block size: per-block overhead against per-transaction gain."""
    b = [r for r in rows if r["experiment"] == "batch" and r["scheduler"] != "sequential"]
    cells = list(dict.fromkeys((r["workload"], r["param"]) for r in b))
    fig, axes = plt.subplots(1, len(cells), figsize=(9, 3.8), sharey=True)
    for ax, (w, p) in zip(axes, cells):
        ax.axhline(1, color=INK2, lw=1, zorder=1)
        for key in ("blockstm", "static"):
            name, color, marker = SCHED[key]
            pts = sorted((r["transactions"], r["speedup_median"]) for r in b
                         if r["workload"] == w and r["param"] == p and r["scheduler"] == key)
            ax.plot(*zip(*pts), color=color, marker=marker, label=name, zorder=3,
                    markeredgecolor=SURFACE, markeredgewidth=1.5)
        ax.set_xscale("log")
        ax.set_xticks([500, 2000, 8000], ["500", "2,000", "8,000"])
        ax.set_title(f"{w} {p}", loc="left")
        ax.set_xlabel("transactions per block (log)")
    axes[0].set_ylabel("speedup at 6 threads")
    fig.suptitle("Block size: bigger blocks amortise overhead — until memory does not",
                 x=0.01, ha="left", fontsize=12, fontweight="semibold", color=INK)
    fig.tight_layout(rect=(0, 0.08, 1, 0.94))
    legend_below(fig, axes[0], 2)
    save(fig, "fig6_block_size.png")


def fig7():
    """Allocator: the same binary under the system allocator and mimalloc."""
    files = {n: RESULTS / f"alloc_{n}.csv" for n in ("system", "mimalloc")}
    if not all(p.exists() for p in files.values()):
        print("skipping fig7: allocator results missing")
        return
    data = {n: load(p.name) for n, p in files.items()}
    cells = list(dict.fromkeys(label(r) for r in data["mimalloc"]))
    fig, (a, b) = plt.subplots(1, 2, figsize=(10, 4))
    h = 0.36
    for i, (n, color) in enumerate(zip(("system", "mimalloc"), PAIR)):
        seq = [next(r["seq_ms_median"] for r in data[n] if label(r) == c and r["scheduler"] == "sequential")
               for c in cells]
        a.barh([j + (i - 0.5) * h for j in range(len(cells))], seq, height=h * 0.92, color=color,
               label=f"{n} allocator", zorder=3)
        sp = [next(r["speedup_median"] for r in data[n] if label(r) == c and r["scheduler"] == "blockstm"
                   and r["threads"] == 6) for c in cells]
        b.barh([j + (i - 0.5) * h for j in range(len(cells))], sp, height=h * 0.92, color=color, zorder=3)
    for ax in (a, b):
        ax.set_yticks(range(len(cells)), cells)
        ax.grid(axis="y", visible=False)
    b.set_yticklabels([])
    a.set_xlabel("sequential block time, ms")
    b.set_xlabel("Block-STM speedup, 6 threads")
    a.set_title("Absolute cost", loc="left")
    b.set_title("Scaling", loc="left")
    fig.suptitle("The allocator is an experimental condition (D15)", x=0.01, ha="left",
                 fontsize=12, fontweight="semibold", color=INK)
    fig.tight_layout(rect=(0, 0.08, 1, 0.95))
    legend_below(fig, a, 2)
    save(fig, "fig7_allocator.png")


if __name__ == "__main__":
    rows = load("sweep.csv")
    for f in (fig1, fig2, fig2b, fig3, fig4, fig5, fig6):
        f(rows)
    fig7()


# --- Slide figures ------------------------------------------------------------
# Rendered as images rather than native PowerPoint charts: the iOS and macOS
# file previews do not draw native charts, and the deck must read on whatever
# a grader opens it with.

def slide_granularity(rows):
    g = [r for r in rows if r["experiment"] == "granularity" and r["workload"] == "erc20"
         and r["param"] == "uniform" and r["scheduler"] != "sequential"]
    keys = list(SCHED)
    fig, ax = plt.subplots(figsize=(7.6, 5.2))
    h = 0.36
    for i, (gran, color) in enumerate((("slot", PAIR[0]), ("account", PAIR[1]))):
        vals = [next(r["speedup_median"] for r in g if r["scheduler"] == k and r["granularity"] == gran)
                for k in keys]
        xs = [j + (i - 0.5) * h for j in range(len(keys))]
        ax.bar(xs, vals, width=h * 0.9, color=color, label=f"conflicts per {'storage slot' if gran == 'slot' else 'account'}",
               zorder=3)
        for x, v in zip(xs, vals):
            ax.text(x, v + 0.05, f"{v:.2f}×", ha="center", va="bottom", fontsize=10, color=INK)
    ax.axhline(1, color=INK2, lw=1, zorder=4)
    ax.text(2.62, 1.03, "sequential", fontsize=8.5, color=INK2, ha="right", va="bottom")
    ax.set_xticks(range(len(keys)), [SCHED[k][0] for k in keys], fontsize=10)
    ax.set_ylabel("speedup at 6 threads")
    ax.set_ylim(0, 3.6)
    ax.set_title("ERC-20 transfers among many holders", loc="left")
    ax.grid(axis="x", visible=False)
    ax.legend(loc="upper center", bbox_to_anchor=(0.5, -0.09), ncol=2)
    fig.tight_layout()
    save(fig, "slide_granularity_erc20.png")


def slide_answer(rows):
    main = {(r["workload"], r["param"]): r for r in rows if r["experiment"] == "main"
            and r["scheduler"] == "blockstm" and r["threads"] == 6}
    acct = next(r for r in rows if r["experiment"] == "granularity" and r["scheduler"] == "blockstm"
                and r["workload"] == "erc20" and r["param"] == "uniform" and r["granularity"] == "account")
    bars = [("compute-heavy, independent", main[("compute", "32768b")]["speedup_median"]),
            ("ERC-20, many holders", main[("erc20", "uniform")]["speedup_median"]),
            ("ETH transfers, independent", main[("transfer", "uniform-sparse")]["speedup_median"]),
            ("ERC-20, per-account detection", acct["speedup_median"]),
            ("NFT mint (one chain)", main[("nft", "mint")]["speedup_median"])]
    fig, ax = plt.subplots(figsize=(6.6, 5.1))
    ys = list(range(len(bars)))[::-1]
    ax.barh(ys, [v for _, v in bars], height=0.55, color=SCHED["blockstm"][1], zorder=3)
    for y, (_, v) in zip(ys, bars):
        # Labels of bars short of the baseline sit past it, not across it.
        ax.text(max(v, 1) + 0.08, y, f"{v:.2f}×", va="center", fontsize=10.5, color=INK)
    ax.axvline(1, color=INK2, lw=1, zorder=4)
    ax.text(1.05, len(bars) - 0.45, "sequential", fontsize=8.5, color=INK2, va="bottom")
    ax.set_yticks(ys, [n for n, _ in bars], fontsize=10.5)
    ax.set_xlim(0, 6.5)
    ax.set_xlabel("Block-STM speedup over sequential, 6 threads")
    ax.grid(axis="y", visible=False)
    fig.tight_layout()
    save(fig, "slide_answer_bars.png")


if __name__ == "__main__":
    rows = load("sweep.csv")
    slide_granularity(rows)
    slide_answer(rows)
