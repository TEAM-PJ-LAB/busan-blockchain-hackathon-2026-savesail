"""SaveSail measurement simulator (README §5.1 / PRD R2~R4).

Synthetic hourly voyage data -> pre-committed randomized ON/OFF blocks -> verifier estimate
(ratio-of-ratios, group bootstrap). Ground truth is known exactly per run.
"""
import numpy as np
from scipy.signal import lfilter

# ship: (mean speed kn, laden no-sail fuel at mean speed kg/h)
SHIP = {"bulk": (12.5, 1900.0), "tanker": (13.0, 1700.0), "gas": (16.5, 2600.0), "container": (17.5, 3500.0)}
# climate: wind mean m/s, wind sd, wind hourly rho, rel. angle mean deg, angle sd, angle rho, seasonal amplitude
CLIM = {
    "trade": (8.0, 2.0, 0.97, 95.0, 25.0, 0.995, 0.0),
    "n_atlantic": (9.0, 4.0, 0.95, 90.0, 60.0, 0.98, 0.25),
    "calm": (4.5, 2.0, 0.95, 90.0, 60.0, 0.98, 0.0),
    "monsoon": (7.0, 3.0, 0.96, 90.0, 45.0, 0.99, 0.5),
}
BASE = dict(
    ship="gas", clim="n_atlantic", s=0.06, hetero=True, rho_e=0.9, sd_e=0.03, sd_m=0.02,
    foul=0.03, drydock=False, model="full", camp_h=3600, train_h=3600, L=6, G=5, miss=0.0,
    att_offon=0.0, exclude_viol=True, att_withhold=0.0, att_who="payer", fc_noise=0.3,
    att_speed=0.0, att_meter=0.0, boot="group", mbb=4, B=1000,
)
HPY = 6000.0  # sailing hours per year


def ar1(rng, n, rho, sd, burn=600):
    x = lfilter([1.0], [1.0, -rho], rng.standard_normal(n + burn))[burn:]
    return x * sd * np.sqrt(1.0 - rho * rho)


def generate(c, rng):
    vm, fref = SHIP[c["ship"]]
    um, usd, urho, am, asd, arho, amp = CLIM[c["clim"]]
    T = c["train_h"] + c["camp_h"]
    t0 = c["train_h"]
    # voyages: alternating laden / ballast, 300-700 sailing hours
    lens = rng.integers(300, 701, size=T // 300 + 2)
    vid = np.repeat(np.arange(lens.size), lens)[:T]
    laden = ((vid + rng.integers(2)) % 2 == 0)
    D = np.where(laden, 1.0, 0.62)
    V = vm + rng.normal(0, 0.5, lens.size)[vid] + ar1(rng, T, 0.99, 0.4)
    V = np.clip(V, 10.0, vm + 3.0)
    ty = np.arange(T) / HPY
    U = np.clip(um * (1 + amp * np.sin(2 * np.pi * ty + rng.uniform(0, 6.3))) + ar1(rng, T, urho, usd), 0.3, 30.0)
    th = am + ar1(rng, T, arho, asd)
    th = np.abs(((th + 180.0) % 360.0) - 180.0)
    Hs = np.clip(0.3 + 0.025 * U ** 1.5 + ar1(rng, T, 0.98, 0.15), 0.1, None)
    Uh = U * np.cos(np.radians(th))
    if c["drydock"]:  # hull cleaned at installation; 2 years of fouling before
        foul = np.where(np.arange(T) < t0, 1 + c["foul"] * (2.0 + ty), 1 + c["foul"] * (ty - ty[t0]))
    else:
        foul = 1 + c["foul"] * ty
    return dict(T=T, t0=t0, V=V, D=D, U=U, th=th, Hs=Hs, Uh=Uh, foul=foul, fref=fref, vm=vm)


def nosail_fuel(g, V, rng, c):
    w = 1 + np.maximum(0.0012 * g["Uh"] * np.abs(g["Uh"]), -0.04) + 0.02 * g["Hs"] ** 2
    F0 = g["fref"] * (V / g["vm"]) ** 3 * g["D"] ** (2 / 3) * w * g["foul"]
    return F0 * np.exp(ar1(rng, g["T"], c["rho_e"], c["sd_e"]))


def features(model, V, D, Uh, Hs):
    cols = [np.ones_like(V), np.log(V)]
    if model in ("full", "no_weather"):
        cols.append(np.log(D))
    if model == "full":
        cols += [Uh * np.abs(Uh), Hs ** 2]
    return np.column_stack(cols)


def boot_weights(rng, ng, B, kind, m):
    if kind == "group":
        return rng.multinomial(ng, np.full(ng, 1.0 / ng), size=B).astype(float)
    k = -(-ng // m)  # moving-block bootstrap over consecutive groups
    starts = rng.integers(0, ng - m + 1, size=(B, k))
    idx = (starts[..., None] + np.arange(m)).reshape(B, -1)[:, :ng]
    W = np.zeros((B, ng))
    np.add.at(W, (np.repeat(np.arange(B), ng), idx.ravel()), 1.0)
    return W


def ratio(S):
    """S[..., 4] = (A_on, P_on, A_off, P_off) sums -> savings rate."""
    return 1.0 - (S[..., 0] / S[..., 1]) / (S[..., 2] / S[..., 3])


def run_one(c, seed):
    rng = np.random.default_rng(seed)
    g = generate(c, rng)
    T, t0, L, G = g["T"], g["t0"], c["L"], c["G"]
    ng = (c["camp_h"] // L) // G
    n_used = ng * G * L
    camp = np.arange(t0, t0 + n_used)
    grp = np.repeat(np.arange(ng), G * L)
    blk = np.tile(np.repeat(np.arange(G), L), ng)
    off_blk = rng.integers(0, G, size=ng)
    is_off = blk == off_blk[grp]

    # sail potential (rotor: lift ~ apparent wind^2, best near beam)
    th_c = g["th"][camp]
    raw = np.clip((g["U"][camp] / 8.0) ** 2, 0, 3) * np.where(th_c > 20, np.sin(np.radians(th_c)) ** 2, 0.0)
    s_h = np.minimum(c["s"] * raw / max(raw.mean(), 1e-9), 0.45) if c["hetero"] else np.full(n_used, c["s"])

    # adversarial selective withholding: decided per group from a noisy forecast of sail potential
    withheld = np.zeros(ng, bool)
    if c["att_withhold"] > 0:
        fc = raw * np.exp(rng.normal(0, c["fc_noise"], n_used))
        on_mean = np.bincount(grp, fc * ~is_off, ng) / np.bincount(grp, (~is_off).astype(float), ng)
        off_mean = np.bincount(grp, fc * is_off, ng) / np.bincount(grp, is_off.astype(float), ng)
        score = on_mean - off_mean if c["att_who"] == "payer" else off_mean - on_mean
        withheld = score >= np.quantile(score, 1 - c["att_withhold"])
    off_eff = is_off & ~withheld[grp]  # withheld group -> all blocks ON

    # OFF-block violation (sail kept ON during commanded OFF)
    viol_grp = rng.random(ng) < c["att_offon"]
    sail_on = ~off_eff | (off_eff & viol_grp[grp])

    V = g["V"].copy()
    if c["att_speed"]:
        V[camp[off_eff]] *= 1 + c["att_speed"]
    nos = nosail_fuel(g, V, rng, c)
    act = nos.copy()
    act[camp] = nos[camp] * (1 - s_h * sail_on)
    meas = act * np.exp(rng.normal(0, c["sd_m"], T))
    if c["att_meter"]:
        meas[camp[off_eff]] *= 1 - c["att_meter"]

    # observed covariates and baseline model fit on pre-installation data
    Vo, Uo = V + rng.normal(0, 0.1, T), np.clip(g["U"] + rng.normal(0, 0.5, T), 0, None)
    tho = g["th"] + rng.normal(0, 5, T)
    Uho, Hso = Uo * np.cos(np.radians(tho)), np.clip(g["Hs"] + rng.normal(0, 0.2, T), 0.05, None)
    X = features(c["model"], Vo, g["D"], Uho, Hso)
    obs = rng.random(T) >= c["miss"]
    tr = np.arange(t0)[obs[:t0]]
    beta = np.linalg.lstsq(X[tr], np.log(meas[tr]), rcond=None)[0]
    pred = np.exp(X @ beta)

    # verifier inclusion: not withheld, not violated, observed
    inc = ~withheld[grp] & ~viol_grp[grp] if c["exclude_viol"] else ~withheld[grp]
    inc = inc & obs[camp]
    A, P = meas[camp], pred[camp]
    on, off = inc & ~off_eff, inc & off_eff
    S = np.column_stack([
        np.bincount(grp, A * on, ng), np.bincount(grp, P * on, ng),
        np.bincount(grp, A * off, ng), np.bincount(grp, P * off, ng)])
    S = S[(S[:, 1] > 0) & (S[:, 3] > 0)]
    r_hat = float(ratio(S.sum(0)))
    W = boot_weights(rng, len(S), c["B"], c["boot"], c["mbb"])
    rb = ratio(W @ S)
    lb, ub = np.percentile(rb, [2.5, 97.5])
    extra = ()
    if c.get("ci_all"):
        rm = ratio(boot_weights(rng, len(S), c["B"], "mbb", c["mbb"]) @ S)
        lp, up = placebo_ci(rng, meas[:t0], pred[:t0], obs[:t0], S, L, G, c["B"])
        extra = (*np.percentile(rm, [2.5, 97.5]), lp, up)

    # truth: fuel-weighted saving over all commanded-ON hours (what settlement multiplies)
    on_all = ~off_eff
    truth = float((nos[camp] * s_h * on_all).sum() / (nos[camp] * on_all).sum())

    # before-after comparator: same ship with sail always ON after installation
    pre = np.arange(int(t0 * 0.75), t0)
    act_ba = nos[camp] * (1 - s_h) * np.exp(rng.normal(0, c["sd_m"], n_used))
    r_ba = 1 - (act_ba.sum() / P.sum()) / (meas[pre].sum() / pred[pre].sum())
    truth_ba = float((nos[camp] * s_h).sum() / nos[camp].sum())
    return (r_hat, lb, ub, truth, r_ba, truth_ba, *extra)


def placebo_ci(rng, A, P, obs, S, L, G, B):
    """Placebo-calibrated interval: apply the same random design to pre-installation (no-sail) data.
    eps = k_off / k_on - 1 is the calibration error of the counterfactual; scaled to campaign size."""
    ngt = (len(A) // L) // G
    n = ngt * G * L
    cell = np.arange(n) // L  # (group, block) cell index
    w = obs[:n].astype(float)
    Ag = np.bincount(cell, A[:n] * w, ngt * G).reshape(ngt, G)
    Pg = np.bincount(cell, P[:n] * w, ngt * G).reshape(ngt, G)
    off = rng.integers(0, G, (B, ngt))
    rows = np.arange(ngt)[None, :]
    Ao, Po = Ag[rows, off].sum(1), Pg[rows, off].sum(1)
    eps = (Ao / Po) / ((Ag.sum() - Ao) / (Pg.sum() - Po)) - 1
    eps *= np.sqrt(ngt / len(S))
    Aon, Pon, Aoff, Poff = S.sum(0)
    k = Aoff / Poff
    lo, hi = np.percentile(eps, [2.5, 97.5])
    # r = 1 - X(1+eps)/k decreases in eps -> lower bound uses the upper eps quantile
    return 1 - (Aon / Pon) * (1 + hi) / k, 1 - (Aon / Pon) * (1 + lo) / k


def summarize_ci(rows, s_target):
    """Per-method coverage / false-positive / payout for rows from run_one(ci_all=True)."""
    a = np.asarray(rows)
    tr = a[:, 3]
    pos = tr > 0
    out = {"n": len(a), "bias": float((a[:, 0] - tr).mean() * 100)}
    for name, j in (("group", 1), ("mbb", 6), ("placebo", 8)):
        lb, ub = a[:, j], a[:, j + 1]
        out[name] = dict(
            fp=float((lb > 0).mean() * 100) if s_target == 0 else None,
            cover=float((lb <= tr).mean() * 100), cover_ub=float((ub >= tr).mean() * 100),
            payout=float((np.clip(lb, 0, None)[pos] / tr[pos]).mean() * 100) if pos.any() else None)
    return out


def summarize(rows, s_target):
    a = np.asarray(rows)
    r, lb, ub, tr, rba, trba = a.T
    err = r - tr
    pos = tr > 0
    return dict(
        n=len(a), bias=err.mean() * 100, rmse=np.sqrt((err ** 2).mean()) * 100,
        err_p95=np.percentile(np.abs(err), 95) * 100,
        fp=float((lb > 0).mean() * 100) if s_target == 0 else np.nan,
        cover=float((lb <= tr).mean() * 100), cover_ub=float((ub >= tr).mean() * 100),
        payout=float((np.clip(lb, 0, None)[pos] / tr[pos]).mean() * 100) if pos.any() else np.nan,
        ba_bias=(rba - trba).mean() * 100,
    )
