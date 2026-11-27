"""PRD 10: a grid condition that fails a criterion is re-run with new seeds (10,000 honest runs) to tell chance from a
method failure. Usage: python3 recheck.py GRID COND [REPS] -> results/recheck_<GRID><COND>.json

Seeds: 977,000,000 + 10,000 * cond + rep (disjoint from the grid's own seeds)."""
import json
import os
import sys
from multiprocessing import Pool

import numpy as np

import measure as m

RESULTS = os.environ.get("SAVESAIL_RESULTS", os.path.join(os.path.dirname(__file__), "results"))


def one(args):
    cfg, seed = args
    st = m.prepare(dict(m.BASE, **cfg), seed)
    o = m.evaluate(st)
    return o["truth"], o["lb"]


def main(grid, cond, reps=10000):
    j = json.load(open(os.path.join(RESULTS, "judgement.json")))
    row = next(x for x in j["rows"] if x["grid"] == grid and x["cond"] == cond)
    cfg = row["cfg"]
    base = 977_000_000 + 10_000 * cond
    with Pool() as p:
        out = np.array(p.map(one, [(cfg, base + i) for i in range(reps)], chunksize=50))
    t, lb = out.T
    res = dict(grid=grid, cond=cond, cfg=cfg, reps=reps, seed_base=base)
    if cfg["s"] > 0:
        hits = int((lb <= t).sum())
        res.update(M4=dict(rate=hits / reps, hits=hits, grid_rate=row.get("cover"),
                           passes=bool(hits / reps >= 0.975 - 3.09 * np.sqrt(0.975 * 0.025 / reps))))
        res["payout_mean"] = float(np.mean(np.maximum(lb, 0) / t))
    else:
        hits = int((lb > 0).sum())
        res.update(M3=dict(rate=hits / reps, hits=hits, grid_rate=row.get("fp"),
                           passes=bool(hits / reps <= 0.025 + 3.09 * np.sqrt(0.975 * 0.025 / reps))))
    path = os.path.join(RESULTS, f"recheck_{grid}{cond}.json")
    json.dump(res, open(path, "w"), indent=1)
    print(json.dumps(res))


if __name__ == "__main__":
    main(sys.argv[1], int(sys.argv[2]), int(sys.argv[3]) if len(sys.argv) > 3 else 10000)
