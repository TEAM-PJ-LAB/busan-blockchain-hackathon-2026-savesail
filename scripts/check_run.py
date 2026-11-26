#!/usr/bin/env python3
"""PRD 7.4 acceptance 2 / G1: recompute the run's chain state with the reference implementation
(offchain/reference/savesail_ref.py) from the attested payloads and the schedule, and compare to the
base unit. Usage: python3 scripts/check_run.py RUN_JSON [scenario|quick]"""
import json
import os
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, "offchain", "reference"))
import savesail_ref as ref  # noqa: E402

ACC = 10 ** 12


def main(run_path, which="scenario"):
    run = json.load(open(run_path))
    sc = json.load(open(os.path.join(ROOT, "scripts/config/demo.json")))[which]
    gpp = sc["groups_per_period"]
    caps = [int(x) for x in sc["caps"]]
    d = ref.Deal(phi_bps=int(sc["phi_bps"]), ref_rate_bps=600, comp_min_bps=9500, miss_max_bps=50, opex=int(sc["opex_per_period"]),
                 senior_sched=[int(sc["senior_per_period"])] * sc["senior_periods"], reserve_target=int(sc["reserve_target"]),
                 senior_units=caps[0], junior_units=caps[1])
    sched = run["schedule"]
    slash, bond = int(sc["slash"]), int(sc["bond"])
    bond_left = [bond, bond, bond]
    fails = []
    for per in run["periods"]:
        p, pl = per["period"], per["payload"]
        miss = [0, 0, 0]
        for g in range(p * gpp, (p + 1) * gpp):
            for r in range(3):
                if sched[g]["missing"] >> r & 1:
                    miss[r] += 1
                    s = min(slash, bond_left[r])
                    bond_left[r] -= s
                    d.reserve += s
        a = dict(lb=pl["lb_bps"], est=pl["est_bps"], ub=pl["ub_bps"], kg_on=pl["kg_on"], kg_miss_payer=pl["kg_miss_payer"],
                 kg_miss_inv=pl["kg_miss_inv"], price=pl["price_micro_per_t"], comp=pl["compliance_bps"], flags=pl["flags"],
                 override=pl["override_groups"])
        due = d.finalize(a, miss[0], miss[1], gpp, ver_miss_groups=miss[2])
        if due != int(per["due"]):
            fails.append(f"period {p}: due {per['due']} != reference {due}")
        if d.E != int(per["entitlement"]):
            fails.append(f"period {p}: entitlement {per['entitlement']} != reference {d.E}")
        d.settle(due)
        x = d.distribute(p)
        want = [x["opex"], x["senior"], x["reserve_draw"], x["reserve_in"], x["junior"]]
        if [int(v) for v in per["dist"]] != want:
            fails.append(f"period {p}: dist {per['dist']} != reference {want}")
    deal = run["deal"]
    for k, v in (("entitlement", d.E), ("paid_cum", d.paid), ("reserve", d.reserve), ("opex_arrears", d.opex_arr),
                 ("senior_arrears", d.sen_arr)):
        if int(deal[k]) != v:
            fails.append(f"deal {k}: {deal[k]} != reference {v}")
    if [int(x) for x in deal["acc"]] != [d.acc[0], d.acc[1]]:
        fails.append(f"deal acc {deal['acc']} != reference {[d.acc[0], d.acc[1]]}")
    # vault balances = distributed - claims (claims are units x acc / ACC per position)
    rl = int(sc["retail_limit"])
    units0 = [rl, rl, caps[0] - 2 * rl]
    claims0 = sum(u * d.acc[0] // ACC for u in units0)
    claims1 = caps[1] * d.acc[1] // ACC
    bal = run["balances"]
    for k, v in (("senior_vault", d.vault[0] - claims0), ("junior_vault", d.vault[1] - claims1), ("reserve_vault", d.reserve),
                 ("collection_vault", 0), ("bond_vault", 0)):
        if int(bal[k]) != v:
            fails.append(f"balance {k}: {bal[k]} != reference {v}")
    for f in fails:
        print("FAIL", f)
    print(f"checked {len(run['periods'])} periods, {run['tx_count']} transactions: {'OK' if not fails else 'MISMATCH'}")
    return 1 if fails else 0


if __name__ == "__main__":
    sys.exit(main(*sys.argv[1:]))
