"""SaveSail economics Monte Carlo (README §5.2 / PRD R1, R5). Annual steps, vectorized over paths.

settle: "ideal"  -> verified = actual (README 5.2 implicit assumption)
        "period" -> each year settled at its own lower bound (haircut h(hours of that year's data))
        "trueup" -> cumulative lower bound re-rates all past years (PRD C1); overpayment refunded at end from reserve
"""
import json
import os

import numpy as np

BASE = dict(
    C=4_000_000.0, S0=1500.0, perf_sd=0.30, yr_sd=0.10, T=10, post=5, phi=0.75, opex=0.02,
    disc=0.08, senior_rate=0.05, dscr=1.5, p_cons=500.0, price="flat750", settle="trueup",
    camp1_h=3600, campN_h=900, reserve_years=1.0, n=20_000, seed=11,
)

# payout ratio h(H) = E[max(LB,0)/true] and its sd vs cumulative randomized hours H, measured with the
# adopted method at the demo condition (payout_table.py -> results/payout_table.json, PRD F1)
_T = os.path.join(os.path.dirname(os.path.abspath(__file__)), "results", "payout_table.json")
H_TABLE = json.load(open(_T)) if os.path.exists(_T) else {"H": [900, 1800, 3600, 7200],
                                                         "mean": [0.60, 0.72, 0.83, 0.88], "sd": [0.12, 0.08, 0.05, 0.03]}


def h_of(H):
    x = np.log(np.asarray(H_TABLE["H"], float))
    return (np.interp(np.log(H), x, H_TABLE["mean"]), np.interp(np.log(H), x, H_TABLE["sd"]))


def price_paths(kind, n, Y, rng):
    if kind == "flat750":
        return np.full((n, Y), 750.0)
    if kind.startswith("drop"):  # drop600 / drop460 from year 2
        p = np.full((n, Y), 750.0)
        p[:, 1:] = float(kind[4:])
        return p
    if kind == "ou":  # log mean-reverting around 750, annual vol 25%
        x = np.zeros((n, Y))
        e = rng.normal(0, 0.25, (n, Y))
        for y in range(Y):
            x[:, y] = (0.7 * x[:, y - 1] if y else 0) + e[:, y]
        return 750.0 * np.exp(x - 0.25 ** 2 / 2)
    if kind == "spike":  # +60% in years 2-3 then back
        p = np.full((n, Y), 750.0)
        p[:, 1:3] = 1200.0
        return p
    raise ValueError(kind)


def irr(cf):
    """Row-wise IRR by bisection; cf[:, 0] < 0."""
    lo, hi = np.full(cf.shape[0], -0.99), np.full(cf.shape[0], 1.0)
    t = np.arange(cf.shape[1])
    for _ in range(60):
        m = (lo + hi) / 2
        v = (cf / (1 + m[:, None]) ** t).sum(1)
        lo, hi = np.where(v > 0, m, lo), np.where(v > 0, hi, m)
    return (lo + hi) / 2


def annuity_factor(r, T):
    return (1 - (1 + r) ** -T) / r


def simulate(cfg, phi=None, detail=False):
    c = dict(BASE, **cfg)
    phi = c["phi"] if phi is None else phi
    rng = np.random.default_rng(c["seed"])
    n, T, Y = c["n"], c["T"], c["T"] + c["post"]
    perf = np.exp(rng.normal(-c["perf_sd"] ** 2 / 2, c["perf_sd"], n))  # mean-1 performance
    yr = np.exp(rng.normal(-c["yr_sd"] ** 2 / 2, c["yr_sd"], (n, Y)))
    P = price_paths(c["price"], n, Y, rng)
    on = np.full(Y, 1.0)
    on[:T] = 0.95
    on[0] = 0.80
    act = c["S0"] * perf[:, None] * yr * on  # actual saved tonnes
    val = act * P  # actual saved value (USD)

    # verified value per contract year
    if c["settle"] == "ideal":
        hy = np.ones((n, T))
    else:
        Hcum = c["camp1_h"] + c["campN_h"] * np.arange(T)
        Hy = np.where(np.arange(T) == 0, c["camp1_h"], c["campN_h"])
        if c["settle"] == "trueup":
            # cumulative estimates share data: z(H) = W(H)/sqrt(H) for a Brownian W over randomized hours
            inc = rng.standard_normal((n, T)) * np.sqrt(np.diff(np.concatenate([[0], Hcum])))
            z = np.cumsum(inc, 1) / np.sqrt(Hcum)
            m, s = h_of(Hcum)
        else:
            z = rng.standard_normal((n, T))
            m, s = h_of(Hy)
        hy = np.clip(m + s * z, 0, 1.05)
    if c["settle"] == "trueup":
        cumv = np.cumsum(val[:, :T], 1)
        E = phi * cumv * hy  # cumulative entitlement re-rated with latest bound
        paid = np.maximum.accumulate(E, axis=1)  # never pay back during the term
        pay = np.diff(np.concatenate([np.zeros((n, 1)), paid], 1), axis=1)
        overpaid = paid[:, -1] - E[:, -1]
    else:
        pay = phi * val[:, :T] * hy
        overpaid = np.zeros(n)

    # senior sizing: P2 of conservative-price verified cash flow covers debt service x DSCR
    mh, _ = h_of(np.where(np.arange(T) == 0, c["camp1_h"], c["campN_h"])) if c["settle"] != "ideal" else (np.ones(T), 0)
    cons = phi * c["S0"] * perf[:, None] * on[:T] * mh * c["p_cons"] - c["opex"] * c["C"]
    ds = max(np.percentile(cons, 2, axis=0).min() / c["dscr"], 0.0)
    senior = min(ds * annuity_factor(c["senior_rate"], T), c["C"])
    junior0 = c["C"] - senior

    # waterfall
    res, arr = np.zeros(n), np.zeros(n)
    jun = np.zeros((n, T + 1))
    jun[:, 0] = -junior0
    target = c["reserve_years"] * ds
    for y in range(T):
        a = pay[:, y] - c["opex"] * c["C"]  # opex first (shortfall absorbed by later cash)
        need = ds + arr
        sp = np.clip(a, 0, need)
        a = a - sp
        draw = np.minimum(res, need - sp)
        res -= draw
        arr = need - sp - draw
        ri = np.clip(np.minimum(a, target - res), 0, None)
        res += ri
        jun[:, y + 1] = a - ri
    refund = np.minimum(np.maximum(overpaid, 0), res)
    res -= refund
    pay_net = pay.copy()
    pay_net[:, -1] -= refund
    cover = np.minimum(arr, res)
    arr -= cover
    res -= cover
    jun[:, T] += res
    j_irr = irr(jun)

    d = (1 + c["disc"]) ** -np.arange(1, Y + 1)
    # after the term the payer owns the sail and bears its O&M (2% of installation cost a year)
    payer = ((val[:, :T] - pay_net) * d[:T]).sum(1) + ((val[:, T:] - c["opex"] * c["C"]) * d[T:]).sum(1)
    if detail:
        return dict(overpaid_sum=float(np.maximum(overpaid, 0).sum()), refund_sum=float(refund.sum()))
    return dict(
        phi=phi, senior_share=senior / c["C"] * 100, senior_loss=float((arr > 1e-6).mean() * 100),
        junior_irr_med=float(np.median(j_irr) * 100), junior_irr_p10=float(np.percentile(j_irr, 10) * 100),
        payer_npv_mean=float(payer.mean() / 1e4), payer_loss=float((payer < 0).mean() * 100),
        refund_cover=float((refund.sum() / max(overpaid.clip(0).sum(), 1e-9)) * 100) if overpaid.any() else np.nan,
        overpay_prob=float((overpaid > 1e-6).mean() * 100),
    )


def solve_phi(cfg, target=12.0):
    lo, hi = 0.30, 1.50
    for _ in range(30):
        m = (lo + hi) / 2
        lo, hi = (lo, m) if simulate(cfg, m)["junior_irr_med"] >= target else (m, hi)
    return hi
