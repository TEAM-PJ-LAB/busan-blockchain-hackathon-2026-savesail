"""Payout ratio h(H) = E[max(lb, 0) / truth] vs cumulative randomized hours, demo condition (PRD F1):
bulk carrier (VLOC), trade winds, wind-linked effect 7%, adopted method (R8). Feeds econ.py (R1)."""
import json
import os
import sys
from multiprocessing import Pool

import numpy as np

import measure as m

RESULTS = os.environ.get("SAVESAIL_RESULTS", "results")  # run_all.py --quick uses a separate directory

COND = dict(ship="bulk", clim="trade", s=0.07, hetero=True)
HOURS = [900, 1800, 3600, 5400, 7200, 10800]


def job(a):
    H, seeds = a
    c = dict(m.BASE, camp_h=H, ba=False, det=False, **COND)
    return [m.run_one(c, s) for s in seeds]


def main(reps=2000):
    out = {"cond": COND, "H": HOURS, "mean": [], "sd": [], "p05": [], "n": reps}
    with Pool(2) as pool:
        for H in HOURS:
            seeds = [list(range(5_000_000 + H * 10 + i, 5_000_000 + H * 10 + reps, 2)) for i in range(2)]
            rows = sum(pool.map(job, [(H, s) for s in seeds]), [])
            h = np.array([max(r["lb"], 0) / r["truth"] for r in rows])
            out["mean"].append(float(h.mean()))
            out["sd"].append(float(h.std(ddof=1)))
            out["p05"].append(float(np.percentile(h, 5)))
            print(H, round(h.mean(), 4), round(h.std(), 4), flush=True)
    json.dump(out, open(f"{RESULTS}/payout_table.json", "w"), indent=1)


if __name__ == "__main__":
    main(int(sys.argv[1]) if len(sys.argv) > 1 else 2000)
