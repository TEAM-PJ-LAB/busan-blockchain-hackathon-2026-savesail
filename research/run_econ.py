"""Economics experiments R1, R5, R6 and criteria F1-F5 (PRD §1.4, §8). Writes results/econ.json and .md."""
import json

import econ

PRICES = ["flat750", "drop600", "drop460", "ou", "spike"]
SETTLES = ["ideal", "period", "trueup"]
TERMS = [7, 10, 12]
README_A = {7: (0.95, 12.9, 271.0), 10: (0.75, 11.8, 369.0)}  # README 5.2 result A, "750 유지" rows


def r2(d):
    return {k: (round(float(v), 4) if isinstance(v, float) or hasattr(v, "dtype") else v) for k, v in d.items()}


def main():
    out = dict(phi={}, table=[], checks={})
    # R1: phi per settlement mode and term (junior median IRR 12% on the flat 750 path), then every price path
    for st in SETTLES:
        for T in TERMS:
            phi = round(econ.solve_phi(dict(T=T, settle=st)), 3)
            out["phi"][f"{st}_{T}"] = phi
            for pr in PRICES:
                r = econ.simulate(dict(T=T, settle=st, price=pr), phi)
                out["table"].append(r2(dict(settle=st, T=T, price=pr, **r)))
    phi10 = out["phi"]["trueup_10"]
    rows = {(x["settle"], x["T"], x["price"]): x for x in out["table"]}
    ck = out["checks"]
    ck["F1"] = dict(phi=phi10, ok=phi10 <= 0.95)
    f2 = {pr: rows[("trueup", 10, pr)]["senior_loss"] for pr in ("flat750", "drop460", "ou")}
    ck["F2"] = dict(senior_loss=f2, ok=f2["flat750"] <= 0.1 and f2["drop460"] <= 0.5 and f2["ou"] <= 0.5)
    worst_loss = max(x["payer_loss"] for x in out["table"])
    min_npv = min(x["payer_npv_mean"] for x in out["table"])
    ck["F3"] = dict(max_payer_loss=worst_loss, min_payer_npv_mean=min_npv, ok=worst_loss == 0 and min_npv > 0)
    over = [x for x in out["table"] if x["settle"] == "trueup"]
    # R5: overpayment and end-of-term refund over every true-up path
    tot_over, tot_ref = 0.0, 0.0
    for x in over:
        r = econ.simulate(dict(T=x["T"], settle="trueup", price=x["price"]), out["phi"][f"trueup_{x['T']}"], detail=True)
        tot_over += r["overpaid_sum"]
        tot_ref += r["refund_sum"]
    ratio = tot_ref / tot_over if tot_over > 0 else None
    ck["F4"] = dict(overpaid_sum=tot_over, refund_sum=tot_ref, ratio=ratio, ok=ratio is None or ratio >= 0.95)
    f5 = {}
    ok5 = True
    for T, (phi, irr, npv) in README_A.items():
        r = econ.simulate(dict(T=T, settle="ideal", price="flat750"), phi)
        d_irr, d_npv = r["junior_irr_med"] - irr, r["payer_npv_mean"] / npv - 1
        f5[T] = dict(irr=r["junior_irr_med"], readme_irr=irr, npv=r["payer_npv_mean"], readme_npv=npv)
        ok5 &= abs(d_irr) <= 0.5 and abs(d_npv) <= 0.05
    ck["F5"] = dict(rows=f5, ok=ok5)
    # R6: README result B (7 years, phi fixed at 0.9, senior sized at 750 / 500 / 460)
    out["R6"] = []
    for pc in (750.0, 500.0, 460.0):
        a = econ.simulate(dict(T=7, settle="ideal", price="flat750", p_cons=pc), 0.9)
        b = econ.simulate(dict(T=7, settle="ideal", price="drop460", p_cons=pc), 0.9)
        out["R6"].append(dict(p_cons=pc, senior_share=float(a["senior_share"]), loss_flat=a["senior_loss"],
                              loss_drop460=b["senior_loss"]))
    json.dump(out, open("results/econ.json", "w"), indent=1, default=float)
    lines = ["| 정산 | 기간 | φ | 연료가 | 선순위 비중 | 선순위 손실 | 후순위 IRR 중앙값 | 후순위 IRR 10% | 지급자 NPV 평균(만 달러) | 지급자 손실 |",
             "|---|---|---|---|---|---|---|---|---|---|"]
    for x in out["table"]:
        lines.append(f"| {x['settle']} | {x['T']} | {out['phi'][x['settle'] + '_' + str(x['T'])]:.3f} | {x['price']} | "
                     f"{x['senior_share']:.1f}% | {x['senior_loss']:.2f}% | {x['junior_irr_med']:.2f}% | "
                     f"{x['junior_irr_p10']:.2f}% | {x['payer_npv_mean']:.1f} | {x['payer_loss']:.2f}% |")
    lines.append("")
    for k, v in ck.items():
        lines.append(f"- {k}: {'통과' if v['ok'] else '실패'} {json.dumps({a: b for a, b in v.items() if a != 'ok'}, default=float)}")
    open("results/econ.md", "w").write("\n".join(lines) + "\n")
    print("\n".join(lines[-6:]))


if __name__ == "__main__":
    main()
