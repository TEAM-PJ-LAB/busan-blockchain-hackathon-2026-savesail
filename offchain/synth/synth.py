"""Synthetic voyage data generator (PRD §7.1).

The world (weather, speed, draft, fouling, unexplained variation, sensor noise, independent sources)
is fixed by the seed before any schedule exists; it is research/measure.py `prepare` exactly. Only
after the chain fixes a group's OFF block does `group()` compute fuel for that group, so data is
always produced after the schedule (PRD §5).

CLI (used by the executor):
  python3 synth.py serve CONFIG.json OUT_DIR
    stdin : one JSON per line  {"group": g, "off_index": k, "viol": false}   (k = 7 -> all blocks ON)
    stdout: one JSON per line  {"group": g, "root_ops": hex, "root_ctrl": hex, "rows": n}
  Files in OUT_DIR: training.csv, independent.csv, engine_curve.csv, meter_key.json,
  ops_<g>.csv, ctrl_<g>.csv, meter_<g>.json, voyages.csv (completed voyages, rewritten each group).
"""
import hashlib
import json
import os
import sys

import numpy as np
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "..", "..", "research"))
import measure as m  # noqa: E402

DRAFT_REF_M = 18.0      # laden draft of the demo VLOC
SFOC_REF = 0.170        # kg/kWh at the SFOC minimum
SAIL_POWER_KW = 120.0   # sail drive power when running
NO_OFF = 7
OPS_HEADER = "hour,group,block,w,speed_kn,draft_m,wind_ms,wind_rel_deg,hs_m,shaft_kw,fuel_kg"
CTRL_HEADER = "hour,group,block,cmd,state,power_kw,override"
IND_HEADER = "hour,speed_ind_kn,wind_ms_ref,wind_rel_deg_ref,hs_m_ref"


def sha(b: bytes) -> bytes:
    return hashlib.sha256(b).digest()


def merkle_root(lines):
    """PRD §7.3: leaf = sha256(0x00 || row), node = sha256(0x01 || left || right), odd level duplicates last."""
    lvl = [sha(b"\x00" + ln.encode()) for ln in lines] or [sha(b"\x00")]
    while len(lvl) > 1:
        if len(lvl) % 2:
            lvl.append(lvl[-1])
        lvl = [sha(b"\x01" + lvl[i] + lvl[i + 1]) for i in range(0, len(lvl), 2)]
    return lvl[0]


def ops_line(h, g, b, w, v, d, u, th, hs, shaft, fuel):
    return f"{h},{g},{b},{w:.2f},{v:.3f},{d:.3f},{u:.3f},{th:.2f},{hs:.3f},{shaft:.1f},{fuel:.3f}"


class World:
    def __init__(self, cfg: dict, seed: int):
        self.cfg = dict(m.BASE, **cfg)
        self.seed = seed
        self.st = m.prepare(self.cfg, seed)
        st, c = self.st, self.cfg
        self.L, self.G = c["L"], c["G"]
        self.gh = self.L * self.G
        self.key = Ed25519PrivateKey.from_private_bytes(sha(f"savesail-test-meter-{seed}".encode()))
        self.done_hours = 0
        n = st["n"]
        self.sail_on = np.zeros(n, bool)
        self.act, self.meas = np.zeros(n), np.zeros(n)

    # ------------------------------------------------------------ fixed inputs
    def training_lines(self):
        st, t0 = self.st, self.st["t0"]
        nos, mf = st["nos"][:t0], st["m_fuel"][:t0]
        load = st["load0"] * nos / st["fref"]
        shaft = nos / (m.sfoc(load) * (1 + self.cfg["sfoc_err"] * (load - 0.75)) * st["eta"][:t0]) * st["m_q"][:t0]
        out = [OPS_HEADER]
        for h in range(t0):
            if not st["obs"][h]:
                continue
            out.append(ops_line(h, -1, -1, 1.0, st["Vo"][h], DRAFT_REF_M * st["Dobs"][h], st["Uo"][h], st["tho"][h],
                                st["Hso"][h], shaft[h] / SFOC_REF, nos[h] * mf[h]))
        return out

    def independent_lines(self):
        st = self.st
        T = st["T"]
        return [IND_HEADER] + [f"{h},{st['Vind'][h]:.3f},{st['Uref'][h]:.3f},{st['thref'][h]:.2f},{st['Href'][h]:.3f}"
                               for h in range(T)]

    def engine_curve_lines(self):
        loads = np.round(np.arange(0.30, 1.0001, 0.05), 2)
        return ["load,sfoc_rel"] + [f"{x:.2f},{m.sfoc(x):.6f}" for x in loads]

    # ------------------------------------------------------------ one group after its schedule is fixed
    def group(self, g: int, off_index: int, viol: bool = False):
        st, c = self.st, self.cfg
        t0, gh, L = st["t0"], self.gh, self.L
        idx = np.arange(g * gh, (g + 1) * gh)  # campaign-relative hours
        blk = (idx // L) % self.G
        off = (blk == off_index) if off_index != NO_OFF else np.zeros(gh, bool)
        sail = ~off | (off & viol)
        self.sail_on[idx] = sail
        hrs = t0 + idx
        nos = st["nos"][hrs]
        act = nos * (1 - st["s_h"][idx] * sail)
        load = st["load0"] * act / st["fref"]
        shaft = act / (m.sfoc(load) * (1 + c["sfoc_err"] * (load - 0.75)) * st["eta"][hrs]) * st["m_q"][hrs]
        meas = act * st["m_fuel"][hrs]
        w = st["tw"][idx]
        ops, ctrl = [], []
        for j, h in enumerate(hrs):
            if not st["obs"][h]:
                continue
            ops.append(ops_line(int(h), g, int(blk[j]), w[j], st["Vo"][h], DRAFT_REF_M * st["Dobs"][h], st["Uo"][h],
                                st["tho"][h], st["Hso"][h], shaft[j] / SFOC_REF, meas[j]))
        for j, h in enumerate(hrs):
            cmd = "OFF" if off[j] else "ON"
            state = "ON" if sail[j] else "OFF"
            ctrl.append(f"{int(h)},{g},{int(blk[j])},{cmd},{state},{SAIL_POWER_KW if sail[j] else 0.0:.1f},0")
        fuel_digest = sha("\n".join(ln.split(",")[-1] for ln in ops).encode())
        meter = dict(group=g, meter_digest=fuel_digest.hex(), meter_sig=self.key.sign(fuel_digest).hex())
        self.done_hours = max(self.done_hours, (g + 1) * gh)
        self.act[idx], self.meas[idx] = act, meas
        return ops, ctrl, meter

    def voyage_lines(self):
        """Completed voyages (campaign part): BDN - dROB = true consumption x bunker measurement error,
        meter_total_kg = sealed-meter totalizer over the same hours (also counts hours whose rows are missing)."""
        st = self.st
        t0, vid = st["t0"], st["vid"][st["camp"]]
        out = ["voyage,bdn_kg,rob_start_kg,rob_end_kg,meter_total_kg,hours"]
        for v in np.unique(vid[:self.done_hours]):
            hv = np.flatnonzero(vid == v)
            if hv[-1] >= self.done_hours:
                continue  # voyage not finished yet
            bdn = float(self.act[hv].sum()) * float(st["bdn"][v])
            out.append(f"{int(v)},{bdn:.3f},2000000.0,2000000.0,{float(self.meas[hv].sum()):.3f},{hv.size}")
        return out


def write(path, lines):
    with open(path, "w") as f:
        f.write("\n".join(lines) + "\n")


def serve(cfg_path, out_dir):
    cfg = json.load(open(cfg_path))
    seed = cfg.pop("seed")
    os.makedirs(out_dir, exist_ok=True)
    wd = World(cfg, seed)
    write(os.path.join(out_dir, "training.csv"), wd.training_lines())
    write(os.path.join(out_dir, "independent.csv"), wd.independent_lines())
    write(os.path.join(out_dir, "engine_curve.csv"), wd.engine_curve_lines())
    pub = wd.key.public_key().public_bytes_raw().hex()
    json.dump(dict(public_key=pub, alg="ed25519"), open(os.path.join(out_dir, "meter_key.json"), "w"))
    for line in sys.stdin:
        if not line.strip():
            continue
        q = json.loads(line)
        g = int(q["group"])
        ops, ctrl, meter = wd.group(g, int(q["off_index"]), bool(q.get("viol", False)))
        write(os.path.join(out_dir, f"ops_{g}.csv"), [OPS_HEADER] + ops)
        write(os.path.join(out_dir, f"ctrl_{g}.csv"), [CTRL_HEADER] + ctrl)
        json.dump(meter, open(os.path.join(out_dir, f"meter_{g}.json"), "w"))
        write(os.path.join(out_dir, "voyages.csv"), wd.voyage_lines())
        print(json.dumps(dict(group=g, root_ops=merkle_root(ops).hex(), root_ctrl=merkle_root(ctrl).hex(),
                              rows=len(ops))), flush=True)


if __name__ == "__main__":
    if len(sys.argv) == 4 and sys.argv[1] == "serve":
        serve(sys.argv[2], sys.argv[3])
    else:
        sys.exit(__doc__)
