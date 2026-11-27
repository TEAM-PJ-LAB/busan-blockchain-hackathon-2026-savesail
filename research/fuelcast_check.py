"""Real-ship check of the measurement method on public operational data (FuelCast).

No public data set has a ship switching a sail on and off (full-scale WAPS data stay with owners and makers), so
this check takes a real ship WITHOUT a sail and keeps everything real except the sail: the ship's own hourly
propeller shaft power, speed, wind and waves carry the real noise, fouling and drift, and a known sail effect is
written into the ON blocks of each random schedule. The estimator, interval and before/after comparator are the
ones in measure.py, unchanged.

Design (fixed before the first run):
- Data: FuelCast, Krohne Digital (huggingface.co/datasets/krohnedigital/FuelCast, CC BY-NC-ND 4.0). The files
  are NOT in this repository; pass their folder as the first argument. Only summary numbers are written.
- Hours: 12 five-minute rows each; a sailing hour has all 12 rows of speed, shaft power, wind and waves and a
  speed of at least 6 kn in every row. Blocks count sailing hours (README 4.2).
- Shaft power: where a ship has port and starboard propeller meters, both must be present in all 12 rows and
  Y is their sum. (Added after the first run: the data's total column adds a missing meter as 0, so hours
  with the starboard meter down showed about half the power. A missing meter is missing data, C16.)
- Ships: those with at least 3,000 sailing hours (1,800-hour campaign after at least 1,200 hours of training).
  CPS_Poseidon qualifies; CPS_Triton and OSS_Ceto do not (counts are written to the result). The first version
  asked for 1,800 training hours; the meter fix below left Poseidon 3,521 hours, so the floor was lowered to
  1,200 before any result on the corrected data was produced.
- Inputs: V = speed through water, D = 1 (no draft in the data), headwind = 10 m wind x cos(wind-from minus
  heading), Hs = wave height, Y = propeller shaft power (the torque-meter quantity the settlement uses, C13).
- Campaign: 1,800 sailing hours (120 groups) starting at three t0 evenly spaced from 1,200 to (sailing hours -
  1,800), rounded down to whole groups; training = every sailing hour before t0. The required range asks for 3,600 training hours, so this is a check outside
  the range on the training side.
- Effect: 0, 3% and 7%, wind-linked as in measure.py (hetero, capped at 45%), applied to ON hours only.
- 2,000 random schedules per condition; placebo calibration and noise rescaling as in BASE (pcf, K=5, B=1000).
- Criteria as PRD 1.3: M1 |bias| <= 0.10%p; M3 (effect 0, lower bound > 0) and M4 (effect > 0, lower bound <=
  truth) by the one-sided binomial test at 0.1%; M10 when before/after bias >= 0.5%p.

Usage: python3 research/fuelcast_check.py <folder with the .parquet files> [--reps N]
"""
import json
import os
import sys

import numpy as np

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import measure as M  # noqa: E402
from judge import too_high, too_low  # noqa: E402

SHIPS = {  # file -> (speed column, power column)
    "CPS_Poseidon": ("Ship_SpeedThroughWater", "Propeller_Total_ShaftPower"),
    "CPS_Triton": ("Ship_SpeedOverGround", "Propeller_Total_ShaftPower"),  # its speed log is stuck at one value
    "OSS_Ceto": ("Ship_SpeedOverGround", "Consumer_Total_ShaftPower"),  # no propeller meter
}
MIN_KN, CAMP_H, MIN_TRAIN = 6.0, 1800, 1200
MIN_HOURS = CAMP_H + MIN_TRAIN
N_STARTS = 3
EFFECTS = (0.0, 0.03, 0.07)
L, G, TRANS = 3, 5, 0.25
SEED = 20261128


def hourly(path, speed, power):
    import pandas as pd
    d = pd.read_parquet(path)
    head = d["Ship_Heading"] if "Ship_Heading" in d else d["Ship_Bearing"]
    pp = ("Propeller_Port_ShaftPower", "Propeller_Starboard_ShaftPower")
    if power == "Propeller_Total_ShaftPower" and all(c in d for c in pp):
        pw = d[pp[0]] + d[pp[1]]  # NaN when either meter is missing (the data's total column fills it with 0)
    else:
        pw = d[power]
    cols = dict(v=d[speed], p=pw, u=d["Weather_WindSpeed10M"], wd=d["Weather_WindDirection10M"], hd=head,
                hs=d["Weather_WaveHeight"])
    x = pd.DataFrame({k: c.to_numpy(float) for k, c in cols.items()})
    x["h"] = np.arange(len(x)) // 12
    full = x.drop(columns="h").notna().all(axis=1)
    g = x.assign(ok=full).groupby("h")
    a = g.agg(n=("ok", "sum"), vmin=("v", "min"), v=("v", "mean"), p=("p", "mean"), u=("u", "mean"), hs=("hs", "mean"))
    # mean relative angle per hour from the five-minute angles (circular mean)
    rel = np.radians(x["wd"] - x["hd"])
    a["c"] = pd.Series(np.cos(rel)).groupby(x["h"]).mean()
    a["s"] = pd.Series(np.sin(rel)).groupby(x["h"]).mean()
    keep = (a["n"] == 12) & (a["vmin"] >= MIN_KN) & (a["p"] > 0)
    return a[keep].reset_index(drop=True), int(len(a))


def run_ship(a, reps, seed):
    T = len(a)
    V, Y, U, Hs = a["v"].to_numpy(), a["p"].to_numpy(), a["u"].to_numpy(), a["hs"].to_numpy()
    th = np.degrees(np.abs(np.arctan2(a["s"].to_numpy(), a["c"].to_numpy())))  # 0 = wind from ahead
    Uh = U * np.cos(np.radians(th))
    D = np.ones(T)
    raw_all = np.clip((U / 8.0) ** 2, 0, 3) * np.where(th > 20, np.sin(np.radians(th)) ** 2, 0.0)
    out = []
    starts = [int(x) // (L * G) * (L * G) for x in np.linspace(MIN_TRAIN, T - CAMP_H, N_STARTS)]
    for t0 in starts:
        rng = np.random.default_rng(seed + t0)
        ng = CAMP_H // (L * G)
        n = ng * G * L
        camp = np.arange(t0, t0 + n)
        tr = np.arange(t0)
        y = np.full(T, np.nan)
        y[:t0] = np.log(Y[:t0])
        kn, beta = M.fit("spline", V, D, Uh, Hs, y, tr)
        pred_tr = M.predict("spline", kn, beta, V[:t0], D[:t0], Uh[:t0], Hs[:t0])
        # cross-fitted placebo on the training hours (as measure.prepare, ci = pcf)
        K, gl = 5, L * G
        fold = np.minimum((np.arange(t0) // gl) * K // max(t0 // gl, 1), K - 1)
        pcf = np.empty(t0)
        for k in range(K):
            ev, fi = np.flatnonzero(fold == k), np.flatnonzero(fold != k)
            knk, bk = M.fit("spline", V, D, Uh, Hs, y, fi)
            pcf[ev] = M.predict("spline", knk, bk, V[ev], D[ev], Uh[ev], Hs[ev])
        twt = np.where(np.arange(t0) % L == 0, 1.0 - TRANS, 1.0)
        eps = M.placebo_eps(rng, Y[:t0], pcf, twt, L, G, 1000, ng)
        res = np.log(Y[:t0] / pcf)  # out-of-fold hourly residuals: the real ship's unexplained variation
        P = M.predict("spline", kn, beta, V[camp], D[camp], Uh[camp], Hs[camp])
        grp = np.repeat(np.arange(ng), G * L)
        blk = np.tile(np.repeat(np.arange(G), L), ng)
        tw = np.where(np.tile(np.arange(L), ng * G) == 0, 1.0 - TRANS, 1.0)
        raw = raw_all[camp]
        Yc = Y[camp]
        pre = np.arange(int(t0 * 0.75), t0)
        for s in EFFECTS:
            s_h = np.minimum(s * raw / max(raw.mean(), 1e-9), 0.45)
            err, lb_pos, cover, pay = [], 0, 0, []
            for _ in range(reps):
                off = blk == rng.integers(0, G, size=ng)[grp]
                F = Yc * (1 - s_h * ~off)
                w_on, w_off = ~off * tw, off * tw
                S = np.column_stack([np.bincount(grp, F * w_on, ng), np.bincount(grp, P * w_on, ng),
                                     np.bincount(grp, F * w_off, ng), np.bincount(grp, P * w_off, ng)])
                r = float(M.ratio(S.sum(0)))
                q_c = M.off_pair_q(np.log(S[:, 2] / S[:, 3]))
                lb, _ = M.interval_from_eps(S, eps, ng, q_c)
                truth = float((Yc * s_h * w_on).sum() / (Yc * w_on).sum())
                err.append((r - truth) * 100)
                lb_pos += int(lb > 0)
                cover += int(lb <= truth)
                if s > 0:
                    pay.append(max(lb, 0.0) / truth)
            # before/after comparator: the sail ON in every campaign hour, compared with the last quarter of training
            r_ba = 1 - ((Yc * (1 - s_h)).sum() / P.sum()) / (Y[pre].sum() / pred_tr[pre - 0].sum())
            truth_ba = float((Yc * s_h).sum() / Yc.sum())
            bias = float(np.mean(err))
            row = dict(t0=t0, train_h=t0, camp_h=n, groups=ng, s=s, reps=reps, bias=round(bias, 4),
                       p95=round(float(np.percentile(np.abs(err), 95)), 4), ba_bias=round((r_ba - truth_ba) * 100, 4))
            ck = dict(M1=bool(abs(bias) <= 0.10))
            if s == 0:
                row["lb_pos"] = round(lb_pos / reps, 4)
                ck["M3"] = bool(not too_high(lb_pos, reps, 0.025))
            else:
                row["cover"] = round(cover / reps, 4)
                row["pay_ratio"] = round(float(np.mean(pay)), 4)
                ck["M4"] = bool(not too_low(cover, reps, 0.975))
            if abs(row["ba_bias"]) >= 0.5:
                ck["M10"] = bool(abs(bias) <= abs(row["ba_bias"]) / 5)
            row["checks"] = ck
            out.append(row)
        if t0 == starts[-1]:
            r1 = float(np.corrcoef(res[:-1], res[1:])[0, 1])
            noise = dict(train_h=t0, resid_sd=round(float(np.std(res)), 4), resid_rho1=round(r1, 4),
                         r2=round(1 - float(np.var(res)) / float(np.var(np.log(Y[:t0]))), 4))
    return out, noise


def main():
    if len(sys.argv) < 2:
        sys.exit(__doc__)
    folder = sys.argv[1]
    reps = int(sys.argv[sys.argv.index("--reps") + 1]) if "--reps" in sys.argv else 2000
    result = dict(source="FuelCast, Krohne Digital, huggingface.co/datasets/krohnedigital/FuelCast (CC BY-NC-ND 4.0)",
                  design=dict(min_kn=MIN_KN, camp_h=CAMP_H, min_train=MIN_TRAIN, min_hours=MIN_HOURS, n_starts=N_STARTS, effects=EFFECTS,
                              block_h=L, group=G, reps=reps, seed=SEED), ships={})
    for name, (spd, pw) in SHIPS.items():
        a, total = hourly(os.path.join(folder, f"{name}.parquet"), spd, pw)
        ship = dict(hours=total, sailing_hours=int(len(a)), included=len(a) >= MIN_HOURS)
        if ship["included"]:
            ship["rows"], ship["noise"] = run_ship(a, reps, SEED)
        result["ships"][name] = ship
    rows = [r for s in result["ships"].values() for r in s.get("rows", [])]
    result["summary"] = dict(conditions=len(rows), failed=[f"{r['t0']}/{r['s']}:{k}" for r in rows
                                                            for k, v in r["checks"].items() if not v])
    if rows:
        ba = [abs(r["ba_bias"]) for r in rows]
        result["summary"].update(
            max_abs_bias=max(abs(r["bias"]) for r in rows), max_lb_pos=max(r["lb_pos"] for r in rows if r["s"] == 0),
            min_cover=min(r["cover"] for r in rows if r["s"] > 0), ba_abs_min=min(ba), ba_abs_max=max(ba),
            ratio_min=round(min(ba) / max(abs(r["bias"]) for r in rows), 1),
            pay_3=[min(r["pay_ratio"] for r in rows if r["s"] == 0.03), max(r["pay_ratio"] for r in rows if r["s"] == 0.03)],
            pay_7=[min(r["pay_ratio"] for r in rows if r["s"] == 0.07), max(r["pay_ratio"] for r in rows if r["s"] == 0.07)])
    path = os.path.join(os.path.dirname(os.path.abspath(__file__)), "results", "fuelcast.json")
    with open(path, "w") as f:
        json.dump(result, f, indent=1, sort_keys=True)
        f.write("\n")
    print(json.dumps(result["summary"]), json.dumps({k: {kk: vv for kk, vv in v.items() if kk != "rows"}
                                                     for k, v in result["ships"].items()}))
    for r in rows:
        print(r)


if __name__ == "__main__":
    main()
