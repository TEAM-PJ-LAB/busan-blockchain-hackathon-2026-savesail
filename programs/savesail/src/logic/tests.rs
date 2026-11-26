//! T2 (PRD appendix A vectors) and T3 (invariants). Expected values come from
//! `python3 -I offchain/reference/savesail_ref.py`.

use super::{schedule::*, settle::*, waterfall::*};

fn hex(b: &[u8; 32]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn chain(seed: [u8; 32], n: usize) -> Vec<[u8; 32]> {
    let mut c = vec![seed];
    for _ in 0..n {
        let x = h(&[c.last().unwrap()]);
        c.push(x);
    }
    c
}

fn seeds() -> [Vec<[u8; 32]>; 3] {
    [b"payer".as_slice(), b"investor_rep", b"verifier"].map(|s| chain(h(&[s]), 120))
}

// ---------- A.1 ----------
#[test]
fn a1_anchors_and_reveals() {
    let ch = seeds();
    let want_anchor = [
        "5c2fcaab56cb2308be7eb52b2f4922aac5d3cc9379fd64f77ee64e2604caa2b3",
        "4fb1b8f572f9f6168a4a7672615401a3804bccb3fccdaa696ec2cfa9a0ca26b4",
        "ec70194db93de911e43a431db28da7a61459d9da9e87e11373902b594edf6dd3",
    ];
    let want_g0 = [
        "fac7dfa5696b74df2c34f0a88ab2c0733052d66c2c4989dbc48dcda46a537979",
        "4b5562e68e9b0df02c95387018f2ee5e6ff3384226f105352dea7d35b7ad8688",
        "c618f754468cf3d8aa3c9db75707418f2c28ce1a27be5e26a0f5aa4f332b793d",
    ];
    for i in 0..3 {
        assert_eq!(hex(&ch[i][120]), want_anchor[i]);
        assert_eq!(hex(&ch[i][119]), want_g0[i]);
    }
}

#[test]
fn a1_digest_and_off_index() {
    let ch = seeds();
    let camp = [0u8; 32];
    let d = h(&[OFF_DOMAIN, &camp, &0u32.to_le_bytes(), &ch[0][119], &ch[1][119], &ch[2][119]]);
    assert_eq!(hex(&d), "4fa78fab4cb11a5eee846d421ad2e99915ae3b0f9e0f602a44456a360bc6f638");
    let mut offs = Vec::new();
    for g in 0..120usize {
        let v = [&ch[0][119 - g], &ch[1][119 - g], &ch[2][119 - g]];
        for (i, c) in ch.iter().enumerate() {
            assert!(verify_reveal(&c[120 - g], v[i], 1));
        }
        offs.push(off_index(&camp, g as u32, v));
    }
    assert_eq!(&offs[..10], &[3, 2, 2, 2, 0, 4, 3, 4, 3, 4]);
    let counts: Vec<usize> = (0..5).map(|i| offs.iter().filter(|&&o| o == i).count()).collect();
    assert_eq!(counts, vec![17, 25, 23, 27, 28]);
}

#[test]
fn a1_skip_rules() {
    let ch = seeds();
    assert!(verify_reveal(&ch[0][119], &ch[0][117], 2));
    assert!(!verify_reveal(&ch[0][119], &ch[0][117], 1));
    assert!(!verify_reveal(&ch[0][120], &ch[0][119], 0));
    assert!(!verify_reveal(&ch[0][120], &ch[0][111], 9)); // k > MAX_SKIP
    assert!(verify_reveal(&ch[0][120], &ch[0][112], 8));
}

#[test]
fn a1_uniformity_10k() {
    let camp = [0u8; 32];
    let (b, c) = (h(&[b"b"]), h(&[b"c"]));
    let mut cnt = [0u32; 5];
    for g in 0u32..10_000 {
        let a = h(&[b"a", &g.to_le_bytes()]);
        cnt[off_index(&camp, g, [&a, &b, &c]) as usize] += 1;
    }
    assert_eq!(cnt, [2009, 2024, 2018, 1946, 2003]);
    let chi2: f64 = cnt.iter().map(|&x| (x as f64 - 2000.0).powi(2) / 2000.0).sum();
    assert!(chi2 < 13.28);
}

#[test]
fn status_byte_roundtrip() {
    for off in [0u8, 4, NO_OFF] {
        for m in 0u8..8 {
            let s = pack_status(off, m);
            assert!(is_resolved(s));
            assert_eq!((off_of(s), missing_of(s)), (off, m));
        }
    }
    assert!(!is_resolved(0));
}

// ---------- A.2 ----------
#[test]
fn a2_merkle_and_acc() {
    let lines: [&[u8]; 3] = [b"0,1,14.2,11.8,812.5", b"1,1,14.1,11.8,809.9", b"2,1,14.3,11.8,815.0"];
    let root = merkle_root(&lines);
    assert_eq!(hex(&root), "cf08df66c4258776453b55c98e2ee0dd74551e307ffd19fc7af549548d2352b4");
    assert_eq!(
        hex(&data_acc_next(&[0u8; 32], 0, &root)),
        "5e5d3bf5f86bd4a2aaeb166cd3f659aaa95ed884f7207b1ae0e064072237bea4"
    );
}

// ---------- A.3 / A.4 / A.5 ----------
const PRICE: u64 = 750_000_000;
const SENIOR: u64 = 736_000_000_000;
const JUNIOR: u64 = 3_264_000_000_000;
const SCHED: u64 = 23_494_160_637;
const OPEX: u64 = 20_000_000_000;
const PHI: u16 = 9000;
const REF: i32 = 600;
const KG: u64 = 3_980_000;

fn pin(kg_on: u64, kg_miss_payer: u64, lb: i32, ub: i32, outcome: u8) -> PeriodInput {
    PeriodInput { kg_on, kg_miss_payer, price_micro_per_t: PRICE, lb_bps: lb, ub_bps: ub, outcome }
}

fn cnt(groups: u32, payer_miss: u32, inv_miss: u32, ver_miss: u32, override_groups: u32) -> Counts {
    Counts { groups, payer_miss, inv_miss, ver_miss, override_groups }
}

#[test]
fn classify_precedence() {
    let ok = cnt(30, 0, 0, 0, 0);
    assert_eq!(classify(10000, 9500, 0, &ok, 50), OUT_NORMAL);
    assert_eq!(classify(9499, 9500, 0, &ok, 50), OUT_PAYER_VIOLATION);
    for f in [F_COVARIATE, F_METER, F_DATA_QUALITY] {
        assert_eq!(classify(10000, 9500, f, &ok, 50), OUT_PAYER_VIOLATION);
    }
    assert_eq!(classify(10000, 9500, F_FEW_GROUPS, &ok, 50), OUT_NO_BOUND);
    assert_eq!(classify(10000, 9500, 0, &cnt(30, 1, 0, 0, 0), 50), OUT_PAYER_VIOLATION);
    assert_eq!(classify(10000, 9500, 0, &cnt(30, 0, 0, 0, 1), 50), OUT_PAYER_VIOLATION);
    assert_eq!(classify(10000, 9500, 0, &cnt(300, 0, 0, 0, 1), 50), OUT_NORMAL); // 1/300 <= 0.5%
    assert_eq!(classify(10000, 9500, 0, &cnt(30, 0, 1, 0, 0), 50), OUT_INVESTOR_OVER_CAP);
    assert_eq!(classify(10000, 9500, 0, &cnt(30, 0, 0, 1, 0), 50), OUT_NO_BOUND);
    // precedence
    assert_eq!(classify(10000, 9500, F_FEW_GROUPS, &cnt(30, 1, 1, 1, 0), 50), OUT_PAYER_VIOLATION);
    assert_eq!(classify(10000, 9500, F_FEW_GROUPS, &cnt(30, 0, 1, 1, 0), 50), OUT_INVESTOR_OVER_CAP);
    // A.3 uses a vector-only cap of 500 bps
    assert_eq!(classify(10000, 9500, 0, &cnt(30, 1, 0, 0, 0), 500), OUT_NORMAL);
    assert_eq!(classify(10000, 9500, 0, &cnt(30, 2, 0, 0, 0), 500), OUT_PAYER_VIOLATION);
}

#[test]
fn a3_four_periods() {
    assert_eq!(classify(9000, 9500, 0, &cnt(30, 0, 0, 0, 0), 500), OUT_PAYER_VIOLATION);
    let rows = [
        (
            pin(3_980_000, 0, 520, 860, OUT_NORMAL),
            147_360_759_493u64,
            147_360_759_493u64,
            Dist { opex: OPEX, senior: SCHED, reserve_draw: 0, reserve_in: 46_988_321_274, junior: 56_878_277_582 },
        ),
        (
            pin(3_850_000, 130_000, 600, 800, OUT_NORMAL),
            197_626_058_267,
            344_986_817_760,
            Dist { opex: OPEX, senior: SCHED, reserve_draw: 0, reserve_in: 0, junior: 154_131_897_630 },
        ),
        (
            pin(3_900_000, 0, 580, 720, OUT_PAYER_VIOLATION),
            204_245_689_654,
            549_232_507_414,
            Dist { opex: OPEX, senior: SCHED, reserve_draw: 0, reserve_in: 0, junior: 160_751_529_017 },
        ),
        (
            pin(3_970_000, 0, 640, 760, OUT_NORMAL),
            207_259_001_638,
            756_491_509_052,
            Dist { opex: OPEX, senior: SCHED, reserve_draw: 0, reserve_in: 0, junior: 163_764_841_001 },
        ),
    ];
    let (mut a, mut b) = (Accrual::default(), Book::default());
    let mut vault = [0u64; 2];
    for (p, due, e, dist) in rows {
        let d = finalize(&mut a, &p, PHI, REF).unwrap();
        assert_eq!((d, a.entitlement), (due, e));
        a.paid_cum += d;
        let out = distribute(&mut b, d, OPEX, SCHED, 2 * SCHED, [SENIOR, JUNIOR]).unwrap();
        assert_eq!(out, dist);
        vault[0] += out.senior;
        vault[1] += out.junior;
    }
    let cs = claimable(SENIOR, b.acc[0], 0).unwrap();
    let cj = claimable(JUNIOR, b.acc[1], 0).unwrap();
    assert_eq!((cs, cj), (93_976_642_545, 535_526_545_224));
    assert_eq!((vault[0] - cs, vault[1] - cj), (3, 6));
}

#[test]
fn a4_close_refund_limited_by_reserve() {
    let (mut a, mut b) = (Accrual::default(), Book::default());
    let d0 = finalize(&mut a, &pin(KG, 0, 800, 1000, OUT_NORMAL), PHI, REF).unwrap();
    assert_eq!(d0, 233_608_695_651);
    a.paid_cum += d0;
    let o0 = distribute(&mut b, d0, OPEX, SCHED, 2 * SCHED, [SENIOR, JUNIOR]).unwrap();
    assert_eq!((o0.reserve_in, o0.junior), (46_988_321_274, 143_126_213_740));
    let d1 = finalize(&mut a, &pin(KG, 0, 300, 700, OUT_NORMAL), PHI, REF).unwrap();
    assert_eq!((d1, a.entitlement), (0, 166_175_257_731));
    let o1 = distribute(&mut b, 0, OPEX, SCHED, 2 * SCHED, [SENIOR, JUNIOR]).unwrap();
    assert_eq!((o1.opex, o1.senior, o1.reserve_draw, b.opex_arrears), (0, SCHED, SCHED, OPEX));
    assert_eq!(a.paid_cum - a.entitlement, 67_433_437_920);
    let c = close(&mut b, a.paid_cum, a.entitlement, [SENIOR, JUNIOR]).unwrap();
    assert_eq!(c, Close { refund: SCHED, opex_arrears_paid: 0, senior_arrears_paid: 0, reserve_to_junior: 0 });
    assert_eq!(b.reserve, 0);
}

/// Two periods; returns (due0, due1, accrual).
fn two(p0: PeriodInput, p1: Option<PeriodInput>) -> (u64, u64, Accrual) {
    let mut a = Accrual::default();
    let d0 = finalize(&mut a, &p0, PHI, REF).unwrap();
    a.paid_cum += d0;
    let d1 = p1.map(|p| finalize(&mut a, &p, PHI, REF).unwrap()).unwrap_or(0);
    (d0, d1, a)
}

#[test]
fn a5_envelope_then_normal() {
    let o = classify(10000, 9500, F_FEW_GROUPS, &cnt(30, 0, 0, 0, 0), 50);
    let (d0, d1, a) = two(pin(KG, 0, 410, 790, o), Some(pin(KG, 0, 600, 800, OUT_NORMAL)));
    assert_eq!((d0, d1, a.last_lb_bps), (0, 342_957_446_808, Some(600)));
}

#[test]
fn a5_investor_over_cap() {
    let o = classify(10000, 9500, 0, &cnt(30, 0, 1, 0, 0), 50);
    let (d0, d1, a) = two(pin(KG, 0, 600, 800, OUT_NORMAL), Some(pin(KG, 0, 650, 850, o)));
    assert_eq!((d0, d1, a.last_lb_bps), (171_478_723_403, 0, Some(600)));
}

#[test]
fn a5_data_quality() {
    let o = classify(10000, 9500, F_DATA_QUALITY, &cnt(30, 0, 0, 0, 0), 50);
    let (d0, _, a) = two(pin(KG, 0, 600, 800, o), None);
    assert_eq!((d0, a.last_lb_bps), (233_608_695_651, None));
}

#[test]
fn a5_override_in_cap() {
    let o = classify(10000, 9500, 0, &cnt(300, 0, 0, 0, 1), 50);
    let (d0, _, a) = two(pin(KG, 13_000, 600, 800, o), None);
    assert_eq!((d0, a.last_lb_bps), (172_241_766_881, Some(600)));
}

#[test]
fn a5_override_over_cap() {
    let o = classify(10000, 9500, 0, &cnt(30, 0, 0, 0, 1), 50);
    let (d0, _, a) = two(pin(KG, 13_000, 600, 800, o), None);
    assert_eq!((d0, a.last_lb_bps), (234_371_739_129, None));
}

#[test]
fn a5_verifier_over_cap() {
    let o = classify(10000, 9500, 0, &cnt(30, 0, 0, 1, 0), 50);
    let (d0, d1, a) = two(pin(KG, 0, 600, 800, OUT_NORMAL), Some(pin(KG, 0, 650, 850, o)));
    assert_eq!((d0, d1, a.last_lb_bps), (171_478_723_403, 171_478_723_405, Some(600)));
}

#[test]
fn skip_rule_counts_only_own_misses() {
    let vonly = pack_status(NO_OFF, VERIFIER_ONLY);
    let payer = pack_status(NO_OFF, 0b001);
    let mut st = [0u8; 40];
    st[..20].fill(vonly); // verifier withheld groups 0..19
    assert_eq!(blocked_by_verifier(&st, -1, 20), 20);
    assert!(skip_ok(0, 21, 20)); // payer revealing group 20 after anchor: own skips = 1
    assert!(!skip_ok(2, 21, 0)); // verifier itself: 21 > MAX_SKIP
    st[20..28].fill(payer); // payer withheld 8 more
    assert_eq!(blocked_by_verifier(&st, -1, 28), 20);
    assert!(skip_ok(0, 28, 20)); // 7 own misses + this group = 8
    assert!(!skip_ok(0, 29, 20)); // 8 own misses: locked out (PRD §6.4)
    assert!(!skip_ok(0, 0, 0));
}

#[test]
fn rate_bounds_and_overflow() {
    assert!(rates_valid(-9999, 0, 4999));
    assert!(!rates_valid(-10000, 0, 100));
    assert!(!rates_valid(500, 400, 600));
    assert!(!rates_valid(100, 200, 5000));
    assert_eq!(g_num(100, -300), Ok(0));
    assert_eq!(g_num(1, 10_000), Err(MathError));
    let mut a = Accrual::default();
    let big = PeriodInput {
        kg_on: u64::MAX,
        kg_miss_payer: 0,
        price_micro_per_t: u64::MAX,
        lb_bps: 4999,
        ub_bps: 4999,
        outcome: OUT_NORMAL,
    };
    assert_eq!(finalize(&mut a, &big, 10000, 0), Err(MathError));
    assert_eq!(a, Accrual::default()); // state untouched on error
    assert_eq!(finalize(&mut a, &pin(1, 0, 0, 0, 9), PHI, REF), Err(MathError));
    // unknown outcome
}

// ---------- T3 invariants (100k random cases, PRD K4) ----------
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

#[test]
fn t3_invariants_100k() {
    let mut r = Rng(0x5A5E_5A11_2026_1127);
    for _ in 0..100_000 {
        let units = [1 + r.below(5_000_000_000_000), 1 + r.below(5_000_000_000_000)];
        let (opex, sched, target) = (r.below(50_000_000_000), r.below(80_000_000_000), r.below(200_000_000_000));
        let phi = (r.below(10_001)) as u16;
        let (mut a, mut b) = (Accrual::default(), Book::default());
        let (mut received, mut out_opex, mut vault, mut refund) = (0u64, 0u64, [0u64; 2], 0u64);
        let mut prev_paid = 0u64;
        let periods = 1 + r.below(8);
        for _ in 0..periods {
            let lb = r.below(4000) as i32 - 1000;
            let ub = lb + r.below((4999 - lb.max(0)) as u64 + 1) as i32;
            let ub = ub.min(4999).max(lb);
            let p = PeriodInput {
                kg_on: r.below(10_000_000),
                kg_miss_payer: if r.below(4) == 0 { r.below(500_000) } else { 0 },
                price_micro_per_t: 300_000_000 + r.below(900_000_000),
                lb_bps: lb,
                ub_bps: ub,
                outcome: r.below(4) as u8,
            };
            let due = finalize(&mut a, &p, phi, 600).unwrap();
            assert_eq!(due, a.entitlement.saturating_sub(a.paid_cum)); // inv 5
            a.paid_cum += due;
            assert!(a.paid_cum >= prev_paid); // inv 4
            prev_paid = a.paid_cum;
            received += due;
            let d = distribute(&mut b, due, opex, sched, target, units).unwrap();
            if b.senior_arrears > 0 {
                assert_eq!(d.junior, 0); // inv 3
            }
            out_opex += d.opex;
            vault[0] += d.senior;
            vault[1] += d.junior;
            // inv 1: conservation (reserve is inside the system)
            assert_eq!(out_opex + vault[0] + vault[1] + b.reserve, received);
        }
        let c = close(&mut b, a.paid_cum, a.entitlement, units).unwrap();
        refund += c.refund;
        out_opex += c.opex_arrears_paid;
        vault[0] += c.senior_arrears_paid;
        vault[1] += c.reserve_to_junior;
        assert_eq!(out_opex + vault[0] + vault[1] + refund, received); // inv 1 after close
                                                                       // inv 2 / 7: claims never exceed vault; rounding dust < (units/ACC + 1) per distribution
        let cs = claimable(units[0], b.acc[0], 0).unwrap();
        let cj = claimable(units[1], b.acc[1], 0).unwrap();
        assert!(cs <= vault[0] && cj <= vault[1]);
        let bound = |u: u64| (periods + 1) * (u / ACC as u64 + 1) + 1;
        assert!(vault[0] - cs <= bound(units[0]) && vault[1] - cj <= bound(units[1]));
    }
}

#[test]
fn t3_extreme_inputs_overflow() {
    // inv 6: extreme inputs are rejected with MathError instead of wrapping
    let mut a = Accrual { w_norm: u128::MAX / 2, ..Default::default() };
    let p = PeriodInput {
        kg_on: u64::MAX,
        kg_miss_payer: 0,
        price_micro_per_t: u64::MAX,
        lb_bps: 1,
        ub_bps: 1,
        outcome: OUT_NORMAL,
    };
    assert_eq!(finalize(&mut a, &p, PHI, REF), Err(MathError));
    let mut b = Book { acc: [u128::MAX, 0], ..Default::default() };
    assert!(distribute(&mut b, 1_000_000, 0, 1_000_000, 0, [1, 1]).is_err());
}
