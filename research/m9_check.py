"""M9-3 failures against the information bound: for every grid condition that fails M9-3, the best detection any
test can reach at its meter noise, campaign hours and missing share (m9_bound.best_power). -> results/m9_check.json"""
import json
import os

import m9_bound as mb

RESULTS = os.environ.get("SAVESAIL_RESULTS", os.path.join(os.path.dirname(__file__), "results"))


def main():
    path = os.path.join(RESULTS, "judgement.json")
    if not os.path.exists(path):  # a quick run of R8 alone writes no judgement
        print("no judgement.json: nothing to check")
        return
    j = json.load(open(path))
    rows = [x for x in j["rows"] if x["grid"] != "R8" and "M9-3" in x["checks"]]

    def bound(x):
        c = x["cfg"]
        return mb.best_power(c.get("sd_m", mb.m.BASE["sd_m"]), c.get("camp_h", mb.m.BASE["camp_h"]),
                             c.get("miss", mb.m.BASE["miss"]))

    fails = [x for x in rows if not x["checks"]["M9-3"]]
    fb = [bound(x) for x in fails]
    passes_below = sum(bound(x) < 0.99 for x in rows if x["checks"]["M9-3"])
    out = dict(conditions=len(rows), failing=len(fails), failing_below_bound=sum(b < 0.99 for b in fb),
               max_bound_failing=max(fb) if fb else None, passing_below_bound=passes_below,
               failing_list=[dict(grid=x["grid"], cond=x["cond"], bound=b) for x, b in zip(fails, fb)])
    json.dump(out, open(os.path.join(RESULTS, "m9_check.json"), "w"), indent=1)
    print({k: v for k, v in out.items() if k != "failing_list"})


if __name__ == "__main__":
    main()
