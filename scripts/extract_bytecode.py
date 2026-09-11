#!/usr/bin/env python3
"""Copy each workload contract's runtime bytecode from Foundry's output into
engine/assets/, where the engine embeds it with include_str!.

Committing the extracted bytecode means the engine builds and tests without
Foundry installed, and CI does not need solc. Run after `forge build` whenever
a contract changes; `engine/tests/contracts.rs` fails if the embedded code and
the storage layout it assumes drift apart.
"""
import json
import pathlib

ROOT = pathlib.Path(__file__).resolve().parent.parent
for name in ("Token", "Collectible", "Pool"):
    artifact = ROOT / "contracts" / "out" / f"{name}.sol" / f"{name}.json"
    code = json.loads(artifact.read_text())["deployedBytecode"]["object"]
    code = code[2:] if code.startswith("0x") else code
    target = ROOT / "engine" / "assets" / f"{name}.runtime.hex"
    target.write_text(code + "\n")
    print(f"{name}: {len(code) // 2} bytes -> {target.relative_to(ROOT)}")
