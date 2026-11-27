"""SaveSail measurement simulator v2 (PRD §1.2-1.3, §7.2, §8 R2-R4, R8, R9).

Synthetic hourly voyage data -> pre-committed randomized ON/OFF blocks -> verifier estimate
(ratio-of-ratios on fuel-weighted sums) with a cross-fitted placebo-calibrated interval, plus the
verifier's manipulation checks (covariate balance C11, fuel/shaft-power balance and bunker mass
balance C13). Ground truth is known exactly per run.

Two stages: `prepare` builds one campaign (data, baseline model, placebo calibration) and
`evaluate` applies one scenario (honest run or a manipulation) to it. Manipulations only touch the
campaign period, so one prepared campaign serves every scenario of an experiment.

Version 1 (6-hour blocks, log-linear model, group bootstrap) is kept in legacy/measure_v1.py to
reproduce E1-E7.
"""
import numpy as np
from scipy.signal import lfilter
from scipy.stats import norm

VERSION = 2
# ship: (mean speed kn, laden no-sail fuel at mean speed kg/h, engine load at mean speed)
SHIP = {"bulk": (12.5, 1900.0, 0.70), "tanker": (13.0, 1700.0, 0.72), "gas": (16.5, 2600.0, 0.78),
        "container": (17.5, 3500.0, 0.75)}
# climate: wind mean m/s, wind sd, wind hourly rho, rel. angle mean deg, angle sd, angle rho, seasonal amplitude
CLIM = {
    "trade": (8.0, 2.0, 0.97, 95.0, 25.0, 0.995, 0.0),
    "n_atlantic": (9.0, 4.0, 0.95, 90.0, 60.0, 0.98, 0.25),
    "calm": (4.5, 2.0, 0.95, 90.0, 60.0, 0.98, 0.0),
    "monsoon": (7.0, 3.0, 0.96, 90.0, 45.0, 0.99, 0.5),
}
BASE = dict(
    ship="gas", clim="n_atlantic", s=0.06, hetero=True, rho_e=0.9, sd_e=0.03, sd_m=0.02,
    sd_q=0.005, sd_eta=0.005, sfoc_err=0.05, bdn_sd=0.001,
    foul=0.03, drydock=False, model="spline", camp_h=3600, train_h=3600, L=3, G=5, trans=0.25, miss=0.0,
    ci="pcf", pcf_k=5, B=1000, stud=True, mbb=4, ba=True, speed_src="ind", wx_src="ind", draft_sd=0.005, fuel_src="shaft",
    refusal="neighbor",
    # detection thresholds (R9): floor on |D| and z = |D|/SE per test
    det=True, cov_floor=0.005, cov_z=4.0, speed_floor=0.0, speed_z=4.0, q_floor=0.002, q_z=3.0,
    mass_tol=0.005, mass_z=4.0, storm_ms=17.2, storm_ref_ms=13.9,
    # independent sources (verifier-fetched): AIS speed (current-corrected) and weather/wave reanalysis.
    # errors are autocorrelated (hourly rho) like real reanalysis errors
    ind_v_sd=0.2, ind_v_rho=0.995, ind_u_sd=1.5, ind_u_rho=0.95, ind_th_sd=15.0, ind_th_rho=0.95,
    ind_h_sd=0.3, ind_h_rho=0.98,
)
# scenario keys (evaluate); defaults = honest campaign
SCEN = dict(att_offon=0.0, exclude_viol=True, att_withhold=0.0, att_who="payer", fc_noise=0.3,
            att_speed=0.0, att_report=None, att_meter=0.0, att_shift=0.0, storm=False)
HPY = 6000.0  # sailing hours per year


def ar1(rng, n, rho, sd, burn=600):
    x = lfilter([1.0], [1.0, -rho], rng.standard_normal(n + burn))[burn:]
    return x * sd * np.sqrt(1.0 - rho * rho)


def sfoc(load):
    """Relative specific fuel oil consumption vs engine load (shop-test shape, minimum near 75% MCR)."""
    return 1.0 + 0.35 * (load - 0.75) ** 2


ENGINE_LOADS = np.round(np.arange(0.30, 1.0001, 0.05), 2)


def engine_table():
    """Shop-test table as published to the verifier (offchain/synth engine_curve.csv): load, SFOC to 6 decimals."""
    return ENGINE_LOADS, np.array([float(f"{sfoc(x):.6f}") for x in ENGINE_LOADS])


def shop_curve(loads, values):
    """Quadratic through the shop-test table; the verifier uses the same fit, so both sides agree exactly."""
    return np.polyfit(loads, values, 2)


SHOP = shop_curve(*engine_table())


def calibrate_sfoc(fuel, shaft, mcr, idx, shop=SHOP):
    """Fuel/shaft ratio = shop curve(load) x exp(a + b*load), load = shaft power / MCR; (a, b) fitted on
    pre-installation hours (PRD C13). The curve's shape comes from the engine test, so nothing has to be
    extrapolated when the sail or a hull cleaning moves the load below the training range."""
    ld = shaft / mcr
    Z = np.column_stack([np.ones(len(ld)), ld])
    return np.linalg.lstsq(Z[idx], (np.log(fuel / shaft) - np.log(np.polyval(shop, ld)))[idx], rcond=None)[0]


def shaft_fuel(shaft, mcr, ab, shop=SHOP):
    """Shaft power converted to fuel on the meter's scale with the calibrated curve (estimator fuel, C22)."""
    ld = shaft / mcr
    return shaft * np.polyval(shop, ld) * np.exp(ab[0] + ab[1] * ld)


def fuel_of(c, st, ell, hrs):
    """True fuel at engine load `ell` (shaft power / MCR): power x SFOC(load) with the engine's deviation
    from its shop test (sfoc_err per unit load) and hourly efficiency noise."""
    return ell * st["mcr"] * sfoc(ell) * (1 + c["sfoc_err"] * (ell - 0.75)) * st["eta"][hrs]


# ---------------------------------------------------------------- baseline model (PRD §7.2-1)
def knots(V, Uh, Hs, idx):
    """Knots at the 5/27.5/50/72.5/95% quantiles of the fitting data (no knowledge of the true model)."""
    q = [5, 27.5, 50, 72.5, 95]
    return dict(Uh=np.percentile(Uh[idx], q), Hs=np.percentile(Hs[idx], q), lv=np.percentile(np.log(V[idx]), q))


def ncs(x, k):
    """Natural cubic spline basis (linear term + K-2 truncated-power terms), Hastie et al. ESL eq. 5.4-5.5."""
    d = lambda j: (np.maximum(x - k[j], 0) ** 3 - np.maximum(x - k[-1], 0) ** 3) / (k[-1] - k[j])
    return [x] + [d(j) - d(len(k) - 2) for j in range(len(k) - 2)]


def features(model, V, D, Uh, Hs, kn=None):
    cols = [np.ones_like(V), np.log(V)]
    if model in ("full", "spline", "no_weather"):
        cols.append(np.log(D))
    if model == "full":
        cols += [Uh * np.abs(Uh), Hs ** 2]
    if model == "spline":
        cols += [*ncs(Uh, kn["Uh"]), *ncs(Hs, kn["Hs"]), *ncs(np.log(V), kn["lv"])[1:], np.log(V) * np.log(D)]
    return np.column_stack(cols)


def fit(model, V, D, Uh, Hs, y, idx):
    kn = knots(V, Uh, Hs, idx) if model == "spline" else None
    beta = np.linalg.lstsq(features(model, V[idx], D[idx], Uh[idx], Hs[idx], kn), y[idx], rcond=None)[0]
    return kn, beta


def predict(model, kn, beta, V, D, Uh, Hs):
    return np.exp(features(model, V, D, Uh, Hs, kn) @ beta)


# ---------------------------------------------------------------- estimator and intervals
def ratio(S):
    """S[..., 4] = (A_on, P_on, A_off, P_off) sums -> savings rate."""
    return 1.0 - (S[..., 0] / S[..., 1]) / (S[..., 2] / S[..., 3])


def boot_weights(rng, ng, B, kind, m):
    if kind == "group":
        return rng.multinomial(ng, np.full(ng, 1.0 / ng), size=B).astype(float)
    k = -(-ng // m)  # moving-block bootstrap over consecutive groups
    starts = rng.integers(0, ng - m + 1, size=(B, k))
    idx = (starts[..., None] + np.arange(m)).reshape(B, -1)[:, :ng]
    W = np.zeros((B, ng))
    np.add.at(W, (np.repeat(np.arange(B), ng), idx.ravel()), 1.0)
    return W


def _eps_window(rng, a, p, G, B):
    """Ratio-of-ratios error of B random schedules (one OFF block per group) on a window of groups."""
    off = rng.integers(0, G, (B, len(a)))
    rows = np.arange(len(a))[None, :]
    Ao, Po = a[rows, off].sum(1), p[rows, off].sum(1)
    return (Ao / Po) / ((a.sum() - Ao) / (p.sum() - Po)) - 1


def neighbor_rows(S_all, paired, unpaired, m_side=3):
    """C24: a group without an OFF block (refused within the cap) keeps its ON hours. Its prediction is
    rescaled by the OFF ratio of the nearest `m_side` paired groups on each side relative to the campaign's OFF
    ratio, so the slow residual drift that pairing removes is removed locally too. Meant for isolated groups;
    long runs of refusals are over the cap and settle by adverse imputation instead (run_r4.py).
    Returns rows [ON fuel, ON pred', 0, 0]."""
    idx = np.flatnonzero(paired)
    if not len(idx) or not unpaired.any():
        return np.zeros((0, 4))
    k_all = S_all[idx, 2].sum() / S_all[idx, 3].sum()
    out = []
    for g in np.flatnonzero(unpaired):
        j = np.searchsorted(idx, g)
        nb = list(idx[max(0, j - m_side):j]) + list(idx[j:j + m_side])
        k_loc = S_all[nb, 2].sum() / S_all[nb, 3].sum()
        out.append([S_all[g, 0], S_all[g, 1] * k_loc / k_all, 0.0, 0.0])
    return np.array(out)


MIX_XI = norm.ppf((np.arange(7) + 0.5) / 7)  # standard normal quantiles for the scale-uncertainty mixture (C25)


def off_pair_q(x):
    """Short-lag noise level of a dataset from its OFF blocks (C25): q = mean over consecutive groups of
    (x_g - x_{g+1})^2 / 2, x = log(OFF fuel / OFF prediction) of each group's OFF block, NaN where a group has none.
    OFF blocks carry no sail effect, so the campaign's value is free of effect variation and of anything done in ON
    blocks. Returns (q, cv): cv is q's coefficient of variation (Newey-West over the pair terms), the sampling error
    of the campaign's level that interval_from_eps mixes over. None without two consecutive OFF blocks."""
    d = x[1:] - x[:-1]
    t = d[np.isfinite(d)] ** 2 / 2
    n = t.size
    if n < 3:
        return None
    u = t - t.mean()
    lag = int(np.ceil(n ** (1 / 3)))
    v = (u @ u) / n + 2 * sum((1 - j / (lag + 1)) * (u[j:] @ u[:-j]) / n for j in range(1, lag + 1))
    return float(t.mean()), float(np.sqrt(max(v, 0.0) / n) / t.mean())


def window_q(Ag, Pg):
    """Expected off_pair_q of a training window over uniform OFF positions: for each consecutive pair of groups the
    mean of (x[g, b1] - x[g+1, b2])^2 / 2 over all block pairs, averaged over the pairs (C25)."""
    with np.errstate(divide="ignore", invalid="ignore"):
        x = np.where((Ag > 0) & (Pg > 0), np.log(Ag / Pg), np.nan)
    d = (x[:-1, :, None] - x[1:, None, :]) ** 2 / 2
    v = np.nanmean(d.reshape(len(d), -1), axis=1) if len(d) else np.zeros(0)
    v = v[np.isfinite(v)]
    return float(v.mean()) if v.size else None


def placebo_eps(rng, A, P, wt, L, G, B, n_target, windows=5):
    """Counterfactual calibration error under the same random design on pre-installation data.

    The error distribution is taken directly at the campaign's length: up to `windows` evenly spaced runs of
    `n_target` consecutive training groups (out-of-fold predictions), so the dependence between groups is the
    same as in the campaign and no 1/sqrt(n) extrapolation is needed. A campaign longer than the training
    data uses the whole training span and extrapolates with the variance-time exponent measured on it
    (alpha = log2(var of half span / var of full span), clipped to [0.5, 1]; 1 means independent groups).
    Each window also carries its noise level (window_q, C25), so interval_from_eps can rescale it to the campaign's.
    Returns [(eps, groups, alpha, q)]; interval_from_eps scales each by (groups / campaign groups) ** (alpha / 2)."""
    ngt = (len(A) // L) // G
    n = ngt * G * L
    cell = np.arange(n) // L  # (group, block) cell index
    Ag = np.bincount(cell, A[:n] * wt[:n], ngt * G).reshape(ngt, G)
    Pg = np.bincount(cell, P[:n] * wt[:n], ngt * G).reshape(ngt, G)
    if n_target <= ngt:
        starts = np.unique(np.linspace(0, ngt - n_target, min(windows, ngt - n_target + 1)).round().astype(int))
        return [(_eps_window(rng, Ag[s0:s0 + n_target], Pg[s0:s0 + n_target], G, B), n_target, 1.0,
                 window_q(Ag[s0:s0 + n_target], Pg[s0:s0 + n_target])) for s0 in starts]
    full = _eps_window(rng, Ag, Pg, G, B)
    h = ngt // 2
    half = np.var(np.concatenate([_eps_window(rng, Ag[:h], Pg[:h], G, B), _eps_window(rng, Ag[h:2 * h], Pg[h:2 * h], G, B)]))
    alpha = float(np.clip(np.log2(half / max(np.var(full), 1e-30)), 0.5, 1.0))
    return [(full, ngt, alpha, window_q(Ag, Pg))]


def interval_from_eps(S, eps_windows, n_groups=None, q=None):
    """S: per-group [ON fuel, ON pred, OFF fuel, OFF pred]; n_groups: groups that carry an OFF block (default len(S));
    q: the campaign's (level, cv) from off_pair_q over its groups' OFF blocks (C25). A single training window's
    realised noise level differs from the campaign's by 10-30% from run to run, which made the interval too narrow
    on average: each window is rescaled by sqrt(level / its own level), and the errors are mixed over the level's
    sampling error (log-scale spread cv / 2 at the MIX_XI quantiles)."""
    n = n_groups or len(S)
    eps = np.concatenate([e * (m / n) ** (al / 2) * (np.sqrt(q[0] / qw) if q and qw else 1.0) for e, m, al, qw in eps_windows])
    if q and q[1] > 0:
        eps = (eps[:, None] * np.exp(0.5 * q[1] * MIX_XI)[None, :]).ravel()
    Aon, Pon, Aoff, Poff = S.sum(0)
    k = Aoff / Poff
    lo, hi = np.percentile(eps, [2.5, 97.5])
    # r = 1 - X(1+eps)/k decreases in eps -> lower bound uses the upper eps quantile
    return 1 - (Aon / Pon) * (1 + hi) / k, 1 - (Aon / Pon) * (1 + lo) / k


def balance(xg_on, xg_off, scale, floor, z):
    """C11-style balance test on group means, Newey-West SE over consecutive groups. Returns (D, SE, flagged)."""
    d = (xg_off - xg_on) / scale
    d = d[np.isfinite(d)]
    n = len(d)
    if n < 3:
        return 0.0, np.inf, False
    D = d.mean()
    u = d - D
    lag = max(1, int(round(n ** (1 / 3))))
    v = (u @ u) / n + 2 * sum((1 - j / (lag + 1)) * (u[j:] @ u[:-j]) / n for j in range(1, lag + 1))
    SE = np.sqrt(max(v, 0.0) / n)
    return float(D), float(SE), bool(abs(D) >= max(floor, 1e-4) and abs(D) > z * SE)


# ---------------------------------------------------------------- stage 1: one campaign
def prepare(c, seed):
    rng = np.random.default_rng(seed)
    vm, fref, load0 = SHIP[c["ship"]]
    um, usd, urho, am, asd, arho, amp = CLIM[c["clim"]]
    T, t0, L, G = c["train_h"] + c["camp_h"], c["train_h"], c["L"], c["G"]
    # voyages: alternating laden / ballast, 300-700 sailing hours
    lens = rng.integers(300, 701, size=T // 300 + 2)
    vid = np.repeat(np.arange(lens.size), lens)[:T]
    laden = ((vid + rng.integers(2)) % 2 == 0)
    D = np.where(laden, 1.0, 0.62)
    V = np.clip(vm + rng.normal(0, 0.5, lens.size)[vid] + ar1(rng, T, 0.99, 0.4), 10.0, vm + 3.0)
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
    w = 1 + np.maximum(0.0012 * Uh * np.abs(Uh), -0.04) + 0.02 * Hs ** 2
    # required propulsion power without sail, as engine load (shaft power / MCR); fuel follows from SFOC(load)
    ell = load0 * (V / vm) ** 3 * D ** (2 / 3) * w * foul * np.exp(ar1(rng, T, c["rho_e"], c["sd_e"]))
    mcr = fref / (load0 * sfoc(load0))  # MCR in fuel-equivalent units: fuel = fref at the mean load

    ng = (c["camp_h"] // L) // G
    n = ng * G * L
    camp = np.arange(t0, t0 + n)
    grp = np.repeat(np.arange(ng), G * L)
    blk = np.tile(np.repeat(np.arange(G), L), ng)
    is_off = blk == rng.integers(0, G, size=ng)[grp]
    tw = np.where(np.tile(np.arange(L), ng * G) == 0, 1.0 - c["trans"], 1.0)  # transient exclusion (C19)
    th_c = th[camp]
    raw = np.clip((U[camp] / 8.0) ** 2, 0, 3) * np.where(th_c > 20, np.sin(np.radians(th_c)) ** 2, 0.0)
    s_h = np.minimum(c["s"] * raw / max(raw.mean(), 1e-9), 0.45) if c["hetero"] else np.full(n, c["s"])

    # noise draws shared by every scenario
    m_fuel = np.exp(rng.normal(0, c["sd_m"], T))
    eta = np.exp(rng.normal(0, c["sd_eta"], T))
    nos = fuel_of(c, dict(mcr=mcr, eta=eta), ell, slice(None))  # no-sail fuel
    m_q = np.exp(rng.normal(0, c["sd_q"], T))
    Vo, Uo = V + rng.normal(0, 0.1, T), np.clip(U + rng.normal(0, 0.5, T), 0, None)
    tho = th + rng.normal(0, 5, T)
    Hso = np.clip(Hs + rng.normal(0, 0.2, T), 0.05, None)
    Dobs = D * np.exp(rng.normal(0, c["draft_sd"], T))  # reported draft (sensor noise)
    obs = rng.random(T) >= c["miss"]
    m_ba = np.exp(rng.normal(0, c["sd_m"], n))
    bdn = 1 + rng.normal(0, c["bdn_sd"], lens.size)
    # independent sources (C11 cross-checks, C17 storm check)
    Vind = V + ar1(rng, T, c["ind_v_rho"], c["ind_v_sd"]) + rng.normal(0, 0.05, T)
    Uref = np.clip(U + ar1(rng, T, c["ind_u_rho"], c["ind_u_sd"]), 0, None)
    thref = th + ar1(rng, T, c["ind_th_rho"], c["ind_th_sd"])
    Href = np.clip(Hs + ar1(rng, T, c["ind_h_rho"], c["ind_h_sd"]), 0.05, None)

    # training period (no sail): SFOC calibration, baseline model, cross-fitted placebo
    tr = np.arange(t0)[obs[:t0]]
    meas_tr = nos[:t0] * m_fuel[:t0]
    # SFOC curve shape calibrated on training data (metered fuel / torque-meter shaft power vs load)
    shaft_tr = ell[:t0] * mcr * m_q[:t0]  # torque-meter reading
    bq = calibrate_sfoc(meas_tr, shaft_tr, mcr, tr)
    # fuel used by the estimator (C22): shaft power x calibrated SFOC on the meter's scale, or the meter itself
    fuel_tr = shaft_fuel(shaft_tr, mcr, bq) if c["fuel_src"] == "shaft" else meas_tr
    Uho = Uo * np.cos(np.radians(tho))
    y = np.full(T, np.nan)
    y[:t0] = np.log(fuel_tr)  # only the training part is used
    Vm = Vind if c["speed_src"] == "ind" else Vo  # speed fed to the baseline model
    if c["wx_src"] == "ind":  # weather inputs from the independent reanalysis instead of ship sensors
        Uho, Hso_m = Uref * np.cos(np.radians(thref)), Href
    else:
        Hso_m = Hso
    kn, beta = fit(c["model"], Vm, Dobs, Uho, Hso_m, y, tr)
    pred_tr = predict(c["model"], kn, beta, Vm[:t0], Dobs[:t0], Uho[:t0], Hso_m[:t0])
    eps = None
    if c["ci"] == "pcf":
        K = c["pcf_k"]
        gl = L * G
        fold = np.minimum((np.arange(t0) // gl) * K // max(t0 // gl, 1), K - 1)
        pcf = np.empty(t0)
        for k in range(K):
            ev, fi = np.flatnonzero(fold == k), np.flatnonzero(fold != k)
            fi = fi[obs[fi]]
            knk, bk = fit(c["model"], Vm, Dobs, Uho, Hso_m, y, fi)
            pcf[ev] = predict(c["model"], knk, bk, Vm[ev], Dobs[ev], Uho[ev], Hso_m[ev])
        twt = np.where(np.arange(t0) % L == 0, 1.0 - c["trans"], 1.0) * obs[:t0]
        eps = placebo_eps(rng, fuel_tr, pcf, twt, L, G, c["B"], ng)
    return dict(c=c, seed=seed, rng_seed=seed + 7_777_777, T=T, t0=t0, ng=ng, n=n, camp=camp, grp=grp, blk=blk,
                is_off=is_off, tw=tw, raw=raw, s_h=s_h, V=V, D=D, U=U, nos=nos, ell=ell, mcr=mcr, vid=vid, fref=fref, load0=load0,
                m_fuel=m_fuel, eta=eta, m_q=m_q, Vo=Vo, Uo=Uo, tho=tho, Hso=Hso, Dobs=Dobs, obs=obs, m_ba=m_ba, bdn=bdn,
                Vind=Vind, Uref=Uref, thref=thref, Href=Href, kn=kn, beta=beta, pred_tr=pred_tr, meas_tr=meas_tr, fuel_tr=fuel_tr, shaft_tr=shaft_tr, eps=eps, bq=bq)


# ---------------------------------------------------------------- stage 2: one scenario
def evaluate(st, sc=None):
    c = st["c"]
    sc = dict(SCEN, **(sc or {}))
    rng = np.random.default_rng(st["rng_seed"])
    ng, n, camp, grp, blk, G, t0 = st["ng"], st["n"], st["camp"], st["grp"], st["blk"], c["G"], st["t0"]
    is_off, tw, raw, s_h = st["is_off"], st["tw"], st["raw"], st["s_h"]

    # selective withholding (M6). payer / investor rep reveal last and know the OFF position;
    # the verifier reveals first (C18) and can only use a group-level forecast.
    withheld = np.zeros(ng, bool)
    if sc["att_withhold"] > 0:
        fc = raw * np.exp(rng.normal(0, sc["fc_noise"], n))
        on_mean = np.bincount(grp, fc * ~is_off, ng) / np.bincount(grp, (~is_off).astype(float), ng)
        off_mean = np.bincount(grp, fc * is_off, ng) / np.bincount(grp, is_off.astype(float), ng)
        score = {"payer": on_mean - off_mean, "investor": off_mean - on_mean}.get(sc["att_who"], np.bincount(grp, fc, ng))
        k = int(np.floor(sc["att_withhold"] * ng + 1e-9))
        if k:
            withheld[np.argsort(-score)[:k]] = True
    off_eff = is_off & ~withheld[grp]

    # safety overrides above the weather threshold (C17): every strong-wind ON block is folded
    storm_grp = np.zeros(ng, bool)
    folded = np.zeros(n, bool)
    if sc["storm"]:
        strong = (st["Uo"][camp] >= c["storm_ms"]) & (st["Uref"][camp] >= c["storm_ref_ms"]) & ~off_eff
        cell = grp * G + blk
        folded = (np.bincount(cell, strong, ng * G) > 0)[cell] & ~off_eff
        storm_grp = np.bincount(grp, folded, ng) > 0

    viol_grp = rng.random(ng) < sc["att_offon"]  # sail kept ON during commanded OFF
    sail_on = (~off_eff & ~folded) | (off_eff & viol_grp[grp])

    V = st["V"][camp].copy()
    ell_n = st["ell"][camp].copy()
    if sc["att_speed"]:  # real speed change in OFF blocks (power ~ V^3)
        ell_n[off_eff] *= (1 + sc["att_speed"]) ** 3
        V[off_eff] *= 1 + sc["att_speed"]
    nos = fuel_of(c, st, ell_n, camp)
    ell_a = ell_n * (1 - s_h * sail_on)  # the sail lowers the propulsion power the engine has to deliver
    act = fuel_of(c, st, ell_a, camp)
    meas = act * st["m_fuel"][camp]
    shaft_m = ell_a * st["mcr"] * st["m_q"][camp]
    vid = st["vid"][camp]
    nv = vid.max() + 1
    if sc["att_meter"]:
        meas[off_eff] *= 1 - sc["att_meter"]
    if sc["att_shift"]:  # move readings from OFF to ON hours, keeping each voyage total
        x = sc["att_shift"]
        moved = np.bincount(vid, meas * off_eff * x, nv)
        on_tot = np.bincount(vid, meas * ~off_eff, nv)
        gain = np.divide(moved, on_tot, out=np.zeros_like(moved), where=on_tot > 0)
        meas = np.where(off_eff, meas * (1 - x), meas * (1 + gain[vid]))

    Vo = V + (st["Vo"][camp] - st["V"][camp])
    Uo, tho, Hso, Do = st["Uo"][camp].copy(), st["tho"][camp].copy(), st["Hso"][camp].copy(), st["Dobs"][camp].copy()
    if sc["att_report"]:  # reported-value manipulation in one block type
        what, size, where = sc["att_report"]
        h = off_eff if where == "off" else ~off_eff
        if what == "speed":
            Vo[h] *= 1 + size
        elif what == "wind":
            Uo[h] *= 1 + size
        elif what == "angle":
            tho[h] += size
        elif what == "wave":
            Hso[h] *= 1 + size
        elif what == "draft":
            Do[h] *= 1 + size
    Uho = Uo * np.cos(np.radians(tho))
    Vind = V + (st["Vind"][camp] - st["V"][camp])  # independent speed follows the real speed
    cacheable = not sc["att_report"] and not sc["att_speed"]
    if cacheable and "P0" in st:
        P = st["P0"]
    else:
        if c["wx_src"] == "ind":
            Uh_m, Hs_m = st["Uref"][camp] * np.cos(np.radians(st["thref"][camp])), st["Href"][camp]
        else:
            Uh_m, Hs_m = Uho, Hso
        P = predict(c["model"], st["kn"], st["beta"], Vind if c["speed_src"] == "ind" else Vo, Do, Uh_m, Hs_m)
        if cacheable:
            st["P0"] = P

    obs = st["obs"][camp]
    # paired groups; refused groups re-enter below through their neighbours' OFF ratio (C24). Unpaired ON hours
    # with the campaign-wide OFF ratio would bring back slow residual drift (run_r4.py, rule include_on).
    inc = ~withheld[grp] & ~storm_grp[grp] & obs
    if sc["exclude_viol"]:
        inc &= ~viol_grp[grp]
    w_on, w_off = (inc & ~off_eff) * tw, (inc & off_eff) * tw
    fs = shaft_fuel(shaft_m, st["mcr"], st["bq"])  # shaft-implied fuel on the meter's scale
    F = fs if c["fuel_src"] == "shaft" else meas
    S_all = np.column_stack([np.bincount(grp, F * w_on, ng), np.bincount(grp, P * w_on, ng),
                             np.bincount(grp, F * w_off, ng), np.bincount(grp, P * w_off, ng)])
    ok_g = (S_all[:, 1] > 0) & (S_all[:, 3] > 0)
    S = S_all[ok_g]
    if c["refusal"] == "neighbor" and withheld.any():  # refused groups keep their ON hours (C24)
        w_ref = (withheld[grp] & ~storm_grp[grp] & obs) * tw  # all five blocks were ON
        R = np.column_stack([np.bincount(grp, F * w_ref, ng), np.bincount(grp, P * w_ref, ng), S_all[:, 2], S_all[:, 3]])
        S = np.vstack([S, neighbor_rows(R, ok_g, withheld & (R[:, 1] > 0))])
    r_hat = float(ratio(S.sum(0)))
    if st["eps"] is not None:
        q_c = None
        if c["stud"]:  # campaign noise level from its paired groups' OFF blocks (C25)
            with np.errstate(divide="ignore", invalid="ignore"):
                q_c = off_pair_q(np.where(ok_g, np.log(S_all[:, 2] / S_all[:, 3]), np.nan))
        lb, ub = interval_from_eps(S, st["eps"], int(ok_g.sum()), q_c)
    else:
        lb, ub = np.percentile(ratio(boot_weights(rng, len(S), c["B"], c["ci"], c["mbb"]) @ S), [2.5, 97.5])

    # truth: fuel-weighted saving over commanded-ON hours the settlement multiplies
    # (transient-weighted; safety-override groups excluded, their fuel settles at the normal bound)
    on_all = (~off_eff & ~storm_grp[grp]) * tw * st["obs"][camp]  # hours with data: kg_on only counts these
    truth = float(((nos - act) * on_all).sum() / (nos * on_all).sum())
    out = dict(r=r_hat, lb=float(lb), ub=float(ub), truth=truth, ng=int(ok_g.sum()))

    if c["ba"]:  # before-after comparator: same ship with sail always ON after installation
        pre = np.arange(int(t0 * 0.75), t0)
        full_ba = fuel_of(c, st, ell_n * (1 - s_h), camp)
        act_ba = full_ba * st["m_ba"]
        out["r_ba"] = float(1 - (act_ba.sum() / P.sum()) / (st["meas_tr"][pre].sum() / st["pred_tr"][pre].sum()))
        out["truth_ba"] = float((nos - full_ba).sum() / nos.sum())

    if c["det"]:  # verifier manipulation checks on included groups (C11, C13)
        # groups that contain a voyage change (port call) are left out of the balance tests
        vg = vid.reshape(ng, -1)
        same_voy = (vg == vg[:, :1]).all(1)
        keep = np.flatnonzero(ok_g & same_voy)
        c_on, c_off = np.bincount(grp, w_on, ng)[keep], np.bincount(grp, w_off, ng)[keep]

        def test(x, scale, floor, z):
            return balance(np.bincount(grp, x * w_on, ng)[keep] / c_on, np.bincount(grp, x * w_off, ng)[keep] / c_off,
                           scale, floor, z)[2]

        z, fl = c["cov_z"], c["cov_floor"]
        flags = dict(
            # balance of the reported inputs (catches real behaviour such as slowing down in OFF blocks)
            speed=test(Vo, Vo.mean(), c["speed_floor"], c["speed_z"]),
            draft=test(Do, Do.mean(), 0.0, z),
            headwind=test(Uho, Uo.mean(), fl, z),
            wave=test(Hso, Hso.mean(), fl, z),
            # balance of reported minus independent values (catches falsified reports)
            speed_x=test(Vo - Vind, Vo.mean(), 0.0, z),
            wind_x=test(Uo - st["Uref"][camp], Uo.mean(), 0.0, z),
            angle_x=test(tho - st["thref"][camp], 90.0, 0.0, z),
            wave_x=test(Hso - st["Href"][camp], Hso.mean(), 0.0, z),
        )
        q = meas / fs
        flags["fuel_shaft"] = test(q, 1.0, c["q_floor"], c["q_z"])
        # voyage bunker mass balance: tolerance max(0.5%, z x statistical sd)
        hours = np.bincount(vid, minlength=nv)
        tot_m = np.bincount(vid, meas, nv)
        tot_t = np.bincount(vid, act, nv) * st["bdn"][:nv]
        sq = np.std(np.log(q))
        with np.errstate(invalid="ignore", divide="ignore"):
            sd_v = np.sqrt(sq ** 2 * np.bincount(vid, meas ** 2, nv) / tot_m ** 2 + c["bdn_sd"] ** 2)
            dev = np.abs(tot_m - tot_t) / tot_m
        used = hours >= 200  # voyages with at least 200 campaign hours (partial ends carry over)
        flags["mass"] = bool((dev > np.maximum(c["mass_tol"], c["mass_z"] * sd_v))[used].any())
        out["flags"] = flags
        out["detected"] = bool(any(flags.values()))
    return out


def run_one(c, seed, sc=None):
    return evaluate(prepare(c, seed), sc)


# ---------------------------------------------------------------- summaries
def summarize(rows):
    r = np.array([x["r"] for x in rows])
    lb = np.array([x["lb"] for x in rows])
    ub = np.array([x["ub"] for x in rows])
    tr = np.array([x["truth"] for x in rows])
    err = (r - tr) * 100
    pos = tr > 0
    n = len(rows)
    out = dict(n=n, bias=float(err.mean()), bias_se=float(err.std(ddof=1) / np.sqrt(n)),
               err_p95=float(np.percentile(np.abs(err), 95)),
               lb_pos=int((lb > 0).sum()), lb_le_truth=int((lb <= tr).sum()), ub_ge_truth=int((ub >= tr).sum()),
               payout=float((np.clip(lb, 0, None)[pos] / tr[pos]).mean() * 100) if pos.any() else None)
    if "r_ba" in rows[0]:
        out["ba_bias"] = float(np.mean([(x["r_ba"] - x["truth_ba"]) * 100 for x in rows]))
    if "detected" in rows[0]:
        det = np.array([x["detected"] for x in rows])
        out["detected"] = int(det.sum())
        out["bias_undetected"] = float((err * ~det).sum() / n)  # detected runs settle as violations (PRD 1.3)
        for k in rows[0]["flags"]:
            out[f"flag_{k}"] = int(sum(x["flags"][k] for x in rows))
    return out
