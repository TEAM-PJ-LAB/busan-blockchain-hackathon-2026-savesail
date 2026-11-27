"""SaveSail verifier (PRD §7.2): period attestation payload + report from committed data.

Statistics are research/measure.py's functions, called on the committed (rounded) data, so the
verifier and the simulator are one implementation.

CLI: python3 verifier.py DATA_DIR CHAIN.json ENVELOPE.json OUT_DIR
  CHAIN.json: {"period": p, "groups_per_period": 60, "status": [byte per group], "snap_ops": hex,
               "snap_ctrl": hex, "prev_outcomes": [o0, ...], "price_micro_per_t": 750000000}
  OUT_DIR/payload_<p>.json (AttestationPayload fields), OUT_DIR/report_<p>.json (report_hash preimage)
"""
import csv
import hashlib
import json
import os
import sys

import numpy as np
from cryptography.exceptions import InvalidSignature
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PublicKey

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "..", "..", "research"))
sys.path.insert(0, os.path.join(HERE, "..", "synth"))
import measure as m  # noqa: E402
from synth import DRAFT_REF_M, NO_OFF, merkle_root  # noqa: E402

FORMAT_VERSION = 1
MIN_TEST_GROUPS = 30
OUT_PAYER, OUT_INV = 1, 2
REQUIRED = ("speed_kn", "draft_m", "wind_ms", "wind_rel_deg", "hs_m", "shaft_kw", "fuel_kg")


def read_csv(path):
    with open(path) as f:
        rows = list(csv.reader(f))
    return rows[0], rows[1:]


def lines_of(path):
    with open(path) as f:
        return f.read().splitlines()[1:]


def cols(header, rows, names, dtype=float):
    ix = [header.index(n) for n in names]
    return [np.array([dtype(r[i]) for r in rows]) for i in ix]


def acc_next(acc: bytes, group: int, root: bytes) -> bytes:
    return hashlib.sha256(acc + group.to_bytes(4, "little") + root).digest()


def status_unpack(b):
    return b & 0x07, (b >> 3) & 0x07, bool(b & 0x80)


class Verifier:
    def __init__(self, data_dir, envelope):
        self.dir = data_dir
        self.env = envelope
        self.c = dict(m.BASE, **envelope["model"])
        h, rows = read_csv(os.path.join(data_dir, "independent.csv"))
        hour, vind, uref, thref, href = cols(h, rows, ["hour", "speed_ind_kn", "wind_ms_ref", "wind_rel_deg_ref", "hs_m_ref"])
        self.ind = {int(t): (a, b, cc, d) for t, a, b, cc, d in zip(hour, vind, uref, thref, href)}
        self.key = Ed25519PublicKey.from_public_bytes(bytes.fromhex(envelope["meter_public_key"]))
        self._train()

    def ind_cols(self, hours):
        a = np.array([self.ind[int(t)] for t in hours])
        return a[:, 0], a[:, 1] * np.cos(np.radians(a[:, 2])), a[:, 3], a[:, 1], a[:, 2]

    # ------------------------------------------------------------ training: model, placebo inputs, SFOC curve
    def _train(self):
        c = self.c
        h, rows = read_csv(os.path.join(self.dir, "training.csv"))
        hour, draft, fuel, shaft = cols(h, rows, ["hour", "draft_m", "fuel_kg", "shaft_kw"])
        hour = hour.astype(int)
        self.train_hours = hour
        if len(hour) < self.env["min_training_h"] * (1 - self.env["max_missing"]):
            raise SystemExit("training data below the campaign precondition (PRD C16)")
        V, Uh, Hs, _, _ = self.ind_cols(hour)
        D = draft / DRAFT_REF_M
        idx = np.arange(len(hour))
        # engine shop-test curve (hash fixed in the envelope) and MCR -> fuel/shaft calibration (PRD C13, C22)
        path = os.path.join(self.dir, "engine_curve.csv")
        if self.env.get("engine_curve_sha256") and hashlib.sha256(open(path, "rb").read()).hexdigest() != self.env["engine_curve_sha256"]:
            raise SystemExit("engine curve differs from the envelope")
        eh, erows = read_csv(path)
        loads, vals = cols(eh, erows, ["load", "sfoc_rel"])
        self.shop = m.shop_curve(loads, vals)
        self.mcr = float(self.env["mcr_kw"])
        self.ab = m.calibrate_sfoc(fuel, shaft, self.mcr, idx, self.shop)
        F = self.est_fuel(fuel, shaft)
        y = np.log(F)
        self.kn, self.beta = m.fit(c["model"], V, D, Uh, Hs, y, idx)
        # cross-fitted out-of-sample predictions on the full training timeline (missing hours weight 0)
        t0 = int(self.env["train_h"])
        L, G, K = c["L"], c["G"], c["pcf_k"]
        A, P, wt = np.zeros(t0), np.ones(t0), np.zeros(t0)
        fold = np.minimum((np.arange(t0) // (L * G)) * K // max(t0 // (L * G), 1), K - 1)
        for k in range(K):
            ev = fold[hour] == k
            kn, b = m.fit(c["model"], V, D, Uh, Hs, y, np.flatnonzero(~ev))
            P[hour[ev]] = m.predict(c["model"], kn, b, V[ev], D[ev], Uh[ev], Hs[ev])
        A[hour] = F
        wt[hour] = np.where(hour % L == 0, 1.0 - c["trans"], 1.0)
        self.placebo = (A, P, wt)
        self.model_coef = [float(x) for x in self.beta]

    def est_fuel(self, fuel, shaft):
        """Fuel the estimator uses (C22): torque-meter shaft power through the calibrated SFOC curve."""
        return m.shaft_fuel(shaft, self.mcr, self.ab, self.shop) if self.c["fuel_src"] == "shaft" else fuel

    # ------------------------------------------------------------ one period
    def attest(self, chain):
        c, env = self.c, self.env
        p, gpp = int(chain["period"]), int(chain["groups_per_period"])
        status = chain["status"]
        L, G = c["L"], c["G"]
        prev = list(chain.get("prev_outcomes", []))
        rng_seed = int.from_bytes(hashlib.sha256(bytes.fromhex(chain["snap_ops"]) + p.to_bytes(2, "little")).digest()[:8],
                                  "little")
        # data commitments: recompute both accumulators and compare with the chain snapshot (acceptance 3)
        acc = [b"\x00" * 32, b"\x00" * 32]
        for g in range((p + 1) * gpp):
            for k, name in enumerate(("ops", "ctrl")):
                acc[k] = acc_next(acc[k], g, merkle_root(lines_of(os.path.join(self.dir, f"{name}_{g}.csv"))))
        if acc[0].hex() != chain["snap_ops"] or acc[1].hex() != chain["snap_ctrl"]:
            raise SystemExit("data accumulator does not match the chain snapshot; no payload (PRD 7.2-10 3)")

        groups = range((p + 1) * gpp)
        recs, excluded, report_groups, unpaired = [], {}, [], []
        meter_ok = True
        payer_miss = inv_miss = override_below = 0
        kg_on = kg_miss_payer = kg_miss_inv = 0.0
        off_blocks = off_ok = 0
        observed = expected = 0
        for g in groups:
            off, missing, resolved = status_unpack(status[g])
            per = g // gpp
            h, rows = read_csv(os.path.join(self.dir, f"ops_{g}.csv"))
            ch, crows = read_csv(os.path.join(self.dir, f"ctrl_{g}.csv"))
            if any(name not in h for name in REQUIRED):
                rows = []  # a required input column is missing: every hour of the group counts as missing (C16)
            else:
                ix = [h.index(name) for name in REQUIRED]
                rows = [r for r in rows if all(r[i].strip() for i in ix)]
                h = h
            meter = json.load(open(os.path.join(self.dir, f"meter_{g}.json")))
            digest = hashlib.sha256("\n".join(r[-1] for r in rows).encode()).digest()
            try:
                self.key.verify(bytes.fromhex(meter["meter_sig"]), digest)
                sig_ok = digest.hex() == meter["meter_digest"]
            except InvalidSignature:
                sig_ok = False
            if not rows:
                h = ["hour", "group", "block", "w", *REQUIRED]
            hour, blk, w, vlog, draft, uo, tho, hso, shaft, fuel = cols(
                h, rows, ["hour", "block", "w", "speed_kn", "draft_m", "wind_ms", "wind_rel_deg", "hs_m", "shaft_kw", "fuel_kg"])
            hour, blk = hour.astype(int), blk.astype(int)
            cmd = {int(r[0]): r[3] for r in crows}
            state = {int(r[0]): r[4] for r in crows}
            override = {int(r[0]): r[6] == "1" for r in crows}
            gh = L * G
            if per == p:
                expected += gh
                observed += len(rows)
                meter_ok &= sig_ok
            is_off = np.array([cmd[t] == "OFF" for t in hour], bool)
            on_state = np.array([state[t] == "ON" for t in hour], bool)
            # OFF compliance from the controller log (all hours, not only rows with data)
            viol = False
            if off != NO_OFF:
                off_hours = [t for t, v in cmd.items() if v == "OFF"]
                ok_blk = all(state[t] == "OFF" for t in off_hours)
                viol = not ok_blk
                if per == p:
                    off_blocks += 1
                    off_ok += ok_blk
            folded = [t for t, v in override.items() if v and cmd[t] == "ON"]
            storm = False
            if folded:
                V_, _, _, Ur, _ = self.ind_cols(np.array(folded))
                ship_wind = max(float(uo[hour == t][0]) for t in folded if (hour == t).any()) if any((hour == t).any() for t in folded) else 0.0
                storm = ship_wind >= env["storm_wind_ms"] and float(Ur.max()) >= env["storm_wind_ref_ms"]
            on_fuel_w = float((fuel * w * ~is_off).sum())
            # groups without a valid OFF block keep their commanded-ON hours for the estimate (C24)
            unp = dict(g=g, per=per, hour=hour, w=w, on=~is_off, D=draft / DRAFT_REF_M, shaft=shaft, fuel=fuel)
            if missing & 1:
                if per == p:
                    payer_miss += 1
                    kg_miss_payer += float((fuel * w).sum())
                excluded[g] = "payer_missing"
                unpaired.append(dict(unp, kind="payer"))
                continue
            if missing & 2:
                if per == p:
                    inv_miss += 1
                    kg_miss_inv += float((fuel * w).sum())
                excluded[g] = "investor_missing"
                unpaired.append(dict(unp, kind="investor"))
                continue
            if missing & 4:
                if per == p:
                    kg_on += float((fuel * w).sum())
                excluded[g] = "verifier_missing"
                unpaired.append(dict(unp, kind="verifier"))
                continue
            if folded and not storm:
                if per == p:
                    override_below += 1
                    kg_miss_payer += on_fuel_w
                excluded[g] = "override_below_threshold"
                unpaired.append(dict(unp, kind="payer"))
                continue
            if per == p:
                kg_on += on_fuel_w
            if folded:
                excluded[g] = "override_storm"
                continue
            if viol:
                excluded[g] = "off_violation"
                unpaired.append(dict(unp, kind="payer"))
                continue
            recs.append(dict(g=g, hour=hour, w=w, off=is_off, D=draft / DRAFT_REF_M, vlog=vlog, uo=uo, tho=tho, hso=hso,
                             shaft=shaft, fuel=fuel))

        # cumulative estimate over included groups (PRD 7.2-3/4)
        S, q_on, q_off, cov = [], [], [], {k: ([], []) for k in ("speed", "draft", "headwind", "wave", "speed_x", "wind_x",
                                                                    "angle_x", "wave_x", "fuel_shaft")}
        allv = []
        for r in recs:
            V, Uh, Hs, Uref, thref = self.ind_cols(r["hour"])
            P = m.predict(c["model"], self.kn, self.beta, V, r["D"], Uh, Hs)
            on, off = ~r["off"], r["off"]
            F = self.est_fuel(r["fuel"], r["shaft"])
            S.append([(F * r["w"] * on).sum(), (P * r["w"] * on).sum(), (F * r["w"] * off).sum(),
                      (P * r["w"] * off).sum()])
            r.update(V=V, Uref=Uref, thref=thref, Href=Hs, uho=r["uo"] * np.cos(np.radians(r["tho"])))
            allv.append(r)
        S = np.array(S).reshape(-1, 4)
        paired_g = [r["g"] for r, row in zip(recs, S) if row[1] > 0 and row[3] > 0]
        # each paired group's OFF-block log ratio, for the campaign noise level (C25)
        x_off = np.full((p + 1) * gpp, np.nan)
        for r, row in zip(recs, S):
            if row[1] > 0 and row[3] > 0:
                x_off[r["g"]] = np.log(row[2] / row[3])
        S = S[(S[:, 1] > 0) & (S[:, 3] > 0)]
        n_hours = float(sum(r["hour"].size for r in recs))

        # manipulation checks on the cumulative included data (C11, C13)
        flags = 0
        tests = {}
        # groups whose draft changes (voyage change inside the group) are left out of the balance tests
        same = [r for r in allv if r["D"].max() / r["D"].min() < 1.05]
        if len(same) >= MIN_TEST_GROUPS:  # balance tests need enough groups for a stable HAC standard error
            def gm(r, x):
                on, off = ~r["off"], r["off"]
                return (x * r["w"] * on).sum() / (r["w"] * on).sum(), (x * r["w"] * off).sum() / (r["w"] * off).sum()

            def test(name, xs, scale, floor, z):
                a = np.array([gm(r, x) for r, x in zip(same, xs)])
                D_, SE, hit = m.balance(a[:, 0], a[:, 1], scale, floor, z)
                tests[name] = dict(D=D_, SE=SE, flagged=hit)
                return hit

            z, fl = c["cov_z"], c["cov_floor"]
            vl = np.concatenate([r["vlog"] for r in same])
            hit = False
            hit |= test("speed", [r["vlog"] for r in same], vl.mean(), c["speed_floor"], c["speed_z"])
            hit |= test("draft", [r["D"] for r in same], np.concatenate([r["D"] for r in same]).mean(), 0.0, z)
            uo_mean = np.concatenate([r["uo"] for r in same]).mean()
            hit |= test("headwind", [r["uho"] for r in same], uo_mean, fl, z)
            hit |= test("wave", [r["hso"] for r in same], np.concatenate([r["hso"] for r in same]).mean(), fl, z)
            hit |= test("speed_x", [r["vlog"] - r["V"] for r in same], vl.mean(), 0.0, z)
            hit |= test("wind_x", [r["uo"] - r["Uref"] for r in same], uo_mean, 0.0, z)
            hit |= test("angle_x", [r["tho"] - r["thref"] for r in same], 90.0, 0.0, z)
            hit |= test("wave_x", [r["hso"] - r["Href"] for r in same], np.concatenate([r["hso"] for r in same]).mean(), 0.0, z)
            if hit:
                flags |= 1
            qs = [r["fuel"] / m.shaft_fuel(r["shaft"], self.mcr, self.ab, self.shop) for r in same]
            if test("fuel_shaft", qs, 1.0, c["q_floor"], c["q_z"]):
                flags |= 2
            # voyage mass balance (completed voyages with >= 200 campaign hours)
            vh, vrows = read_csv(os.path.join(self.dir, "voyages.csv"))
            sq = float(np.std(np.log(np.concatenate(qs))))
            mass = []
            for vr in vrows:
                d = dict(zip(vh, vr))
                if int(d["hours"]) < 200:
                    continue
                implied = float(d["bdn_kg"]) - (float(d["rob_end_kg"]) - float(d["rob_start_kg"]))
                tot = float(d["meter_total_kg"])
                sd_v = np.sqrt(sq ** 2 / int(d["hours"]) + c["bdn_sd"] ** 2)
                dev = abs(tot - implied) / tot
                bad = dev > max(c["mass_tol"], c["mass_z"] * sd_v)
                mass.append(dict(voyage=int(d["voyage"]), deviation=dev, flagged=bool(bad)))
                if bad:
                    flags |= 2
            tests["mass_balance"] = mass
        if not meter_ok:
            flags |= 2
        if n_hours < env["min_campaign_h"]:
            flags |= 4
        miss_rate = 1 - observed / max(expected, 1)
        if miss_rate > env["max_missing"]:
            flags |= 8

        # cumulative estimate (PRD 7.2-3/4, C24). Unpaired groups: within the party's limit they re-enter through
        # their neighbours' OFF ratio; groups a party left unpaired in a period where it is over its limit count at
        # that party's worst case (payer 100% saving, investor representative 0%), so refusing never pays (R4).
        miss_max, comp_min = int(chain.get("miss_max_bps", 50)), int(chain.get("comp_min_bps", 9500))
        compliance_now = 10_000 * off_ok // off_blocks if off_blocks else 10_000
        payer_over = {q for q in range(p) if q < len(prev) and prev[q] == OUT_PAYER}
        inv_over = {q for q in range(p) if q < len(prev) and prev[q] == OUT_INV}
        if ((payer_miss + override_below) * 10_000 > miss_max * gpp or compliance_now < comp_min
                or flags & 0b1011):
            payer_over.add(p)
        if inv_miss * 10_000 > miss_max * gpp:
            inv_over.add(p)
        ng_all = (p + 1) * gpp
        Sg = np.zeros((ng_all, 4))
        for r, row in zip([r for r in recs if r["g"] in set(paired_g)], S):
            Sg[r["g"]] = row
        paired = np.zeros(ng_all, bool)
        paired[paired_g] = True
        nb, adverse = np.zeros(ng_all, bool), []
        for u in unpaired:
            V, Uh, Hs, _, _ = self.ind_cols(u["hour"])
            P = m.predict(c["model"], self.kn, self.beta, V, u["D"], Uh, Hs)
            F = self.est_fuel(u["fuel"], u["shaft"])
            on_w = u["w"] * u["on"]
            row = [(F * on_w).sum(), (P * on_w).sum()]
            if row[1] <= 0:
                continue
            if u["kind"] == "payer" and u["per"] in payer_over:
                adverse.append((row[1], 1.0))
                excluded[u["g"]] += ":imputed_payer_worst_case"
            elif u["kind"] == "investor" and u["per"] in inv_over:
                adverse.append((row[1], 0.0))
                excluded[u["g"]] += ":imputed_investor_worst_case"
            else:
                Sg[u["g"], :2] = row
                nb[u["g"]] = True
                excluded[u["g"]] += ":neighbour_calibrated"
        S_est = np.vstack([S, m.neighbor_rows(Sg, paired, nb)]) if nb.any() else S
        r_hat = float(m.ratio(S_est.sum(0))) if len(S) else 0.0
        rng = np.random.default_rng(rng_seed)
        A, Pt, wt = self.placebo
        eps = m.placebo_eps(rng, A, Pt, wt, L, G, c["B"], max(len(S), 1))
        q_c = m.off_pair_q(x_off) if c["stud"] else None
        lb, ub = m.interval_from_eps(S_est, eps, len(S), q_c) if len(S) else (0.0, 0.0)
        if adverse and len(S):
            pv = float(S_est[:, 1].sum())
            pa, ra = sum(a for a, _ in adverse), sum(a * x for a, x in adverse)
            mix = lambda x: (pv * x + ra) / (pv + pa)  # noqa: E731
            r_hat, lb, ub = mix(r_hat), mix(lb), mix(ub)

        bps = lambda x: int(np.floor(x * 10_000))
        lb_b, est_b, ub_b = bps(lb), int(round(r_hat * 10_000)), int(np.ceil(ub * 10_000))
        est_b = min(max(est_b, lb_b), ub_b)
        ub_b = min(ub_b, 4999)
        lb_b = max(min(lb_b, est_b), -9999)
        payload = dict(
            period=p, data_root_ops=chain["snap_ops"], data_root_ctrl=chain["snap_ctrl"],
            kg_on=int(round(kg_on)), kg_miss_payer=int(round(kg_miss_payer)), kg_miss_inv=int(round(kg_miss_inv)),
            override_groups=override_below, price_micro_per_t=int(chain["price_micro_per_t"]),
            lb_bps=lb_b, est_bps=est_b, ub_bps=ub_b,
            compliance_bps=int(10_000 * off_ok // off_blocks) if off_blocks else 10_000,
            flags=flags, envelope_hash=envelope_hash(self.env))
        report = dict(
            format_version=FORMAT_VERSION, period=p, data_root_ops=chain["snap_ops"], data_root_ctrl=chain["snap_ctrl"],
            model=dict(kind=c["model"], coef=self.model_coef, knots={k: list(map(float, v)) for k, v in (self.kn or {}).items()}),
            groups=dict(included=len(S), neighbour_calibrated=int(nb.sum()), imputed=len(adverse), excluded=excluded,
                        hours=n_hours),
            sums=S.sum(0).tolist() if len(S) else [0, 0, 0, 0],
            interval=dict(method="cross-fitted placebo", folds=c["pcf_k"], draws=c["B"], seed=rng_seed,
                          campaign_q=list(q_c) if q_c else None, window_q=[w[3] for w in eps]),
            lb=lb, est=r_hat, ub=ub, compliance_bps=payload["compliance_bps"], tests=tests, meter_signatures_ok=meter_ok,
            missing_rate=miss_rate, flags=flags, payload=payload)
        rep_bytes = json.dumps(report, sort_keys=True, separators=(",", ":"), default=float).encode()
        payload["report_hash"] = hashlib.sha256(rep_bytes).hexdigest()
        return payload, rep_bytes


def envelope_hash(env):
    return hashlib.sha256(json.dumps(env, sort_keys=True, separators=(",", ":")).encode()).hexdigest()


def main(data_dir, chain_path, env_path, out_dir):
    env = json.load(open(env_path))
    chain = json.load(open(chain_path))
    v = Verifier(data_dir, env)
    payload, rep = v.attest(chain)
    os.makedirs(out_dir, exist_ok=True)
    p = payload["period"]
    open(os.path.join(out_dir, f"report_{p}.json"), "wb").write(rep)
    json.dump(payload, open(os.path.join(out_dir, f"payload_{p}.json"), "w"), indent=1)
    print(json.dumps(payload))


if __name__ == "__main__":
    if len(sys.argv) == 3 and sys.argv[1] == "hash":
        print(envelope_hash(json.load(open(sys.argv[2]))))
    elif len(sys.argv) == 5:
        main(*sys.argv[1:])
    else:
        sys.exit(__doc__)
