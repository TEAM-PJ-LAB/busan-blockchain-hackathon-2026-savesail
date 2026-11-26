"""Upper bound on fuel-meter manipulation detection (PRD M9-3), independent of the test used.

A manipulation that scales the meter by (1 + d) in OFF blocks only shows up as a shift d in the mean of
log(meter / shaft-implied fuel) between OFF and ON hours. With hourly noise sigma = sqrt(sd_m^2 + sd_q^2 +
sd_eta^2), n_off OFF hours and n_on ON hours, the best two-sided test at false-alarm rate a has power
Phi(|d| / SE - z_a/2) with SE = sigma * sqrt(1/n_off + 1/n_on) (Neyman-Pearson; any real test does worse).
Writes results/m9_bound.json: the best possible detection rate of a 1% change over the PRD 1.2 range.
"""
import json
import os

import numpy as np
from scipy.stats import norm

import measure as m

RESULTS = os.environ.get("SAVESAIL_RESULTS", os.path.join(os.path.dirname(os.path.abspath(__file__)), "results"))


def best_power(sd_m, hours, miss, d=0.01, alpha=0.0027, c=m.BASE):
    sigma = np.sqrt(sd_m ** 2 + c["sd_q"] ** 2 + c["sd_eta"] ** 2)
    w = 1 - c["trans"] / c["L"]  # transient-weighted share of each block
    n_off = hours / c["G"] * (1 - miss) * w
    n_on = n_off * (c["G"] - 1)
    se = sigma * np.sqrt(1 / n_off + 1 / n_on)
    return float(norm.cdf(abs(d) / se - norm.ppf(1 - alpha / 2)))


def main():
    rows = []
    for sd_m in (0.005, 0.01, 0.02, 0.03, 0.04, 0.05):
        for hours in (1800, 3600, 7200):
            for miss in (0.0, 0.15):
                rows.append(dict(sd_m=sd_m, hours=hours, miss=miss, best_detection_1pct=best_power(sd_m, hours, miss)))
    out = dict(alpha=0.0027, note="two-sided z = 3 false-alarm rate; requirement M9-3 is >= 99%", rows=rows)
    json.dump(out, open(os.path.join(RESULTS, "m9_bound.json"), "w"), indent=1)
    for r in rows:
        print(f"meter noise {100 * r['sd_m']:.1f}%  {r['hours']} h  missing {100 * r['miss']:.0f}%  best detection {100 * r['best_detection_1pct']:.1f}%")


if __name__ == "__main__":
    main()
