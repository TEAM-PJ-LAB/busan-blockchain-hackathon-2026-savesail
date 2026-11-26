"""T10 offchain acceptance (PRD §7.1 3, §7.2 14). Run: python3 -m pytest -q offchain/tests"""
import csv
import hashlib
import json
import os
import shutil
import sys
import time

import numpy as np
import pytest

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
sys.path[:0] = [os.path.join(ROOT, "offchain", "synth"), os.path.join(ROOT, "offchain", "verifier"), os.path.join(ROOT, "research")]
import measure as m  # noqa: E402
import verifier as V  # noqa: E402
from synth import CTRL_HEADER, OPS_HEADER, World, merkle_root, write  # noqa: E402

CFG = dict(ship="bulk", clim="trade", s=0.07, hetero=True, camp_h=3600, train_h=3600)
SEED = 4242
GPP = 60


def build(d, n_groups=240, viol=(), seed=SEED, cfg=CFG):
    wd = World(dict(cfg), seed)
    os.makedirs(d, exist_ok=True)
    write(f"{d}/training.csv", wd.training_lines())
    write(f"{d}/independent.csv", wd.independent_lines())
    rng = np.random.default_rng(seed)
    status = []
    for g in range(n_groups):
        off = int(rng.integers(0, 5))
        ops, ctrl, meter = wd.group(g, off, g in viol)
        write(f"{d}/ops_{g}.csv", [OPS_HEADER] + ops)
        write(f"{d}/ctrl_{g}.csv", [CTRL_HEADER] + ctrl)
        json.dump(meter, open(f"{d}/meter_{g}.json", "w"))
        status.append(0x80 | off)
    write(f"{d}/voyages.csv", wd.voyage_lines())
    env = json.load(open(os.path.join(ROOT, "offchain/verifier/envelope.template.json")))
    env["meter_public_key"] = wd.key.public_key().public_bytes_raw().hex()
    return wd, status, env


def chain_for(d, status, p, prev=None):
    acc = [b"\x00" * 32, b"\x00" * 32]
    for g in range((p + 1) * GPP):
        for k, n in enumerate(("ops", "ctrl")):
            acc[k] = V.acc_next(acc[k], g, merkle_root(V.lines_of(f"{d}/{n}_{g}.csv")))
    return dict(period=p, groups_per_period=GPP, status=status, snap_ops=acc[0].hex(), snap_ctrl=acc[1].hex(),
                prev_outcomes=prev or [0] * p, price_micro_per_t=750_000_000)


@pytest.fixture(scope="module")
def base(tmp_path_factory):
    d = str(tmp_path_factory.mktemp("base"))
    wd, status, env = build(d)
    return d, wd, status, env


def copy(base, tmp_path):
    d, wd, status, env = base
    dst = str(tmp_path / "data")
    shutil.copytree(d, dst)
    return dst, wd, list(status), dict(env)


def rewrite(path, fn):
    rows = list(csv.reader(open(path)))
    out = [rows[0]] + [fn(r) for r in rows[1:]]
    with open(path, "w") as f:
        f.write("\n".join(",".join(r) for r in out if r is not None) + "\n")


def resign(d, wd, g):
    rows = list(csv.reader(open(f"{d}/ops_{g}.csv")))[1:]
    dig = hashlib.sha256("\n".join(r[-1] for r in rows).encode()).digest()
    json.dump(dict(group=g, meter_digest=dig.hex(), meter_sig=wd.key.sign(dig).hex()), open(f"{d}/meter_{g}.json", "w"))


def attest(d, env, status, p=3, prev=None):
    return V.Verifier(d, env).attest(chain_for(d, status, p, prev))


# ------------------------------------------------------------------ 7.1 synth (3)
def test_synth_deterministic(tmp_path):
    digests = set()
    for i in range(5):
        d = str(tmp_path / f"r{i}")
        build(d, n_groups=20)
        h = hashlib.sha256()
        for f in sorted(os.listdir(d)):
            h.update(open(os.path.join(d, f), "rb").read())
        digests.add(h.hexdigest())
    assert len(digests) == 1


def test_synth_same_rule_as_measure():
    wd = World(dict(CFG), SEED)
    st = m.prepare(dict(m.BASE, **CFG), SEED)
    assert np.max(np.abs(wd.st["nos"] - st["nos"]) / st["nos"]) < 1e-9
    assert np.max(np.abs(wd.st["s_h"] - st["s_h"])) < 1e-9


def test_synth_speed(tmp_path):
    t = time.time()
    build(str(tmp_path / "d"), n_groups=240)
    assert time.time() - t < 10


# ------------------------------------------------------------------ 7.2 verifier (14)
def test_estimate_matches_measure_functions(base):
    """1: independent assembly from the CSVs with research/measure.py functions, equal within 1e-12."""
    d, wd, status, env = base
    pl, rep = attest(d, env, status)
    r = json.loads(rep)
    c = dict(m.BASE, **env["model"])
    ind = {int(x[0]): list(map(float, x[1:])) for x in list(csv.reader(open(f"{d}/independent.csv")))[1:]}
    tr = list(csv.reader(open(f"{d}/training.csv")))
    hdr = tr[0]
    T = np.array([[float(x) for x in row] for row in tr[1:]])
    col = {k: i for i, k in enumerate(hdr)}
    hrs = T[:, col["hour"]].astype(int)
    I = np.array([ind[h] for h in hrs])
    Vv, Uh, Hs = I[:, 0], I[:, 1] * np.cos(np.radians(I[:, 2])), I[:, 3]
    D = T[:, col["draft_m"]] / 18.0
    kn, beta = m.fit(c["model"], Vv, D, Uh, Hs, np.log(T[:, col["fuel_kg"]]), np.arange(len(hrs)))
    S = []
    for g in range(4 * GPP):
        rows = list(csv.reader(open(f"{d}/ops_{g}.csv")))[1:]
        ctrl = {int(x[0]): x[3] for x in list(csv.reader(open(f"{d}/ctrl_{g}.csv")))[1:]}
        A = np.array([[float(x) for x in row] for row in rows])
        h = A[:, 0].astype(int)
        J = np.array([ind[x] for x in h])
        P = m.predict(c["model"], kn, beta, J[:, 0], A[:, 5] / 18.0, J[:, 1] * np.cos(np.radians(J[:, 2])), J[:, 3])
        off = np.array([ctrl[x] == "OFF" for x in h])
        w, f = A[:, 3], A[:, 10]
        S.append([(f * w * ~off).sum(), (P * w * ~off).sum(), (f * w * off).sum(), (P * w * off).sum()])
    est = float(m.ratio(np.array(S).sum(0)))
    assert abs(est - r["est"]) < 1e-12


def test_deterministic_payload(base):
    """2 (and 8): same input -> same payload bytes, report hash and random seed."""
    d, wd, status, env = base
    a, ra = attest(d, env, status)
    b, rb = attest(d, env, status)
    assert a == b and ra == rb
    assert json.loads(ra)["interval"]["seed"] == json.loads(rb)["interval"]["seed"]


def test_snapshot_mismatch_refused(base):
    d, wd, status, env = base
    ch = chain_for(d, status, 1)
    ch["snap_ops"] = "00" * 32
    with pytest.raises(SystemExit):
        V.Verifier(d, env).attest(ch)


def test_few_groups_bit2(base):
    d, wd, status, env = base
    pl, _ = attest(d, env, status, p=0)
    assert pl["flags"] & 4 and not pl["flags"] & 8


def test_missing_rate_bit3(base, tmp_path):
    d, wd, status, env = copy(base, tmp_path)
    for g in range(3 * GPP, 4 * GPP):
        rewrite(f"{d}/ops_{g}.csv", lambda r: r if int(r[0]) % 5 else None)  # drop 20% of the hours
        resign(d, wd, g)
    pl, _ = attest(d, env, status)
    assert pl["flags"] & 8


def test_training_precondition(base, tmp_path):
    d, wd, status, env = copy(base, tmp_path)
    rewrite(f"{d}/training.csv", lambda r: r if int(r[0]) < 2000 else None)
    with pytest.raises(SystemExit):
        V.Verifier(d, env)


def test_real_speed_change_bit0(base, tmp_path):
    d, wd, status, env = copy(base, tmp_path)
    for g in range(4 * GPP):  # OFF blocks 3% slower: reported and independent speed both change
        off = status[g] & 7
        rewrite(f"{d}/ops_{g}.csv", lambda r: r[:4] + [f"{float(r[4]) * 0.97:.3f}"] + r[5:] if int(r[2]) == off else r)
    ind = list(csv.reader(open(f"{d}/independent.csv")))
    offh = set()
    for g in range(4 * GPP):
        for r in list(csv.reader(open(f"{d}/ops_{g}.csv")))[1:]:
            if int(r[2]) == status[g] & 7:
                offh.add(int(r[0]))
    with open(f"{d}/independent.csv", "w") as f:
        f.write("\n".join(",".join(r if i == 0 or int(r[0]) not in offh else [r[0], f"{float(r[1]) * 0.97:.3f}"] + r[2:])
                          for i, r in enumerate(ind)) + "\n")
    pl, _ = attest(d, env, status)
    assert pl["flags"] & 1


def test_reported_draft_falsified_bit0(base, tmp_path):
    d, wd, status, env = copy(base, tmp_path)
    for g in range(4 * GPP):
        off = status[g] & 7
        rewrite(f"{d}/ops_{g}.csv", lambda r: r[:5] + [f"{float(r[5]) * 1.01:.3f}"] + r[6:] if int(r[2]) == off else r)
    pl, _ = attest(d, env, status)
    assert pl["flags"] & 1


def test_meter_under_read_bit1(base, tmp_path):
    d, wd, status, env = copy(base, tmp_path)
    for g in range(4 * GPP):
        off = status[g] & 7
        rewrite(f"{d}/ops_{g}.csv", lambda r: r[:10] + [f"{float(r[10]) * 0.99:.3f}"] if int(r[2]) == off else r)
        resign(d, wd, g)  # tampered sealed meter still signs
    pl, _ = attest(d, env, status)
    assert pl["flags"] & 2


def test_meter_shift_bit1(base, tmp_path):
    d, wd, status, env = copy(base, tmp_path)
    for g in range(4 * GPP):  # move 2% of OFF readings into ON hours, keeping the group total
        off = status[g] & 7
        rows = list(csv.reader(open(f"{d}/ops_{g}.csv")))[1:]
        fo = sum(float(r[10]) for r in rows if int(r[2]) == off)
        fn = sum(float(r[10]) for r in rows if int(r[2]) != off)
        k = 0.02 * fo / fn
        rewrite(f"{d}/ops_{g}.csv", lambda r: r[:10] + [f"{float(r[10]) * (0.98 if int(r[2]) == off else 1 + k):.3f}"])
        resign(d, wd, g)
    pl, _ = attest(d, env, status)
    assert pl["flags"] & 2


def test_meter_signature_bit1(base, tmp_path):
    d, wd, status, env = copy(base, tmp_path)
    g = 3 * GPP + 5
    rewrite(f"{d}/ops_{g}.csv", lambda r: r[:10] + [f"{float(r[10]) + 1:.3f}"])  # edited without the meter key
    pl, _ = attest(d, env, status)
    assert pl["flags"] & 2


def _fold(d, status, g, wind, wind_ref):
    on_blk = ((status[g] & 7) + 1) % 5
    hours = [int(r[0]) for r in list(csv.reader(open(f"{d}/ctrl_{g}.csv")))[1:] if int(r[2]) == on_blk]
    rewrite(f"{d}/ctrl_{g}.csv", lambda r: r[:4] + ["OFF", "0.0", "1"] if int(r[0]) in hours else r)
    rewrite(f"{d}/ops_{g}.csv", lambda r: r[:6] + [f"{wind:.3f}"] + r[7:] if int(r[0]) in hours else r)
    ind = list(csv.reader(open(f"{d}/independent.csv")))
    with open(f"{d}/independent.csv", "w") as f:
        f.write("\n".join(",".join(r if i == 0 or int(r[0]) not in hours else [r[0], r[1], f"{wind_ref:.3f}"] + r[3:])
                          for i, r in enumerate(ind)) + "\n")


def test_override_storm_in_kg_on(base, tmp_path):
    d, wd, status, env = copy(base, tmp_path)
    pl0, _ = attest(d, env, status)
    _fold(d, status, 3 * GPP + 7, 19.0, 15.0)
    pl, rep = attest(d, env, status)
    assert pl["override_groups"] == 0 and pl["kg_miss_payer"] == 0
    assert json.loads(rep)["groups"]["excluded"][str(3 * GPP + 7)] == "override_storm"
    assert abs(pl["kg_on"] - pl0["kg_on"]) <= 1


def test_override_below_threshold_counts_as_payer_missing(base, tmp_path):
    d, wd, status, env = copy(base, tmp_path)
    _fold(d, status, 3 * GPP + 9, 19.0, 10.0)  # ship anemometer says storm, reanalysis does not
    pl, _ = attest(d, env, status)
    assert pl["override_groups"] == 1 and pl["kg_miss_payer"] > 0


def test_off_block_sail_on_is_violation(base, tmp_path):
    d, wd, status, env = copy(base, tmp_path)
    g = 3 * GPP + 11
    off = status[g] & 7
    rewrite(f"{d}/ctrl_{g}.csv", lambda r: r[:4] + ["ON", "120.0", "1"] if int(r[2]) == off else r)
    pl, rep = attest(d, env, status)
    assert pl["compliance_bps"] < 10_000 and pl["override_groups"] == 0
    assert json.loads(rep)["groups"]["excluded"][str(g)] == "off_violation"
