"""Test grids A-D (PRD §1.3) and the manipulation panel (M6-M9) evaluated on every grid condition."""
import itertools

import numpy as np

from measure import BASE, CLIM, SHIP

REF = dict(ship="gas", clim="n_atlantic", sd_e=0.03, sd_m=0.02)  # grid A reference condition


def grid_A():
    return [dict(REF, s=s, hetero=h) for s in (0.0, 0.03, 0.05, 0.08, 0.12) for h in (False, True)]


def grid_B():
    levels = [("ship", list(SHIP)), ("clim", list(CLIM)), ("sd_e", [0.01, 0.03, 0.05, 0.08]),
              ("rho_e", [0.5, 0.8, 0.9, 0.95, 0.98, 0.995]), ("sd_m", [0.005, 0.02, 0.05]),
              ("foul", [0.0, 0.03, 0.08]), ("drydock", [True]), ("miss", [0.0, 0.05, 0.15]),
              ("camp_h", [1800, 3600, 7200])]
    out, seen = [], set()
    for k, vals in levels:
        for v in vals:
            for s in (0.0, 0.06):
                c = dict(REF, s=s, hetero=True, **{k: v})
                key = tuple(sorted(dict(BASE, **c).items()))
                if key not in seen:
                    seen.add(key)
                    out.append(c)
    return out


def grid_C(n=300, seed=20261127):
    """Latin hypercube over the whole operating range (PRD 1.2); 30% of conditions have no sail effect."""
    r = np.random.default_rng(seed)
    dims = 10
    u = (np.argsort(r.random((dims, n)), axis=1).T + r.random((n, dims))) / n  # one stratum per condition
    ships, clims = list(SHIP), list(CLIM)
    out = []
    for i in range(n):
        x = u[i]
        s = 0.0 if x[2] < 0.3 else round(0.12 * (x[2] - 0.3) / 0.7, 4)
        out.append(dict(
            ship=ships[int(x[0] * 4)], clim=clims[int(x[1] * 4)], s=s, hetero=bool(x[3] < 0.5),
            sd_e=round(0.08 * x[4], 4), rho_e=round(0.5 + 0.495 * x[5], 4), sd_m=round(0.005 + 0.045 * x[6], 4),
            foul=round(0.08 * x[7], 4), drydock=bool(r.random() < 0.2), miss=round(0.15 * x[8], 4),
            camp_h=int(round((1800 + 5400 * x[9]) / 15) * 15)))
    return out


def grid_D():
    worst = dict(sd_e=(0.03, 0.08), rho_e=(0.9, 0.995), sd_m=(0.02, 0.05), miss=(0.0, 0.15), camp_h=(3600, 1800),
                 foul=((0.03, False), (0.08, True)))
    keys = list(worst)
    out = []
    for bits in itertools.product((0, 1), repeat=len(keys)):
        c = dict(REF)
        for k, b in zip(keys, bits):
            v = worst[k][b]
            if k == "foul":
                c["foul"], c["drydock"] = v
            else:
                c[k] = v
        for s in (0.0, 0.05):
            out.append(dict(c, s=s, hetero=True))
    return out


METHODS = {  # R8 comparison: design and estimator choices (PRD C19, C20, C22); the last one is adopted
    "v1_L6_loglin_group": dict(L=6, model="full", ci="group", speed_src="log", wx_src="ship", fuel_src="meter"),
    "L6_spline_pcf": dict(L=6, model="spline", ci="pcf", speed_src="log", wx_src="ship", fuel_src="meter"),
    "L3_spline_pcf": dict(L=3, model="spline", ci="pcf", speed_src="log", wx_src="ship", fuel_src="meter"),
    "L3_spline_pcf_ind_meter": dict(L=3, model="spline", ci="pcf", speed_src="ind", wx_src="ind", fuel_src="meter"),
    "L3_spline_pcf_ind_shaft": dict(L=3, model="spline", ci="pcf", speed_src="ind", wx_src="ind", fuel_src="shaft"),
}


def grid_R8():
    conds = [dict(), dict(rho_e=0.8), dict(rho_e=0.995), dict(sd_e=0.08), dict(camp_h=1800), dict(ship="bulk", miss=0.15)]
    return [dict(REF, method=k, s=s, hetero=True, **mv, **cv)
            for k, mv in METHODS.items() for cv in conds for s in (0.0, 0.05)]


def newbuild(grid):
    """Grids A-D in newbuild mode (no pre-installation data, PRD C28). Same conditions and seeds as the base grid,
    so every run is paired with the adopted method on the same campaign."""
    return lambda: [dict(c, newbuild=True) for c in grid()]


GRIDS = {"A": grid_A, "B": grid_B, "C": grid_C, "D": grid_D, "R8": grid_R8,
         "NA": newbuild(grid_A), "NB": newbuild(grid_B), "NC": newbuild(grid_C), "ND": newbuild(grid_D)}
REPS = {"A": 10_000, "B": 2_000, "C": 2_000, "D": 2_000, "R8": 2_000, "NA": 10_000, "NB": 2_000, "NC": 2_000, "ND": 2_000}
# newbuild grids: manipulation panel at 500 runs per condition (as grid C) to fit the job time limit; the slower
# cross-fitted baseline makes each evaluation about 2.5x the adopted method's
ATTACK_REPS = {"A": 2_000, "B": 2_000, "C": 500, "D": 2_000, "R8": 0, "NA": 2_000, "NB": 2_000, "NC": 500, "ND": 2_000}


def attack_panel():
    """Scenarios applied to every condition with a sail effect (PRD M6-M9). name -> scenario."""
    p = {"storm": dict(storm=True)}  # M6-3
    for q in (0.02, 0.05, 0.10, 0.20):  # M7
        p[f"offon_{q}"] = dict(att_offon=q)
    for x in (-0.05, -0.03, 0.03, 0.05):  # M8-1 real speed change in OFF blocks
        p[f"speed_{x}"] = dict(att_speed=x)
    for side in ("off", "on"):  # M8-2 reported values (draft is the only ship-reported model input)
        for x in range(1, 11):
            p[f"rep_draft_{side}_{x}"] = dict(att_report=("draft", x / 100, side))
            p[f"rep_draft_{side}_-{x}"] = dict(att_report=("draft", -x / 100, side))
        for what, xs in (("wind", (0.01, 0.05, 0.10)), ("wave", (0.01, 0.05, 0.10)), ("angle", (1, 5, 10)),
                         ("speed", (0.001, 0.01, 0.02))):
            for x in xs:
                p[f"rep_{what}_{side}_{x}"] = dict(att_report=(what, x, side))
                p[f"rep_{what}_{side}_-{x}"] = dict(att_report=(what, -x, side))
    for x in np.arange(1, 11) * 0.005:  # M9-2: OFF under/over-reading and total-preserving shifts
        x = round(float(x), 4)
        p[f"meter_{x}"] = dict(att_meter=x)
        p[f"meter_-{x}"] = dict(att_meter=-x)
        p[f"shift_{x}"] = dict(att_shift=x)
        p[f"shift_-{x}"] = dict(att_shift=-x)
    return p
