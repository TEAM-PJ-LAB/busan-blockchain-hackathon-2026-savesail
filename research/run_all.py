"""PRD R7: reproduce every research result with one command.

    python3 research/run_all.py                      # full run: grids R8,A,B,C,D, judgement, payout table, economics
    python3 research/run_all.py --quick 20 --twice   # small run done twice; checks the output files are byte-identical

Every random draw is seeded from (grid, condition, repetition), so the same command gives the same files.
Writes <out>/run_all.json with the elapsed seconds of each step and the sha256 of each output file.
"""
import argparse
import hashlib
import json
import os
import subprocess
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
OUTPUTS = ["judgement.json", "judgement.md", "payout_table.json", "econ.json", "econ.md", "m9_bound.json", "r4.json", "r4.md"]


def sha(path):
    return hashlib.sha256(open(path, "rb").read()).hexdigest() if os.path.exists(path) else None


def run(out, grids, quick):
    os.makedirs(os.path.join(out, "raw"), exist_ok=True)
    env = dict(os.environ, SAVESAIL_RESULTS=out)
    steps = []

    def step(name, args, judged=False):
        t = time.time()
        r = subprocess.run([sys.executable, *args], cwd=HERE, env=env, stdout=subprocess.DEVNULL)
        if r.returncode and not judged:  # judge.py exits 1 when a criterion fails; that is a result, not a crash
            raise SystemExit(f"{name} failed ({r.returncode})")
        steps.append(dict(step=name, seconds=round(time.time() - t, 1), exit=r.returncode))
        print(f"{name}: {steps[-1]['seconds']}s exit {r.returncode}", flush=True)

    for g in grids:
        step(f"grid {g}", ["run_grid.py", g, "0", "1"] + (["--quick", str(quick)] if quick else []))
    step("judge", ["judge.py"], judged=True)
    step("payout table", ["payout_table.py", str(quick or 2000)])
    step("economics", ["run_econ.py"])
    step("meter detection bound", ["m9_bound.py"])
    step("refusal incentives (R4)", ["run_r4.py"] + (["20", str(min(quick, 50))] if quick else []))
    rec = dict(grids=grids, quick=quick, steps=steps, sha256={f: sha(os.path.join(out, f)) for f in OUTPUTS})
    json.dump(rec, open(os.path.join(out, "run_all.json"), "w"), indent=1)
    return rec


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--quick", type=int, default=0, help="repetitions per condition (default: PRD counts)")
    ap.add_argument("--grids", default="R8,A,B,C,D")
    ap.add_argument("--out", default=None, help="results directory (default results, or results/quick with --quick)")
    ap.add_argument("--twice", action="store_true", help="run twice into separate directories and compare sha256")
    a = ap.parse_args()
    grids = a.grids.split(",")
    out = os.path.abspath(a.out or os.path.join(HERE, "results", "quick" if a.quick else ""))
    if not a.twice:
        run(out, grids, a.quick)
        return 0
    r1, r2 = run(os.path.join(out, "run1"), grids, a.quick), run(os.path.join(out, "run2"), grids, a.quick)
    same = r1["sha256"] == r2["sha256"]
    for f in OUTPUTS:
        print(f"{f}: {r1['sha256'][f]} {'==' if r1['sha256'][f] == r2['sha256'][f] else '!='} {r2['sha256'][f]}")
    print("identical" if same else "DIFFERENT")
    return 0 if same else 1


if __name__ == "__main__":
    sys.exit(main())
