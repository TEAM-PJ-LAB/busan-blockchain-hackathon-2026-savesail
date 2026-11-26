"""Run measurement experiments E1-E5 in parallel. Usage: python3 -I run_measure.py E1 [E2 ...]
Writes results/<exp>.jsonl (one line per configuration)."""
import json, sys, time
from multiprocessing import Pool
import numpy as np
from measure_v1 import BASE, CLIM, SHIP, run_one, summarize, summarize_ci

CHUNK = 250


def cfgs_E1():  # design comparison: block length x OFF ratio
    return [dict(L=L, G=G, s=s) for s in (0.0, 0.06) for L in (3, 6, 12, 24) for G in (5, 10, 20)]


def cfgs_E2():  # one factor at a time around BASE, effect 0 and 6%
    sweep = [("ship", [k for k in SHIP if k != BASE["ship"]]), ("clim", [k for k in CLIM if k != BASE["clim"]]),
             ("hetero", [False]), ("rho_e", [0.5, 0.8, 0.95, 0.98, 0.995]), ("sd_e", [0.01, 0.05, 0.08]),
             ("sd_m", [0.005, 0.05]), ("foul", [0.0, 0.08]), ("drydock", [True]),
             ("model", ["no_weather", "speed_only"]), ("camp_h", [900, 1800, 7200]), ("miss", [0.05, 0.15])]
    out = [dict(s=s) for s in (0.0, 0.03, 0.05, 0.06, 0.08, 0.12)]
    for k, vals in sweep:
        out += [dict({k: v}, s=s) for v in vals for s in (0.0, 0.06)]
    return out


def cfgs_E3(n=300, seed=7):  # random stress combinations over every factor
    r = np.random.default_rng(seed)
    pick = lambda xs: xs[r.integers(len(xs))]
    out = []
    for _ in range(n):
        out.append(dict(
            ship=pick(list(SHIP)), clim=pick(list(CLIM)),
            s=0.0 if r.random() < 0.3 else float(np.round(r.uniform(0.02, 0.12), 3)),
            hetero=bool(r.random() < 0.8), rho_e=pick([0.5, 0.8, 0.9, 0.95, 0.98]),
            sd_e=pick([0.01, 0.03, 0.05, 0.08]), sd_m=pick([0.005, 0.02, 0.05]),
            foul=pick([0.0, 0.03, 0.08]), drydock=bool(r.random() < 0.2),
            model=pick(["full", "full", "no_weather", "speed_only"]), camp_h=pick([1800, 3600, 7200]),
            miss=pick([0.0, 0.05, 0.15])))
    return out


def cfgs_E4():  # manipulation attempts (effect 6%)
    out = [dict(att_offon=q, exclude_viol=e) for q in (0.02, 0.05, 0.1, 0.2) for e in (True, False)]
    out += [dict(att_withhold=k, att_who=w, fc_noise=f) for k in (0.01, 0.02, 0.05, 0.1)
            for w in ("payer", "investor") for f in (0.1, 0.5)]
    out += [dict(att_speed=x, model=m) for x in (-0.05, -0.03, 0.03) for m in ("full", "speed_only")]
    out += [dict(att_meter=x) for x in (0.01, 0.02, 0.05)]
    return out


def cfgs_E5():  # precision runs for acceptance criteria
    return [dict(s=s, hetero=h, B=2000) for s in (0.0, 0.03, 0.05, 0.08) for h in (True, False)]


def cfgs_E7():  # CI method comparison (group bootstrap / moving-block / placebo) under stress
    var = [{}, dict(hetero=False), dict(rho_e=0.95), dict(rho_e=0.98), dict(rho_e=0.995), dict(sd_e=0.08),
           dict(sd_m=0.05), dict(camp_h=900), dict(camp_h=1800), dict(camp_h=7200), dict(train_h=1800),
           dict(clim="trade"), dict(clim="calm"), dict(clim="monsoon"), dict(ship="container"), dict(ship="bulk"),
           dict(drydock=True), dict(foul=0.08), dict(model="speed_only"), dict(model="no_weather"),
           dict(miss=0.15), dict(att_withhold=0.05), dict(att_offon=0.05)]
    return [dict(v, s=s, ci_all=True) for v in var for s in (0.0, 0.05, 0.08)]


REPS = {"E1": 2000, "E2": 2000, "E3": 300, "E4": 1000, "E5": 4000, "E7": 2000}


def work(args):
    cfg, seed0, n = args
    return [run_one(cfg, seed0 + i) for i in range(n)]


def main(exps):
    with Pool(2) as pool:
        for ex in exps:
            cfgs = [dict(BASE, **d) for d in globals()[f"cfgs_{ex}"]()]
            reps, t = REPS[ex], time.time()
            jobs, owner = [], []
            for ci, c in enumerate(cfgs):
                for k in range(0, reps, CHUNK):
                    jobs.append((c, 10_000_000 * int(ex[1:]) + ci * 100_000 + k, min(CHUNK, reps - k)))
                    owner.append(ci)
            rows = [[] for _ in cfgs]
            for ci, res in zip(owner, pool.imap(work, jobs, chunksize=1)):
                rows[ci].extend(res)
            with open(f"results/v1/{ex}.jsonl", "w") as f:
                for c, rw in zip(cfgs, rows):
                    sm = summarize_ci(rw, c["s"]) if c.get("ci_all") else {k: float(v) for k, v in summarize(rw, c["s"]).items()}
                    f.write(json.dumps({"cfg": c, **sm}) + "\n")
            print(f"{ex}: {len(cfgs)} configs x {reps} reps in {time.time() - t:.0f}s", flush=True)


if __name__ == "__main__":
    main(sys.argv[1:])
