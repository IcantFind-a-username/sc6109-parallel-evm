#!/usr/bin/env python3
"""M1 gate: replay exported workloads on Anvil and compare with the engine.

The differential test proves the parallel engines match our sequential engine.
Only this proves our sequential engine matches the EVM as a real node runs it:
Anvil's own state handling, EIP-161 account clearing, fee accounting and
revert semantics, driven through its JSON-RPC interface.

    cargo run --release --bin bench -- export ../results/scratch/crosscheck.json
    python3 scripts/anvil_crosscheck.py results/scratch/crosscheck.json

Each workload gets a fresh Anvil. Base state is installed with the anvil_set*
methods, transactions are sent in block order from auto-impersonated senders
with mining driven explicitly — one transaction per block, which fixes the order exactly;
none of the workload contracts reads the block number or timestamp — and then
every account and storage slot the workload could have touched is compared.

Standard library only.
"""
import json
import os
import pathlib
import shutil
import socket
import subprocess
import sys
import time
import urllib.request

ZERO_WORD = "0x" + "0" * 64


def anvil_binary():
    found = shutil.which("anvil") or str(pathlib.Path.home() / ".foundry" / "bin" / "anvil")
    if not os.path.exists(found):
        sys.exit("anvil not found on PATH or in ~/.foundry/bin")
    return found


def free_port():
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


class Rpc:
    def __init__(self, port):
        self.url = f"http://127.0.0.1:{port}"
        self.next_id = 0

    def _post(self, payload):
        req = urllib.request.Request(self.url, json.dumps(payload).encode(), {"Content-Type": "application/json"})
        with urllib.request.urlopen(req, timeout=60) as r:
            return json.loads(r.read())

    def call(self, method, *params):
        self.next_id += 1
        out = self._post({"jsonrpc": "2.0", "id": self.next_id, "method": method, "params": list(params)})
        if "error" in out:
            raise RuntimeError(f"{method}: {out['error']}")
        return out["result"]

    def batch(self, calls, chunk=500):
        results = []
        for start in range(0, len(calls), chunk):
            payload = []
            for method, params in calls[start:start + chunk]:
                self.next_id += 1
                payload.append({"jsonrpc": "2.0", "id": self.next_id, "method": method, "params": params})
            reply = {r["id"]: r for r in self._post(payload)}
            for p in payload:
                r = reply[p["id"]]
                if "error" in r:
                    raise RuntimeError(f"{p['method']}: {r['error']}")
                results.append(r["result"])
        return results


def word(hex_value):
    return "0x" + hex_value[2:].rjust(64, "0")


def start_anvil():
    port = free_port()
    proc = subprocess.Popen(
        [anvil_binary(), "--port", str(port), "--hardfork", "osaka",
         "--block-base-fee-per-gas", "0", "--gas-price", "0", "--gas-limit", "100000000",
         "--auto-impersonate", "--no-mining", "--silent"],
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    rpc = Rpc(port)
    for _ in range(200):
        try:
            rpc.call("eth_chainId")
            return proc, rpc
        except OSError:
            time.sleep(0.05)
    proc.kill()
    sys.exit("anvil did not start")


def check(workload, beneficiary, rpc):
    problems = []
    rpc.call("anvil_setCoinbase", beneficiary)

    setup = []
    for a in workload["accounts"]:
        setup.append(("anvil_setBalance", [a["address"], a["balance"]]))
        setup.append(("anvil_setNonce", [a["address"], hex(a["nonce"])]))
        if a["code"] != "0x":
            setup.append(("anvil_setCode", [a["address"], a["code"]]))
    for s in workload["storage"]:
        setup.append(("anvil_setStorageAt", [s["address"], word(s["slot"]), word(s["value"])]))
    rpc.batch(setup)

    for i, tx in enumerate(workload["txs"]):
        sent = {k: tx[k] for k in ("from", "to", "value", "data", "gas", "gasPrice", "nonce")}
        tx_hash = rpc.call("eth_sendTransaction", sent)
        # Automine is asynchronous; mining explicitly is synchronous, so each
        # transaction is in its own block, in order, before the next is sent.
        rpc.call("evm_mine")
        receipt = rpc.call("eth_getTransactionReceipt", tx_hash)
        if receipt is None:
            problems.append(f"tx {i}: not mined")
            continue
        succeeded = receipt["status"] == "0x1"
        if succeeded != tx["success"]:
            problems.append(f"tx {i}: engine success={tx['success']}, anvil success={succeeded}")

    expected = {a["address"].lower(): a for a in workload["expected"]["accounts"]}
    codes = {a["address"].lower(): a["code"].lower() for a in workload["accounts"]}
    addresses = set(expected) | set(codes) | {t["to"].lower() for t in workload["txs"]} | {beneficiary.lower()}
    addresses = sorted(addresses)
    reads = []
    for a in addresses:
        reads += [("eth_getBalance", [a, "latest"]), ("eth_getTransactionCount", [a, "latest"]),
                  ("eth_getCode", [a, "latest"])]
    values = rpc.batch(reads)
    for n, a in enumerate(addresses):
        balance, nonce, code = (int(values[3 * n], 16), int(values[3 * n + 1], 16), values[3 * n + 2].lower())
        want = expected.get(a)
        want_balance, want_nonce = (int(want["balance"], 16), want["nonce"]) if want else (0, 0)
        if (balance, nonce) != (want_balance, want_nonce):
            what = "absent (EIP-161)" if want is None else f"balance {want_balance} nonce {want_nonce}"
            problems.append(f"{a}: engine {what}, anvil balance {balance} nonce {nonce}")
        if code != codes.get(a, "0x"):
            problems.append(f"{a}: code differs")

    want_slots = {(s["address"].lower(), int(s["slot"], 16)): int(s["value"], 16) for s in workload["expected"]["storage"]}
    slots = set(want_slots) | {(s["address"].lower(), int(s["slot"], 16)) for s in workload["storage"]}
    slots = sorted(slots)
    got = rpc.batch([("eth_getStorageAt", [a, word(hex(slot)), "latest"]) for a, slot in slots])
    for (a, slot), value in zip(slots, got):
        if int(value, 16) != want_slots.get((a, slot), 0):
            problems.append(f"{a} slot {hex(slot)}: engine {want_slots.get((a, slot), 0)}, anvil {int(value, 16)}")

    return problems, len(addresses), len(slots)


def main():
    path = sys.argv[1] if len(sys.argv) > 1 else "results/scratch/crosscheck.json"
    data = json.load(open(path))
    anvil_version = subprocess.run([anvil_binary(), "--version"], capture_output=True, text=True).stdout.split("\n")[0]
    lines = [f"# Anvil cross-validation (M1 gate)", f"# {anvil_version}; hardfork osaka; one transaction per block", ""]
    failed = 0
    for w in data["workloads"]:
        proc, rpc = start_anvil()
        try:
            problems, n_accounts, n_slots = check(w, data["beneficiary"], rpc)
        finally:
            proc.terminate()
            proc.wait()
        status = "PASS" if not problems else "FAIL"
        failed += bool(problems)
        reverted = sum(not t["success"] for t in w["txs"])
        line = (f"{status}  {w['name']:<24} {len(w['txs']):>4} txs ({reverted} reverted)  "
                f"{n_accounts:>4} accounts  {n_slots:>4} slots compared")
        print(line)
        lines.append(line)
        for p in problems[:10]:
            print("      " + p)
            lines.append("      " + p)
    summary = f"\n{len(data['workloads']) - failed}/{len(data['workloads'])} workloads agree with Anvil"
    print(summary)
    lines.append(summary)
    pathlib.Path("results").mkdir(exist_ok=True)
    pathlib.Path("results/anvil_crosscheck.txt").write_text("\n".join(lines) + "\n")
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
