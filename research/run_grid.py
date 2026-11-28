"""Run grid conditions (honest runs + manipulation panel). Sharded for parallel jobs; resumable.

Usage: python3 run_grid.py GRID[@S/N] SHARD NSHARDS [--quick N]
Writes results/raw/<GRID>_<SHARD>.jsonl (with @S/N: <GRID>_SofN_<SHARD>.jsonl), one line per (condition, chunk of seeds).
GRID@S/N re-runs only the chunks of shard S of an N-shard run, split over NSHARDS jobs (same seeds).
"""
import json
import os
import sys
import time
from multiprocessing import Pool

import numpy as np

import measure as m
from grids import ATTACK_REPS, GRIDS, REPS, attack_panel

RESULTS = os.environ.get("SAVESAIL_RESULTS", "results")  # run_all.py --quick uses a separate directory

CHUNK = 250
PANEL = attack_panel()


def chunk_job(args):
    grid, ci, cond, rep0, n, attack_n = args
    c = dict(m.BASE, **{k: v for k, v in cond.items() if k != "method"})
    base = grid[1:] if grid in ("NA", "NB", "NC", "ND") else grid  # newbuild grids reuse the base grid's seeds
    seed_base = (ord(base[0]) - 64 + 10 * (len(base) - 1)) * 100_000_000 + ci * 100_000
    hon = dict(err=[], lb_pos=[], cover=[], pay=[], det=[], ba=[], flags={})
    att = {k: dict(n=0, det=0, se=0.0, se2=0.0, su=0.0) for k in PANEL} if attack_n else {}
    for j in range(rep0, rep0 + n):
        st = m.prepare(c, seed_base + j)
        o = m.evaluate(st)
        e = (o["r"] - o["truth"]) * 100
        hon["err"].append(round(e, 6))
        hon["lb_pos"].append(int(o["lb"] > 0))
        hon["cover"].append(int(o["lb"] <= o["truth"]))
        hon["pay"].append(round(max(o["lb"], 0) / o["truth"], 6) if o["truth"] > 0 else None)
        hon["det"].append(int(o["detected"]))
        hon["ba"].append(round((o["r_ba"] - o["truth_ba"]) * 100, 6))
        for k, v in o["flags"].items():
            hon["flags"][k] = hon["flags"].get(k, 0) + int(v)
        if j - rep0 < attack_n:
            for k, sc in PANEL.items():
                a = m.evaluate(st, sc)
                ea = (a["r"] - a["truth"]) * 100
                d = att[k]
                d["n"] += 1
                d["det"] += int(a["detected"])
                d["se"] += ea
                d["se2"] += ea * ea
                d["su"] += 0.0 if a["detected"] else ea
    return dict(grid=grid, cond=ci, cfg=cond, rep0=rep0, n=n, honest=hon, attacks=att)


def jobs_for(grid, quick=None):
    conds = GRIDS[grid]()
    reps = quick or REPS[grid]
    areps = min(reps, quick or ATTACK_REPS[grid])
    out = []
    for ci, cond in enumerate(conds):
        for rep0 in range(0, reps, CHUNK):
            n = min(CHUNK, reps - rep0)
            an = 0 if cond["s"] == 0 or not areps else max(0, min(n, areps - rep0))
            out.append((grid, ci, cond, rep0, n, an))
    return out


def assign(jobs, nshards):
    """Shard of each job: costliest first, dealt out back and forth over the shards, so every shard gets a similar
    share of the manipulation panels (a plain k % NSHARDS put all panel chunks on a quarter of the shards)."""
    cost = [j[4] + j[5] * len(PANEL) for j in jobs]
    out = [0] * len(jobs)
    for p, i in enumerate(sorted(range(len(jobs)), key=lambda i: -cost[i])):
        r = p % nshards
        out[i] = r if (p // nshards) % 2 == 0 else nshards - 1 - r
    return out


def main():
    spec, shard, nshards = sys.argv[1], int(sys.argv[2]), int(sys.argv[3])
    grid, _, outer = spec.partition("@")  # "NC@0/40": only shard 0 of a 40-shard run, split again over NSHARDS
    quick = int(sys.argv[5]) if len(sys.argv) > 5 and sys.argv[4] == "--quick" else None
    os.makedirs(f"{RESULTS}/raw", exist_ok=True)
    tag = f"{outer.replace('/', 'of')}_{shard}" if outer else f"{shard}"
    path = f"{RESULTS}/raw/{grid}_{tag}.jsonl"
    done = set()
    if os.path.exists(path):
        for line in open(path):
            d = json.loads(line)
            done.add((d["cond"], d["rep0"]))
    jobs = jobs_for(grid, quick)
    if outer:
        o, on = map(int, outer.split("/"))
        jobs = [j for j, a in zip(jobs, assign(jobs, on)) if a == o]
    jobs = [j for j, a in zip(jobs, assign(jobs, nshards)) if a == shard and (j[1], j[3]) not in done]
    t = time.time()
    with Pool(os.cpu_count() or 1) as pool, open(path, "a") as f:
        for i, res in enumerate(pool.imap_unordered(chunk_job, jobs)):
            f.write(json.dumps(res) + "\n")
            f.flush()
            print(f"{grid} shard {shard}: {i + 1}/{len(jobs)} chunks, {time.time() - t:.0f}s", flush=True)


if __name__ == "__main__":
    main()
