#!/usr/bin/env python3
"""Checks PRD K2/K3/K6 and the T1/T4/T6 counts from target/tmp/k2_hits.txt.

Run after `rm -f target/tmp/k2_hits.txt && cargo test --test program`.
"""
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]
HITS = ROOT / "target/tmp/k2_hits.txt"
ERRORS = ROOT / "programs/savesail/src/error.rs"
COMMANDS = ["initialize_deal", "init_vaults", "set_kyc", "invest", "close_fundraising", "register_anchor",
            "reveal", "resolve_missing", "commit_group_data", "approve_attestation", "settle", "distribute",
            "claim", "release_bonds", "close_deal"]
CU_MAX = 100_000


def main() -> int:
    names = re.findall(r"^\s+([A-Z][A-Za-z]+),\s*$", ERRORS.read_text(), re.M)
    lines = HITS.read_text().splitlines()
    err_hit = {l.split()[2] for l in lines if l.startswith("ERR ")}
    paths = sum(1 for l in lines if l.split()[0] in ("ERR", "AERR", "FAIL"))
    cu = {}
    for l in lines:
        if l.startswith("OK "):
            _, n, c = l.split()
            cu[n] = max(cu.get(n, 0), int(c))
    unauth = {l.split()[1] for l in lines if l.startswith("UNAUTH ")}
    flows = sum(1 for l in lines if l.startswith("T3ONCHAIN"))

    fails = []
    missing = [n for n in names if n not in err_hit]
    if missing:
        fails.append(f"K2: error codes never hit: {missing}")
    if paths < 60:
        fails.append(f"T1: error paths {paths} < 60")
    nok = [c for c in COMMANDS if c not in cu]
    if nok:
        fails.append(f"T1: commands without a successful run: {nok}")
    over = {c: v for c, v in cu.items() if v > CU_MAX}
    if over:
        fails.append(f"K3: over {CU_MAX} CU: {over}")
    nu = [c for c in COMMANDS if c not in unauth]
    if nu:
        fails.append(f"K6: commands without an unauthorized test: {nu}")
    if flows < 300:
        fails.append(f"T3: on-chain random flows {flows} < 300")

    print(f"error codes hit {len(err_hit & set(names))}/{len(names)}, error paths {paths}, "
          f"commands ok {len(COMMANDS) - len(nok)}/15, unauthorized {len(unauth & set(COMMANDS))}/15, "
          f"on-chain flows {flows}")
    for c in COMMANDS:
        if c in cu:
            print(f"  CU {c:<20} {cu[c]:>7}")
    for f in fails:
        print("FAIL", f)
    return 1 if fails else 0


if __name__ == "__main__":
    sys.exit(main())
