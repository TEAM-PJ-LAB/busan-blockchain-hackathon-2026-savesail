"""PRD R4: does refusing to reveal (or forcing a violation) ever pay?  -> results/r4.json, r4.md

For a 10-year contract whose first year is the 240-group campaign (4 periods x 60 groups), a party that
forecasts the sail effect picks periods and groups and refuses to reveal there. Refusals above the cap make
the period a violation (payer: the period's fuel is fixed at max(upper bound, reference rate); investor
representative: the period's normal fuel is worth 0, PRD C14). The verifier's cumulative estimate then
drops the whole violation period ("exclude_period", the v0.7 rule), only the withheld groups ("exclude_groups"),
keeps every commanded-ON hour and calibrates with valid OFF blocks only ("include_on"), calibrates withheld groups
with their neighbours' OFF ratio ("neighbor"), or counts withheld groups at the refuser's worst case ("impute":
payer 100% saving, investor representative 0%). PRD C24 adopts "neighbor" within the cap and "impute" over it;
with 60 groups a period every refusal here is over the 0.5% cap. The contract's total entitlement uses the final true-up bound after 10 years
(9 more years of 900-hour honest campaigns), computed with the reference implementation's settlement rules.

Gain = entitlement change in the attacker's favour as a share of the honest 10-year entitlement, before the
bond slash (so the real gain is lower). PRD R4 requires gain < 0 for every refusal rate and forecast noise.
Also reports PRD M6 (1)(2): estimate bias when payer / investor representative / verifier withhold up to the
cap (0.5% of groups) with forecast noise 0.1 and 0.5, over the A grid conditions with an effect.
"""
import json
import os
import sys
from multiprocessing import Pool

import numpy as np

import grids
import measure as m

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "offchain", "reference"))
import savesail_ref as ref  # noqa: E402

RESULTS = os.environ.get("SAVESAIL_RESULTS", os.path.join(os.path.dirname(os.path.abspath(__file__)), "results"))
GPP, PERIODS, PRICE = 60, 4, 750_000_000
FUTURE_H = 9 * 900  # years 2-10 confirmation campaigns
CLIMATES = ("trade", "n_atlantic", "monsoon")
KS, JS, NOISES = (1, 3, 10, 30, 60), (1, 2), (0.1, 0.5)
# v0.7: drop violation periods and withheld groups; candidates; PRD C24 = "impute" over the cap
RULES = ("exclude_period", "exclude_groups", "include_on", "neighbor", "impute")
IMPUTE = {"payer": 1.0, "investor": 0.0}  # "impute": a refused group counts as the refuser's worst case


def campaign(c, seed):
    """Per-group sums for the honest data plus everything the strategies need."""
    st = m.prepare(c, seed)
    m.evaluate(st)  # caches the baseline predictions st["P0"]
    camp, grp, ng, off, tw = st["camp"], st["grp"], st["ng"], st["is_off"], st["tw"]
    obs = st["obs"][camp]
    ell = st["ell"][camp]
    s_h = st["s_h"]

    def sums(withheld):
        on_cmd = ~off | withheld[grp]  # withheld groups keep the sail on in every block
        ell_a = ell * (1 - s_h * on_cmd)
        act = m.fuel_of(c, st, ell_a, camp)
        meas = act * st["m_fuel"][camp]
        fs = m.shaft_fuel(ell_a * st["mcr"] * st["m_q"][camp], st["mcr"], st["bq"])
        P = st["P0"]
        w_on, w_off = on_cmd * tw * obs, (off & ~withheld[grp]) * tw * obs  # withheld groups: every block is ON
        S = np.column_stack([np.bincount(grp, fs * w_on, ng), np.bincount(grp, P * w_on, ng),
                             np.bincount(grp, fs * w_off, ng), np.bincount(grp, P * w_off, ng)])
        kg_on = np.bincount(grp, meas * on_cmd * obs, ng)  # metered ON fuel per group
        return S, kg_on

    nos = m.fuel_of(c, st, ell, camp)
    on_all = (~off) * tw * obs
    truth = float((nos * s_h * on_all).sum() / (nos * on_all).sum())
    fc_raw = np.bincount(grp, st["raw"], ng) / np.bincount(grp, minlength=ng)
    return dict(st=st, sums=sums, truth=truth, fc_raw=fc_raw, ng=ng)


def bound(S, eps, rows):
    s = S[rows]
    s = s[(s[:, 1] > 0) | (s[:, 3] > 0)]
    paired = s[s[:, 3] > 0]
    if not len(paired):
        return 0.0, 0.0, 0.0, 0
    r = float(m.ratio(s.sum(0)))
    lo, hi = m.interval_from_eps(paired, eps)  # error scale from the groups with an OFF block
    r_p = float(m.ratio(paired.sum(0)))
    return r, float(lo + r - r_p), float(hi + r - r_p), len(paired)


def entitlement(cw, who, periods, k, noise, rule, rng):
    """Total 10-year entitlement (micro-dUSD) under one strategy; who=None is the honest run."""
    ng, st = cw["ng"], cw["st"]
    withheld = np.zeros(ng, bool)
    targets = []
    if who:
        f = cw["fc_raw"] * np.exp(rng.normal(0, noise, ng))
        per = f[: PERIODS * GPP].reshape(PERIODS, GPP).mean(1)
        order = np.argsort(-per) if who == "payer" else np.argsort(per)
        targets = sorted(order[:periods].tolist())
        for p in targets:
            g = np.arange(p * GPP, (p + 1) * GPP)
            pick = g[np.argsort(-f[g])[:k]] if who == "payer" else g[np.argsort(f[g])[:k]]
            withheld[pick] = True
    S, kg = cw["sums"](withheld)
    d = ref.Deal(phi_bps=7850, ref_rate_bps=600, comp_min_bps=9500, miss_max_bps=50, opex=0, senior_sched=[0] * 40,
                 reserve_target=0, senior_units=1, junior_units=1)
    excluded = np.zeros(ng, bool)
    last = None
    for p in range(PERIODS):
        upto = np.arange((p + 1) * GPP)
        keep = ~excluded[upto] if rule == "include_on" else ~withheld[upto] & ~excluded[upto]
        rows = upto[keep]
        r, lb, ub, n = bound(S, st["eps"], rows)
        if rule == "neighbor" and withheld[upto].any() and n:
            Sn = np.vstack([S[rows], m.neighbor_rows(S[upto], ~withheld[upto], withheld[upto])])
            r = float(m.ratio(Sn.sum(0)))
            lb, ub = (float(x) for x in m.interval_from_eps(Sn, st["eps"], n))
        ref_rows = upto[withheld[upto]]
        if rule == "impute" and len(ref_rows) and n:
            pv, pr = S[rows, 1].sum(), S[ref_rows, 1].sum()
            mix = lambda x: (pv * x + pr * IMPUTE[who]) / (pv + pr)  # noqa: E731
            r, lb, ub = mix(r), mix(lb), mix(ub)
        g = np.arange(p * GPP, (p + 1) * GPP)
        miss = int(withheld[g].sum())
        kg_on = float(kg[g][~withheld[g]].sum())
        kg_miss = float(kg[g][withheld[g]].sum())
        bps = lambda x: int(np.clip(round(x * 1e4), -9999, 4999))  # noqa: E731
        a = dict(lb=min(bps(lb), bps(r)), est=bps(r), ub=max(bps(ub), bps(r)), kg_on=int(kg_on), price=PRICE, comp=10_000,
                 flags=4 if n * 15 < 1800 else 0, override=0,
                 kg_miss_payer=int(kg_miss) if who == "payer" else 0, kg_miss_inv=int(kg_miss) if who == "investor" else 0)
        d.finalize(a, miss if who == "payer" else 0, miss if who == "investor" else 0, GPP)
        if miss and rule == "exclude_period":  # v0.7: a violation period leaves the cumulative estimate
            excluded[g] = True
        last = (r, lb, n + (len(ref_rows) if rule == "impute" and n else 0))  # imputed groups keep their weight
    # years 2-10: honest 900-hour campaigns; the true-up re-rates all normal fuel at the final bound
    r1, lb1, n1 = last if last[2] else (cw["truth"], cw["truth"], 0)
    h1 = n1 * 15
    est = (h1 * r1 + FUTURE_H * cw["truth"]) / (h1 + FUTURE_H)
    lb10 = est - (r1 - lb1) * np.sqrt(h1 / (h1 + FUTURE_H))
    w_year = ref.value_micro(int(float(kg.sum())), PRICE)
    w_norm = d.w_norm + 9 * w_year
    return (ref.g_num(w_norm, int(round(lb10 * 1e4))) + d.sv_fixed) * 7850 // 10_000


def r4_seed(a):
    clim, seed = a
    c = dict(m.BASE, ship="bulk", clim=clim, s=0.07, hetero=True)
    cw = campaign(c, seed)
    rng = np.random.default_rng(seed + 99)
    base = entitlement(cw, None, 0, 0, 0, "impute", rng)
    out = {}
    for who in ("payer", "investor"):
        for j in JS:
            for k in KS:
                for noise in NOISES:
                    for rule in RULES:
                        e = entitlement(cw, who, j, k, noise, rule, np.random.default_rng(seed + 7))
                        gain = (base - e) if who == "payer" else (e - base)
                        out[f"{clim}|{who}|{j}|{k}|{noise}|{rule}"] = gain / base
    return out


def m6_seed(a):
    cond, seed = a
    st = m.prepare(dict(m.BASE, **cond), seed)
    h = m.evaluate(st)
    out = {"honest": h["r"] - h["truth"]}
    for who in ("payer", "investor", "verifier"):
        for noise in NOISES:
            o = m.evaluate(st, dict(att_withhold=0.005, att_who=who, fc_noise=noise))
            out[f"{who}|{noise}"] = o["r"] - o["truth"]
    return out


def main(seeds=200, m6_reps=1000):
    with Pool(os.cpu_count() or 1) as pool:
        rows = pool.map(r4_seed, [(cl, 8_000_000 + 1000 * i + s) for i, cl in enumerate(CLIMATES) for s in range(seeds)])
        agg = {}
        for r in rows:
            for k, v in r.items():
                agg.setdefault(k, []).append(v)
        r4 = {k: dict(mean=float(np.mean(v)), p95=float(np.percentile(v, 95)), n=len(v)) for k, v in agg.items()}
        conds = [c for c in grids.grid_A() if c["s"] > 0]
        m6 = {}
        for ci, cond in enumerate(conds):
            res = pool.map(m6_seed, [(cond, 9_000_000 + 10_000 * ci + s) for s in range(m6_reps)])
            m6[ci] = dict(cfg=cond, **{k: float(np.mean([x[k] for x in res])) * 100 for k in res[0]})
    worst = {rule: max(v["mean"] for k, v in r4.items() if k.endswith(rule)) for rule in RULES}
    m6_worst = {who: max(abs(c[f"{who}|{n}"]) for c in m6.values() for n in NOISES)
                for who in ("payer", "investor", "verifier")}
    out = dict(seeds=seeds, m6_reps=m6_reps, r4=r4, worst_mean_gain=worst, m6=m6, m6_worst_bias=m6_worst,
               ok=dict(r4=worst["impute"] < 0, m6=max(m6_worst.values()) <= 0.10))
    json.dump(out, open(os.path.join(RESULTS, "r4.json"), "w"), indent=1)
    lines = ["| 규칙 | 거부로 얻는 최대 평균 이득(10년 정산액 대비, 보증금 차감 전) |", "|---|---|"]
    lines += [f"| {rule} | {100 * v:+.3f}% |" for rule, v in worst.items()]
    lines += ["", "| 상한(0.5%)까지 골라 거부한 쪽 | 편향 최대(%p, M6 ①②) |", "|---|---|"]
    lines += [f"| {who} | {v:.3f} |" for who, v in m6_worst.items()]
    open(os.path.join(RESULTS, "r4.md"), "w").write("\n".join(lines) + "\n")
    print("\n".join(lines))


if __name__ == "__main__":
    main(*(int(x) for x in sys.argv[1:3]))
