"""Figures that explain the R8 method table (README 5.1): the size of R8's zero-effect sample, and how the meter-based
estimate of the first full grid (v2.0) fared on meter manipulation (M9-2). -> results/method_note.json"""
import json
import math
import os

import grids

RESULTS = os.environ.get("SAVESAIL_RESULTS", os.path.join(os.path.dirname(__file__), "results"))
ADOPTED = list(grids.METHODS)[-1]


def main():
    out = {}
    j = os.path.join(RESULTS, "judgement.json")
    if os.path.exists(j):
        r8 = json.load(open(j)).get("r8", {})
        n0 = sum(1 for c in grids.grid_R8() if c["method"] == ADOPTED and c["s"] == 0) * grids.REPS["R8"]
        fp = r8.get(ADOPTED, {}).get("false_pay")
        out["r8"] = dict(method=ADOPTED, effect0_runs=n0, false_pay=fp,
                         false_pay_se=math.sqrt(fp * (1 - fp) / n0) if fp is not None else None)
    v20 = os.path.join(RESULTS, "v2.0", "judgement.json")
    if os.path.exists(v20):
        rows = [x for x in json.load(open(v20))["rows"] if "M9-2" in x["checks"]]
        out["v2_0_meter_M9_2"] = dict(of=len(rows), fail=sum(not x["checks"]["M9-2"] for x in rows))
    json.dump(out, open(os.path.join(RESULTS, "method_note.json"), "w"), indent=1)
    print(out)


if __name__ == "__main__":
    main()
