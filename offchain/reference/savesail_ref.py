"""SaveSail reference model (docs/PRD.md §6). Rust program must match these outputs bit-for-bit.

Run: python3 -I offchain/reference/savesail_ref.py   (prints test vectors, asserts invariants)
"""
from hashlib import sha256

BPS = 10_000
ACC = 10**12  # accumulator scale
NO_OFF = 7    # off_index for all-ON (missing) group


def h(*parts: bytes) -> bytes:
    return sha256(b"".join(parts)).digest()


# ---------- schedule (§6.4) ----------
def chain(seed: bytes, n: int) -> list[bytes]:
    """c[0]=seed, c[i]=H^i(seed). anchor=c[n]; group g reveals c[n-1-g]."""
    c = [seed]
    for _ in range(n):
        c.append(h(c[-1]))
    return c


def verify_reveal(last: bytes, value: bytes, k: int, max_skip: int = 8) -> bool:
    if not 1 <= k <= max_skip:
        return False
    x = value
    for _ in range(k):
        x = h(x)
    return x == last


def off_index(campaign: bytes, group: int, v: tuple[bytes, bytes, bytes]) -> int:
    d = h(b"SAVESAIL_OFF_V1", campaign, group.to_bytes(4, "little"), *v)
    return int.from_bytes(d[:8], "little") % 5


# ---------- data commitment (§7.3) ----------
def merkle_root(lines: list[bytes]) -> bytes:
    lvl = [h(b"\x00", ln) for ln in lines] or [h(b"\x00")]
    while len(lvl) > 1:
        if len(lvl) % 2:
            lvl.append(lvl[-1])
        lvl = [h(b"\x01", lvl[i], lvl[i + 1]) for i in range(0, len(lvl), 2)]
    return lvl[0]


def acc_next(acc: bytes, group: int, root: bytes) -> bytes:
    return h(acc, group.to_bytes(4, "little"), root)


# ---------- settlement (§6.5) ----------
def value_micro(kg: int, price_micro_per_t: int) -> int:
    return kg * price_micro_per_t // 1000


def g_num(w: int, rate_bps: int) -> int:
    """saved value = ON value * r/(1-r); rate clamped to >=0."""
    r = max(0, rate_bps)
    return w * r // (BPS - r)


class Deal:
    def __init__(s, phi_bps, ref_rate_bps, comp_min_bps, miss_max_bps, opex, senior_sched,
                 reserve_target, senior_units, junior_units):
        s.phi, s.ref, s.comp_min, s.miss_max = phi_bps, ref_rate_bps, comp_min_bps, miss_max_bps
        s.opex, s.sched, s.res_target = opex, senior_sched, reserve_target
        s.units = {0: senior_units, 1: junior_units}
        s.w_norm = s.sv_fixed = s.E = s.paid = 0
        s.last_lb = None
        s.collection = s.reserve = s.opex_out = 0
        s.vault = {0: 0, 1: 0}
        s.acc = {0: 0, 1: 0}
        s.opex_arr = s.sen_arr = 0
        s.received_total = 0

    def finalize(s, a: dict, payer_miss_groups: int, inv_miss_groups: int, groups: int,
                 ver_miss_groups: int = 0) -> int:
        """a: attested payload (flags optional). Returns due for the period (PRD 6.5).
        payer violation  -> period ON fuel fixed at max(ub, ref)   (adverse to payer)
        investor over-cap -> period ON fuel valued 0              (adverse to investors)
        out of envelope or verifier over-cap -> fuel joins w_norm, bound not updated (C15, C18)
        a["override"] = safety-override groups below the weather threshold: counted as payer-missing
        groups, their ON fuel is in kg_miss_payer (C17). Overrides above the threshold are in kg_on.
        precedence: payer violation > investor over-cap > out of envelope / verifier over-cap > normal"""
        assert -BPS < a["lb"] <= a["est"] <= a["ub"] < BPS // 2
        flags = a.get("flags", 0)
        assert flags & ~0b1111 == 0
        price = a["price"]
        ov = a.get("override", 0)
        assert 0 <= ov <= groups and payer_miss_groups + inv_miss_groups + ver_miss_groups + ov <= groups
        assert payer_miss_groups + ov > 0 or a["kg_miss_payer"] == 0
        assert inv_miss_groups > 0 or a["kg_miss_inv"] == 0
        payer_viol = (a["comp"] < s.comp_min or (payer_miss_groups + ov) * BPS > s.miss_max * groups
                      or flags & 0b1011 != 0)  # speed, meter, data quality
        inv_viol = inv_miss_groups * BPS > s.miss_max * groups
        ver_over = ver_miss_groups * BPS > s.miss_max * groups
        w = value_micro(a["kg_on"], price)
        if payer_viol:
            s.sv_fixed += g_num(w, max(a["ub"], s.ref))
        elif inv_viol:
            pass
        elif flags & 0b100 or ver_over:
            s.w_norm += w
        else:
            s.w_norm += w
            s.last_lb = a["lb"]
        s.sv_fixed += g_num(value_micro(a["kg_miss_payer"], price), a["ub"])  # adverse to payer
        # kg_miss_inv contributes 0 (adverse to investors)
        lb = s.last_lb if s.last_lb is not None else 0
        s.E = (g_num(s.w_norm, lb) + s.sv_fixed) * s.phi // BPS
        return max(0, s.E - s.paid)

    def settle(s, due: int):
        s.paid += due
        s.collection += due
        s.received_total += due

    def distribute(s, p: int) -> dict:
        av = s.collection
        o_due = s.opex + s.opex_arr
        o = min(av, o_due); av -= o; s.opex_arr = o_due - o; s.opex_out += o
        s_due = s.sched[p] + s.sen_arr
        sp = min(av, s_due); av -= sp
        draw = min(s.reserve, s_due - sp); s.reserve -= draw; sp += draw
        s.sen_arr = s_due - sp
        r = min(av, max(0, s.res_target - s.reserve)); av -= r; s.reserve += r
        j = av
        for t, amt in ((0, sp), (1, j)):
            s.vault[t] += amt
            s.acc[t] += amt * ACC // s.units[t]
        s.collection = 0
        return dict(opex=o, senior=sp, reserve_draw=draw, reserve_in=r, junior=j,
                    opex_arr=s.opex_arr, sen_arr=s.sen_arr)

    def claimable(s, t: int, units: int, snap: int) -> int:
        return units * (s.acc[t] - snap) // ACC

    def close(s) -> dict:
        """order: payer refund of overpayment -> opex arrears -> senior arrears -> junior."""
        refund = min(max(0, s.paid - s.E), s.reserve); s.reserve -= refund
        o = min(s.opex_arr, s.reserve); s.reserve -= o; s.opex_arr -= o; s.opex_out += o
        sp = min(s.sen_arr, s.reserve); s.reserve -= sp; s.sen_arr -= sp
        s.vault[0] += sp; s.acc[0] += sp * ACC // s.units[0]
        j = s.reserve; s.reserve = 0
        s.vault[1] += j; s.acc[1] += j * ACC // s.units[1]
        return dict(refund=refund, opex_arr_paid=o, sen_arr_paid=sp, reserve_to_junior=j)

def annuity(principal: int, rate_bps_per_period: int, n: int) -> int:
    """level payment (floor search). Used by the config generator; the program takes the schedule as input."""
    lo, hi = 0, 2 * principal
    while lo < hi:  # smallest payment that amortizes principal within n periods
        m = (lo + hi) // 2
        bal = principal
        for _ in range(n):
            bal = bal + bal * rate_bps_per_period // BPS - m
        lo, hi = (m + 1, hi) if bal > 0 else (lo, m)
    return lo


def main():
    # --- schedule vectors ---
    camp = bytes(32)
    seeds = [h(b"payer"), h(b"investor_rep"), h(b"verifier")]
    n = 120
    ch = [chain(sd, n) for sd in seeds]
    anchors = [c[n] for c in ch]
    offs = []
    for g in range(n):
        v = tuple(c[n - 1 - g] for c in ch)
        last = [c[n - g] for c in ch]
        assert all(verify_reveal(last[i], v[i], 1) for i in range(3))
        offs.append(off_index(camp, g, v))
    assert verify_reveal(ch[0][n - 1], ch[0][n - 3], 2)           # skip after a missed group
    assert not verify_reveal(ch[0][n - 1], ch[0][n - 3], 1)      # wrong k rejected
    print("anchors     ", [a.hex()[:16] for a in anchors])
    print("off[0..9]   ", offs[:10], " counts", [offs.count(i) for i in range(5)])

    # uniformity over 10k groups (same function, fresh values)
    cnt = [0] * 5
    for g in range(10_000):
        cnt[off_index(camp, g, (h(b"a", g.to_bytes(4, "little")), h(b"b"), h(b"c")))] += 1
    chi2 = sum((c - 2000) ** 2 / 2000 for c in cnt)
    assert chi2 < 13.28, chi2  # df=4, p=0.01
    print("uniform 10k ", cnt, f"chi2={chi2:.2f}")

    # merkle / acc vectors
    lines = [b"0,1,14.2,11.8,812.5", b"1,1,14.1,11.8,809.9", b"2,1,14.3,11.8,815.0"]
    root = merkle_root(lines)
    print("merkle3     ", root.hex())
    print("acc(0,g0)   ", acc_next(bytes(32), 0, root).hex())

    # --- settlement & waterfall (README base case scaled to quarters) ---
    install = 4_000_000_000_000           # 4,000,000 dUSD (6 dp)
    senior = install * 1840 // BPS        # 18.4%
    junior = install - senior
    sched = [annuity(senior, 125, 40)] * 40   # 5%/yr quarterly, 10 years
    d = Deal(phi_bps=9000, ref_rate_bps=600, comp_min_bps=9500, miss_max_bps=500,
             opex=20_000_000_000, senior_sched=sched, reserve_target=2 * sched[0],
             senior_units=senior, junior_units=junior)
    price = 750_000_000                   # 750 USD/t in micro-USD
    pay = [  # (payload, payer_miss_groups, groups)
        (dict(kg_on=3_980_000, kg_miss_payer=0, kg_miss_inv=0, price=price, lb=520, est=690, ub=860, comp=10000), 0, 30),
        (dict(kg_on=3_850_000, kg_miss_payer=130_000, kg_miss_inv=0, price=price, lb=600, est=700, ub=800, comp=10000), 1, 30),
        (dict(kg_on=3_900_000, kg_miss_payer=0, kg_miss_inv=0, price=price, lb=580, est=650, ub=720, comp=9000), 0, 30),  # violation
        (dict(kg_on=3_970_000, kg_miss_payer=0, kg_miss_inv=0, price=price, lb=640, est=700, ub=760, comp=10000), 0, 30),
    ]
    for p, (a, pm, gp) in enumerate(pay):
        due = d.finalize(a, pm, 0, gp)
        d.settle(due)
        r = d.distribute(p)
        print(f"P{p} due={due:>13,} E={d.E:>13,} | " + " ".join(f"{k}={v:,}" for k, v in r.items()))
        out = d.opex_out + d.vault[0] + d.vault[1] + d.reserve + d.collection
        assert out == d.received_total, (out, d.received_total)       # conservation
    s_claim = d.claimable(0, senior, 0); j_claim = d.claimable(1, junior, 0)
    assert s_claim <= d.vault[0] and j_claim <= d.vault[1]
    print("senior_due/q", f"{sched[0]:,}", "claims", f"{s_claim:,}", f"{j_claim:,}",
          "dust", d.vault[0] - s_claim, d.vault[1] - j_claim)

    # close: LB falls in the last period -> E < paid -> refund from reserve, rest to junior
    c = Deal(phi_bps=9000, ref_rate_bps=600, comp_min_bps=9500, miss_max_bps=500,
             opex=20_000_000_000, senior_sched=[sched[0]] * 2, reserve_target=2 * sched[0],
             senior_units=senior, junior_units=junior)
    base = dict(kg_on=3_980_000, kg_miss_payer=0, kg_miss_inv=0, price=price, comp=10000)
    for p, (lb, est, ub) in enumerate([(800, 900, 1000), (300, 500, 700)]):
        due = c.finalize(dict(base, lb=lb, est=est, ub=ub), 0, 0, 30)
        c.settle(due)
        r = c.distribute(p)
        print(f"C{p} due={due:>13,} E={c.E:>13,} paid={c.paid:,} | " + " ".join(f"{k}={v:,}" for k, v in r.items()))
    over, res_before = c.paid - c.E, c.reserve
    res = c.close()
    assert res["refund"] == min(over, res_before) and c.reserve == 0
    out = c.opex_out + c.vault[0] + c.vault[1] + c.reserve + c.collection + res["refund"]
    assert out == c.received_total, (out, c.received_total)
    print("close       ", res, "overpaid", f"{over:,}")

    # A.5 out-of-envelope period then a valid one; investor over-cap period
    mk = lambda: Deal(phi_bps=9000, ref_rate_bps=600, comp_min_bps=9500, miss_max_bps=50,
                      opex=0, senior_sched=[0] * 3, reserve_target=0, senior_units=1, junior_units=1)
    q = mk()
    d0 = q.finalize(dict(base, lb=410, est=600, ub=790, flags=0b100), 0, 0, 30)
    q.settle(d0)
    d1 = q.finalize(dict(base, lb=600, est=700, ub=800), 0, 0, 30)
    print(f"A5 envelope: due0={d0:,} due1={d1:,} E={q.E:,} last_lb={q.last_lb}")
    assert d0 == 0 and q.E == (2 * value_micro(3_980_000, price)) * 600 // 9400 * 9000 // BPS
    r = mk()
    e0 = r.finalize(dict(base, lb=600, est=700, ub=800), 0, 0, 30)
    r.settle(e0)
    e1 = r.finalize(dict(base, lb=650, est=750, ub=850), 0, 1, 30)  # investor withholds 1/30 > 0.5%
    print(f"A5 investor over-cap: due0={e0:,} due1={e1:,} E={r.E:,} last_lb={r.last_lb}")
    assert e1 == 0 and r.last_lb == 600
    t = mk()
    f0 = t.finalize(dict(base, lb=600, est=700, ub=800, flags=0b1000), 0, 0, 30)  # payer data quality
    print(f"A5 data quality: due0={f0:,} last_lb={t.last_lb}")
    assert f0 == value_micro(3_980_000, price) * 800 // 9200 * 9000 // BPS and t.last_lb is None
    u = mk()  # safety override within cap: 1 of 300 groups (C17)
    h0 = u.finalize(dict(base, lb=600, est=700, ub=800, override=1, kg_miss_payer=13_000), 0, 0, 300)
    print(f"A5 override in-cap: due0={h0:,} last_lb={u.last_lb}")
    assert h0 == (value_micro(3_980_000, price) * 600 // 9400
                  + value_micro(13_000, price) * 800 // 9200) * 9000 // BPS and u.last_lb == 600
    v = mk()  # safety override over cap: 1 of 30 groups -> payer violation
    k0 = v.finalize(dict(base, lb=600, est=700, ub=800, override=1, kg_miss_payer=13_000), 0, 0, 30)
    print(f"A5 override over-cap: due0={k0:,} last_lb={v.last_lb}")
    assert k0 == (value_micro(3_980_000, price) * 800 // 9200
                  + value_micro(13_000, price) * 800 // 9200) * 9000 // BPS and v.last_lb is None
    x = mk()  # verifier withholds 1/30 > 0.5% in period 2: bound not updated (C18)
    m0 = x.finalize(dict(base, lb=600, est=700, ub=800), 0, 0, 30)
    x.settle(m0)
    m1 = x.finalize(dict(base, lb=650, est=750, ub=850), 0, 0, 30, ver_miss_groups=1)
    print(f"A5 verifier over-cap: due0={m0:,} due1={m1:,} E={x.E:,} last_lb={x.last_lb}")
    assert x.E == (2 * value_micro(3_980_000, price)) * 600 // 9400 * 9000 // BPS
    assert m1 == x.E - m0 and x.last_lb == 600

if __name__ == "__main__":
    main()
