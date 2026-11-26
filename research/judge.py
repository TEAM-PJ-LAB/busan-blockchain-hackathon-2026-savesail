"""Apply the PRD §1.3 decision rules to results/raw/*.jsonl -> results/judgement.json and .md.

Ratio criteria: one-sided binomial test per condition at alpha = 0.1% plus the pooled value within
2 Monte-Carlo standard errors of the target. Mean criteria: per-condition value directly.
"""
import glob
import json
import os
import math
import sys
from collections import defaultdict

from scipy.stats import binom

RESULTS = os.environ.get("SAVESAIL_RESULTS", "results")  # run_all.py --quick uses a separate directory

ALPHA = 0.001
LOW_NOISE = lambda c: c.get("sd_e", 0.03) <= 0.03 + 1e-12 and c.get("sd_m", 0.02) <= 0.02 + 1e-12


def load(paths):
    agg = {}
    lines = [json.loads(line) for p in paths for line in open(p)]
    lines.sort(key=lambda d: (d["grid"], d["cond"], d["rep0"]))  # float sums independent of completion order
    for d in lines:
        key = (d["grid"], d["cond"])
        a = agg.setdefault(key, dict(cfg=d["cfg"], err=[], lb_pos=0, cover=0, pay=[], det=0, ba=[], n=0,
                                     flags=defaultdict(int), attacks=defaultdict(lambda: defaultdict(float))))
        h = d["honest"]
        a["n"] += d["n"]
        a["err"] += h["err"]
        a["lb_pos"] += sum(h["lb_pos"])
        a["cover"] += sum(h["cover"])
        a["pay"] += [x for x in h["pay"] if x is not None]
        a["det"] += sum(h["det"])
        a["ba"] += h["ba"]
        for k, v in h["flags"].items():
            a["flags"][k] += v
        for k, v in d["attacks"].items():
            for kk, vv in v.items():
                a["attacks"][k][kk] += vv
    return agg


def p95(xs):
    s = sorted(abs(x) for x in xs)
    return s[min(len(s) - 1, int(math.ceil(0.95 * len(s))) - 1)]


def too_high(x, n, p0):  # evidence that the rate exceeds p0
    return binom.sf(x - 1, n, p0) < ALPHA


def too_low(x, n, p0):  # evidence that the rate is below p0
    return binom.cdf(x, n, p0) < ALPHA


def judge(agg):
    rows, fails = [], []
    pool = defaultdict(lambda: [0, 0])  # name -> [hits, n]
    for (g, ci), a in sorted(agg.items()):
        if g == "R8":  # method comparison, reported separately (r8_table); criteria apply to grids A-D
            continue
        c, n = a["cfg"], a["n"]
        h = c.get("camp_h", 3600)
        s = c["s"]
        bias = sum(a["err"]) / n
        r = dict(grid=g, cond=ci, cfg=c, n=n, bias=bias, p95=p95(a["err"]), checks={})
        ck = r["checks"]
        ck["M1"] = abs(bias) <= 0.10
        if LOW_NOISE(c):
            ck["M2"] = r["p95"] <= (0.60 if h >= 3600 else 1.00)
        if s == 0:
            r["fp"] = a["lb_pos"] / n
            ck["M3"] = not too_high(a["lb_pos"], n, 0.025)
            pool["M3"][0] += a["lb_pos"]
            pool["M3"][1] += n
        else:
            r["cover"] = a["cover"] / n
            ck["M4"] = not too_low(a["cover"], n, 0.975)
            pool["M4"][0] += a["cover"]
            pool["M4"][1] += n
            r["payout"] = 100 * sum(a["pay"]) / max(len(a["pay"]), 1)
            if LOW_NOISE(c) and s >= 0.05:
                ck["M5"] = r["payout"] >= (90.0 if h >= 3600 else 85.0)
        # M8-3: joint false-positive rate of all manipulation checks on honest runs
        r["fp_det"] = a["det"] / n
        ck["M8-3"] = not too_high(a["det"], n, 0.01)
        pool["M8-3"][0] += a["det"]
        pool["M8-3"][1] += n
        # M10: randomized design vs before-after comparison
        ba = sum(a["ba"]) / n
        r["ba_bias"] = ba
        if abs(ba) >= 0.5:
            ck["M10"] = abs(bias) <= abs(ba) / 5
        # manipulation panel
        att = a["attacks"]
        worst = {}
        for name, v in att.items():
            na = v["n"]
            if not na:
                continue
            mean_all, mean_und = v["se"] / na, v["su"] / na
            if name == "storm":
                ck["M6-3"] = abs(mean_all) <= 0.10
                worst["storm"] = mean_all
            elif name.startswith("offon"):
                ck.setdefault("M7", True)
                ck["M7"] &= abs(mean_all) <= 0.10  # violations are excluded via the controller log
            elif name.startswith("speed_"):
                ck.setdefault("M8-1", True)
                ck["M8-1"] &= not too_low(int(v["det"]), int(na), 0.99)
                pool["M8-1"][0] += int(v["det"])
                pool["M8-1"][1] += int(na)
                ck.setdefault("M8-4", True)
                ck["M8-4"] &= abs(mean_und) <= 0.10
            elif name.startswith("rep_"):
                ck.setdefault("M8-2", True)
                ck["M8-2"] &= abs(mean_und) <= 0.10
                worst["rep"] = max(worst.get("rep", 0), abs(mean_und))
            elif name.startswith(("meter_", "shift_")):
                ck.setdefault("M9-2", True)
                ck["M9-2"] &= abs(mean_und) <= 0.10
                worst["meter"] = max(worst.get("meter", 0), abs(mean_und))
                if abs(float(name.split("_")[1])) >= 0.01:
                    ck.setdefault("M9-3", True)
                    ck["M9-3"] &= not too_low(int(v["det"]), int(na), 0.99)
                    pool["M9-3"][0] += int(v["det"])
                    pool["M9-3"][1] += int(na)
        r["worst_attack"] = worst
        for k, ok in ck.items():
            if not ok:
                fails.append((g, ci, k))
        rows.append(r)
    pooled = {}
    for k, (x, n) in pool.items():
        p = x / n
        se = math.sqrt(p * (1 - p) / n) if n else 0
        tgt = {"M3": 0.025, "M4": 0.975, "M8-3": 0.01, "M8-1": 0.99, "M9-3": 0.99}[k]
        ok = (p <= tgt + 2 * se) if k in ("M3", "M8-3") else (p >= tgt - 2 * se)
        pooled[k] = dict(rate=p, n=n, se=se, target=tgt, ok=ok)
        if not ok:
            fails.append(("pooled", -1, k))
    return rows, pooled, fails


def r8_table(agg):
    """R8: per method, pooled zero-effect false-pay rate, coverage, payout, worst bias and 95th-percentile error."""
    by = defaultdict(lambda: dict(fp=[0, 0], cov=[0, 0], pay=[], bias=0.0, p95=0.0))
    for (g, ci), a in sorted(agg.items()):
        if g != "R8":
            continue
        k = a["cfg"]["method"]
        b, n = by[k], a["n"]
        if a["cfg"]["s"] == 0:
            b["fp"][0] += a["lb_pos"]
            b["fp"][1] += n
        else:
            b["cov"][0] += a["cover"]
            b["cov"][1] += n
            b["pay"].append(100 * sum(a["pay"]) / max(len(a["pay"]), 1))
        b["bias"] = max(b["bias"], abs(sum(a["err"]) / n))
        b["p95"] = max(b["p95"], p95(a["err"]))
    return {k: dict(false_pay=v["fp"][0] / max(v["fp"][1], 1), coverage=v["cov"][0] / max(v["cov"][1], 1),
                    payout_mean=sum(v["pay"]) / max(len(v["pay"]), 1), worst_bias=v["bias"], worst_p95=v["p95"])
            for k, v in by.items()}


def main():
    paths = sys.argv[1:] or glob.glob(f"{RESULTS}/raw/*.jsonl")
    agg = load(paths)
    rows, pooled, fails = judge(agg)
    by = defaultdict(lambda: defaultdict(lambda: [0, 0]))
    for r in rows:
        for k, ok in r["checks"].items():
            by[r["grid"]][k][0] += ok
            by[r["grid"]][k][1] += 1
    r8 = r8_table(agg)
    json.dump(dict(rows=rows, pooled=pooled, fails=fails, r8=r8), open(f"{RESULTS}/judgement.json", "w"), indent=1, default=str)
    crit = ["M1", "M2", "M3", "M4", "M5", "M6-3", "M7", "M8-1", "M8-2", "M8-3", "M8-4", "M9-2", "M9-3", "M10"]
    lines = ["| 격자 | 조건 | " + " | ".join(crit) + " |", "|---|---|" + "---|" * len(crit)]
    for g in sorted(by):
        cells = [f"{by[g][k][0]}/{by[g][k][1]}" if by[g][k][1] else "-" for k in crit]
        lines.append(f"| {g} | {sum(1 for r in rows if r['grid'] == g)} | " + " | ".join(cells) + " |")
    lines.append("")
    lines.append("| 합산 | 값 | 목표 | 2SE | 판정 |")
    lines.append("|---|---|---|---|---|")
    for k, v in pooled.items():
        lines.append(f"| {k} | {100 * v['rate']:.3f}% (n={v['n']:,}) | {100 * v['target']:.1f}% | {100 * 2 * v['se']:.3f}%p | {'통과' if v['ok'] else '실패'} |")
    lines.append("")
    lines.append(f"실패 {len(fails)}건" + ("" if not fails else ": " + ", ".join(f"{g}{c}:{k}" for g, c, k in fails[:60])))
    if r8:
        lines += ["", "R8 방법 비교(정직 실행, 6개 조건 × 효과 0·5%)", "",
                  "| 방법 | 효과 0 지급 | 하한 ≤ 참값 | 지급 비율 평균 | 최대 편향 | 최대 오차 95백분위 |", "|---|---|---|---|---|---|"]
        for k, v in r8.items():
            lines.append(f"| {k} | {100 * v['false_pay']:.2f}% | {100 * v['coverage']:.2f}% | {v['payout_mean']:.1f}% | "
                         f"{v['worst_bias']:.3f}%p | {v['worst_p95']:.3f}%p |")
    open(f"{RESULTS}/judgement.md", "w").write("\n".join(lines) + "\n")
    print("\n".join(lines))
    return 1 if fails else 0


if __name__ == "__main__":
    sys.exit(main())
