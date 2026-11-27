//! LiteSVM tests on the compiled program (PRD §10: T1 commands, T2 on-chain vectors, T3 on-chain
//! invariants, T4 security, T6 compute units). Build first: `cargo build-sbf`.
//! Every checked error and every successful command is appended to `target/tmp/k2_hits.txt`;
//! `scripts/check_k2.py` verifies K2 (all error codes hit) and the T1/T6 counts from that file.

include!("common/harness.rs");

// =====================================================================================
// T2 on-chain: appendix A vectors reproduced by the deployed program
// =====================================================================================
#[test]
fn t2_a1_anchors_off_index_and_skip() {
    let mut e = Env::new(Cfg::new(120, 30, 4));
    e.goto_active();
    let c = e.camp_state();
    let want = [
        "5c2fcaab56cb2308be7eb52b2f4922aac5d3cc9379fd64f77ee64e2604caa2b3",
        "4fb1b8f572f9f6168a4a7672615401a3804bccb3fccdaa696ec2cfa9a0ca26b4",
        "ec70194db93de911e43a431db28da7a61459d9da9e87e11373902b594edf6dd3",
    ];
    for (h, w) in c.last_hash.iter().zip(want) {
        assert_eq!(hex(h), w);
    }
    assert_eq!(hex(&e.reveal_value(0, 0)), "fac7dfa5696b74df2c34f0a88ab2c0733052d66c2c4989dbc48dcda46a537979");
    for g in 0..10u16 {
        e.resolve_all(g);
        let v = [e.reveal_value(0, g), e.reveal_value(1, g), e.reveal_value(2, g)];
        let off = sch::off_index(&e.campaign.to_bytes(), g as u32, [&v[0], &v[1], &v[2]]);
        assert_eq!(e.camp_state().status[g as usize], sch::pack_status(off, 0));
    }
    // skip: payer withholds group 10, then reveals c_{N-1-11} with k = 2; the k = 1 value fails
    e.play(10, 0b001);
    ok(e.reveal(2, 11));
    let p = e.payer.insecure_clone();
    let wrong = e.reveal_value(0, 10);
    expect_e(e.reveal_as(&p, 0, 11, wrong), E::HashChainMismatch);
    ok(e.reveal(0, 11));
    assert_eq!(e.camp_state().last_group[0], 11);
}

#[test]
fn t2_a2_data_commit() {
    let mut e = Env::new(Cfg::a4());
    e.goto_active();
    e.resolve_all(0);
    let lines: [&[u8]; 3] = [b"0,1,14.2,11.8,812.5", b"1,1,14.1,11.8,809.9", b"2,1,14.3,11.8,815.0"];
    let root = sch::merkle_root(&lines);
    assert_eq!(hex(&root), "cf08df66c4258776453b55c98e2ee0dd74551e307ffd19fc7af549548d2352b4");
    let p = e.payer.insecure_clone();
    ok(e.commit_as(&p, 0, 0, root));
    assert_eq!(hex(&e.camp_state().data_acc[0]), "5e5d3bf5f86bd4a2aaeb166cd3f659aaa95ed884f7207b1ae0e064072237bea4");
}

#[test]
fn t2_a3_four_periods() {
    let mut e = Env::new(Cfg { miss_max: 500, ..Cfg::new(120, 30, 4) });
    e.goto_active();
    type Row = (u64, u64, i32, i32, u16, u64, u64, [u64; 5]);
    let rows: [Row; 4] = [
        (
            3_980_000,
            0,
            520,
            860,
            10_000,
            147_360_759_493,
            147_360_759_493,
            [20_000 * M, SCHED, 0, 46_988_321_274, 56_878_277_582],
        ),
        (
            3_850_000,
            130_000,
            600,
            800,
            10_000,
            197_626_058_267,
            344_986_817_760,
            [20_000 * M, SCHED, 0, 0, 154_131_897_630],
        ),
        (3_900_000, 0, 580, 720, 9_000, 204_245_689_654, 549_232_507_414, [20_000 * M, SCHED, 0, 0, 160_751_529_017]),
        (3_970_000, 0, 640, 760, 10_000, 207_259_001_638, 756_491_509_052, [20_000 * M, SCHED, 0, 0, 163_764_841_001]),
    ];
    for (p, (kg, miss, lb, ub, comp, due, ent, dist)) in rows.into_iter().enumerate() {
        let p = p as u16;
        let withheld: &[(u16, u8)] = if p == 1 { &[(31, 0b001)] } else { &[] };
        e.period_with(p, withheld);
        let pl = AttestationPayload { kg_miss_payer: miss, ..e.payload(p, kg, lb, (lb + ub) / 2, ub, comp) };
        let ps = e.close_period(pl);
        assert_eq!((ps.due, e.deal_state().entitlement, ps.dist), (due, ent, dist), "period {p}");
        let want_outcome = if p == 2 { st::OUT_PAYER_VIOLATION } else { st::OUT_NORMAL };
        assert_eq!(ps.outcome, want_outcome);
    }
    assert_eq!(e.bal(e.vault(V_RESERVE)), 46_988_321_274 + SLASH); // + bond slashed for group 31
    let jv = e.bal(e.vault(V_JUNIOR));
    let before = e.bal(e.t(&e.inv[2]));
    ok(e.claim(2, 1));
    assert_eq!(e.bal(e.t(&e.inv[2])) - before, 535_526_545_224);
    assert_eq!(jv - 535_526_545_224, 6);
    let sv = e.bal(e.vault(V_SENIOR));
    ok(e.claim(0, 0));
    ok(e.claim(1, 0));
    let left = e.bal(e.vault(V_SENIOR));
    assert_eq!(sv, 4 * SCHED);
    assert!(4 * SCHED - left >= 93_976_642_545 - 1 && left <= 3 + 1);
}

#[test]
fn flow_a4_end_to_end() {
    let mut e = Env::new(Cfg::a4());
    e.goto_funded();
    assert_eq!(e.bal(e.t(&e.mfr)), 10_000_000 * M + SENIOR + JUNIOR);
    assert_eq!(e.bal(e.vault(V_RAISE)), 0);
    for r in 0..3 {
        ok(e.register(r));
    }
    assert_eq!(e.deal_state().status, ST_ACTIVE);
    assert_eq!(e.bal(e.vault(V_BOND)), 3 * BOND);

    e.period_ready(0);
    let p0 = e.payload(0, KG, 800, 900, 1000, 10_000);
    ok(e.approve(0, p0));
    assert!(!e.period_state(0).finalized);
    ok(e.approve(2, p0));
    let ps = e.period_state(0);
    assert!(ps.finalized && ps.outcome == st::OUT_NORMAL);
    assert_eq!(ps.due, 233_608_695_651);
    let payer0 = e.bal(e.t(&e.payer));
    ok(e.settle(0));
    assert_eq!(payer0 - e.bal(e.t(&e.payer)), 233_608_695_651);
    let opex0 = e.bal(e.t(&e.opex_owner));
    ok(e.distribute(0));
    assert_eq!(e.bal(e.t(&e.opex_owner)) - opex0, 20_000 * M);
    assert_eq!(e.bal(e.vault(V_SENIOR)), SCHED);
    assert_eq!(e.bal(e.vault(V_RESERVE)), 46_988_321_274);
    assert_eq!(e.bal(e.vault(V_JUNIOR)), 143_126_213_740);
    assert_eq!(e.bal(e.vault(V_COLLECT)), 0);

    e.period_ready(1);
    let p1 = e.payload(1, KG, 300, 500, 700, 10_000);
    ok(e.approve(1, p1));
    ok(e.approve(0, p1));
    assert_eq!(e.period_state(1).due, 0);
    assert_eq!(e.deal_state().entitlement, 166_175_257_731);
    ok(e.settle(1));
    ok(e.distribute(1));
    assert_eq!(e.bal(e.vault(V_SENIOR)), 2 * SCHED);
    assert_eq!(e.bal(e.vault(V_RESERVE)), 46_988_321_274 - SCHED);
    let d = e.deal_state();
    assert_eq!((d.opex_arrears, d.senior_arrears, d.reserve), (20_000 * M, 0, 46_988_321_274 - SCHED));

    let before: Vec<u64> = (0..3).map(|k| e.bal(e.t(&e.inv[k]))).collect();
    ok(e.claim(0, 0));
    ok(e.claim(1, 0));
    ok(e.claim(2, 1));
    let got: Vec<u64> = (0..3).map(|k| e.bal(e.t(&e.inv[k])) - before[k]).collect();
    let acc = e.deal_state().acc;
    assert_eq!(got[0] as u128, (400_000 * M) as u128 * acc[0] / 1_000_000_000_000);
    assert!(got[0] + got[1] <= 2 * SCHED && 2 * SCHED - (got[0] + got[1]) <= 3);
    assert!(got[2] <= 143_126_213_740 && 143_126_213_740 - got[2] <= 4);
    expect_e(e.claim(0, 0), E::NothingToClaim); // double claim (T4-6)

    ok(e.release_bonds());
    assert_eq!(e.bal(e.vault(V_BOND)), 0);
    let payer1 = e.bal(e.t(&e.payer));
    ok(e.close_deal());
    assert_eq!(e.bal(e.t(&e.payer)) - payer1, SCHED); // refund limited by reserve (A.4)
    assert_eq!(e.bal(e.vault(V_RESERVE)), 0);
    assert_eq!(e.deal_state().status, ST_CLOSED);
    for (k, cu) in &e.cu {
        assert!(*cu <= 100_000, "{k} uses {cu} CU");
    }
}

/// One-period or two-period A.5 case on chain. Returns (due per period, last_lb, outcomes).
fn a5_case(
    n_groups: u16,
    gpp: u16,
    periods: &[(AttestationPayloadSpec, &[(u16, u8)])],
) -> (Vec<u64>, Option<i32>, Vec<u8>) {
    let n_p = (n_groups / gpp) as usize;
    let mut e = Env::new(Cfg::new(n_groups, gpp, n_p));
    e.goto_active();
    let (mut dues, mut outs) = (vec![], vec![]);
    for (p, (spec, withheld)) in periods.iter().enumerate() {
        e.period_with(p as u16, withheld);
        let base = e.payload(p as u16, KG, spec.lb, spec.lb + 100, spec.ub, 10_000);
        let pl = AttestationPayload { flags: spec.flags, override_groups: spec.ovr, kg_miss_payer: spec.miss, ..base };
        let ps = e.close_period(pl);
        dues.push(ps.due);
        outs.push(ps.outcome);
    }
    (dues, e.deal_state().last_lb_bps, outs)
}

#[derive(Clone, Copy)]
struct AttestationPayloadSpec {
    lb: i32,
    ub: i32,
    flags: u8,
    ovr: u16,
    miss: u64,
}

const fn sp(lb: i32, ub: i32) -> AttestationPayloadSpec {
    AttestationPayloadSpec { lb, ub, flags: 0, ovr: 0, miss: 0 }
}

#[test]
fn t2_a5_envelope_then_normal() {
    let r = a5_case(60, 30, &[(AttestationPayloadSpec { flags: 0b100, ..sp(410, 790) }, &[]), (sp(600, 800), &[])]);
    assert_eq!(r, (vec![0, 342_957_446_808], Some(600), vec![st::OUT_NO_BOUND, st::OUT_NORMAL]));
}

#[test]
fn t2_a5_investor_over_cap() {
    let r = a5_case(60, 30, &[(sp(600, 800), &[]), (sp(650, 850), &[(40, 0b010)])]);
    assert_eq!(r, (vec![171_478_723_403, 0], Some(600), vec![st::OUT_NORMAL, st::OUT_INVESTOR_OVER_CAP]));
}

#[test]
fn t2_a5_data_quality() {
    let r = a5_case(30, 30, &[(AttestationPayloadSpec { flags: 0b1000, ..sp(600, 800) }, &[])]);
    assert_eq!(r, (vec![233_608_695_651], None, vec![st::OUT_PAYER_VIOLATION]));
}

#[test]
fn t2_a5_override_in_cap() {
    let r = a5_case(300, 300, &[(AttestationPayloadSpec { ovr: 1, miss: 13_000, ..sp(600, 800) }, &[])]);
    assert_eq!(r, (vec![172_241_766_881], Some(600), vec![st::OUT_NORMAL]));
}

#[test]
fn t2_a5_override_over_cap() {
    let r = a5_case(30, 30, &[(AttestationPayloadSpec { ovr: 1, miss: 13_000, ..sp(600, 800) }, &[])]);
    assert_eq!(r, (vec![234_371_739_129], None, vec![st::OUT_PAYER_VIOLATION]));
}

#[test]
fn t2_a5_verifier_over_cap() {
    let r = a5_case(60, 30, &[(sp(600, 800), &[]), (sp(650, 850), &[(45, 0b100)])]);
    assert_eq!(r, (vec![171_478_723_403, 171_478_723_405], Some(600), vec![st::OUT_NORMAL, st::OUT_NO_BOUND]));
}

// =====================================================================================
// T1: command-level checks
// =====================================================================================
#[test]
fn t1_initialize_and_vaults() {
    let mut e = Env::new(Cfg::a4());
    let base = e.params();
    let bad = |f: &dyn Fn(&mut DealParams)| {
        let mut p = base.clone();
        f(&mut p);
        p
    };
    for p in [
        bad(&|p| p.install_cost += 1),
        bad(&|p| p.caps = [0, SENIOR + JUNIOR]),
        bad(&|p| p.n_groups = 5),
        bad(&|p| p.n_groups = 0),
        bad(&|p| {
            p.n_groups = 514;
            p.groups_per_period = 257
        }),
        bad(&|p| p.groups_per_period = 0),
        bad(&|p| {
            p.n_groups = 10;
            p.groups_per_period = 2;
            p.senior_schedule = vec![SCHED; 5]
        }),
        bad(&|p| p.phi_bps = 10_001),
        bad(&|p| p.senior_schedule = vec![SCHED; 41]),
        bad(&|p| p.senior_schedule = vec![SCHED; 1]),
        bad(&|p| p.verifiers[1] = p.verifiers[0]),
        bad(&|p| p.verifiers[2] = p.verifiers[1]),
        bad(&|p| p.ref_rate_bps = 5000),
        bad(&|p| p.ref_rate_bps = -1),
        bad(&|p| p.comp_min_bps = 10_001),
        bad(&|p| p.miss_max_bps = 10_001),
        bad(&|p| p.reveal_timeout_slots = 0),
    ] {
        expect_e(e.init_deal(p), E::InvalidParams);
    }
    ok(e.init_deal(base.clone()));
    let d = e.deal_state();
    assert_eq!((d.envelope_hash, d.n_periods, d.status), (ENV_HASH, 2, ST_FUNDRAISING));
    expect_fail(e.init_deal(base), "re-initialization (T4-3)");
    let x = Keypair::new();
    e.svm.airdrop(&x.pubkey(), 1_000_000_000).unwrap();
    let i = e.ix(
        ix::InitVaults {},
        ac::InitVaults {
            admin: x.pubkey(),
            deal: e.deal,
            payment_mint: e.mint,
            raise_vault: e.vault(V_RAISE),
            collection_vault: e.vault(V_COLLECT),
            senior_vault: e.vault(V_SENIOR),
            junior_vault: e.vault(V_JUNIOR),
            reserve_vault: e.vault(V_RESERVE),
            bond_vault: e.vault(V_BOND),
            token_program: spl_token::ID,
            system_program: system_program::ID,
        },
    );
    expect_e(e.send_with_vaults("x", i, &x), E::Unauthorized);
    ok(e.init_vaults());
    expect_fail(e.init_vaults(), "vaults created twice");
    let d = e.deal_state();
    assert!(d.vaults_ready);
    for k in V_RAISE..=V_BOND {
        assert_eq!(d.vault(k), e.vault(k));
        let t = TokAcc::unpack(&e.svm.get_account(&e.vault(k)).unwrap().data).unwrap();
        assert_eq!((t.mint, t.owner, t.amount), (e.mint, e.deal, 0));
    }
    assert_eq!(d.vault(6), Pubkey::default());
    // a second set of fresh vault accounts is refused by the handler (vaults_ready)
    let fresh: Vec<Keypair> = (0..6).map(|_| Keypair::new()).collect();
    let i = e.ix(
        ix::InitVaults {},
        ac::InitVaults {
            admin: e.admin.pubkey(),
            deal: e.deal,
            payment_mint: e.mint,
            raise_vault: fresh[0].pubkey(),
            collection_vault: fresh[1].pubkey(),
            senior_vault: fresh[2].pubkey(),
            junior_vault: fresh[3].pubkey(),
            reserve_vault: fresh[4].pubkey(),
            bond_vault: fresh[5].pubkey(),
            token_program: spl_token::ID,
            system_program: system_program::ID,
        },
    );
    let a = e.admin.insecure_clone();
    expect_e(e.send("x", i, &[&a, &fresh[0], &fresh[1], &fresh[2], &fresh[3], &fresh[4], &fresh[5]]), E::InvalidState);
    assert_eq!(e.deal_state().vaults, d.vaults);
}

#[test]
fn t1_kyc_invest_close() {
    let mut e = Env::new(Cfg::a4());
    let p = e.params();
    ok(e.init_deal(p));
    expect_ae(e.invest(0, 0, M), AE::AccountNotInitialized); // no vaults yet
    ok(e.init_vaults());
    let a = e.inv[0].pubkey();
    let x = Keypair::new();
    e.svm.airdrop(&x.pubkey(), 1_000_000_000).unwrap();
    expect_e(e.set_kyc_as(&x, a, 0, 1), E::Unauthorized);
    expect_e(e.set_kyc(a, 2, 1), E::InvalidParams);
    expect_e(e.invest(0, 0, M), E::KycRequired);
    ok(e.set_kyc(a, TIER_RETAIL, 100 * M));
    ok(e.set_kyc(e.inv[2].pubkey(), TIER_PRO, 4_000_000 * M));
    expect_e(e.invest(0, 1, M), E::TierNotAllowed);
    expect_e(e.invest(0, 0, 0), E::InvalidParams);
    expect_e(e.invest(0, 2, M), E::InvalidParams);
    expect_e(e.invest(0, 0, 101 * M), E::LimitExceeded);
    ok(e.invest(0, 0, 100 * M));
    expect_e(e.invest(0, 0, 1), E::LimitExceeded);
    expect_e(e.invest(0, 0, u64::MAX), E::MathOverflow);
    let mut clk = e.svm.get_sysvar::<Clock>();
    clk.unix_timestamp += 365 * 24 * 3600;
    e.svm.set_sysvar(&clk);
    ok(e.invest(0, 0, 100 * M)); // window reset after 365 days
    expect_e(e.invest(2, 1, JUNIOR + 1), E::CapExceeded);
    expect_e(e.close_fundraising(), E::NotFullySubscribed);
    // wrong-mint token account (T4-5)
    let fake = Pubkey::new_unique();
    e.set_token(fake, Pubkey::new_unique(), e.inv[0].pubkey(), 1_000 * M);
    let wk = e.inv[0].insecure_clone();
    expect_fail(e.invest_with(&wk, 0, M, fake), "mint swap (T4-5)");
    let pos = pda(&[SEED_POS, e.deal.as_ref(), &[0], a.as_ref()]);
    let pst = Position::try_deserialize(&mut &e.svm.get_account(&pos).unwrap().data[..]).unwrap();
    assert_eq!(pst.units, 200 * M);
    let i = e.ix(
        ix::CloseFundraising {},
        ac::CloseFundraising {
            admin: x.pubkey(),
            deal: e.deal,
            payment_mint: e.mint,
            raise_vault: e.vault(V_RAISE),
            manufacturer_token: e.t(&e.mfr),
            token_program: spl_token::ID,
        },
    );
    expect_e(e.send("x", i, &[&x]), E::Unauthorized);
}

#[test]
fn t1_close_fundraising_states() {
    let mut e = Env::new(Cfg::a4());
    e.goto_funded();
    expect_e(e.close_fundraising(), E::InvalidState);
    expect_e(e.invest(0, 0, M), E::InvalidState);
}

#[test]
fn t1_schedule_commands() {
    let mut e = Env::new(Cfg::a4());
    e.goto_fundraising();
    expect_e(e.register(0), E::InvalidState); // before funded
    ok(e.invest(0, 0, 400_000 * M));
    ok(e.invest(1, 0, SENIOR - 400_000 * M));
    ok(e.invest(2, 1, JUNIOR));
    ok(e.close_fundraising());
    let rep = e.rep.insecure_clone();
    expect_e(e.register_as(&rep, 0, [1; 32]), E::Unauthorized);
    expect_e(e.register_as(&rep, 3, [1; 32]), E::Unauthorized); // unknown role
    ok(e.register(0));
    expect_e(e.register(0), E::AlreadyRegistered); // anchor cannot change (T4-7)
    expect_e(e.reveal(0, 0), E::InvalidState); // not active yet
    ok(e.register(1));
    ok(e.register(2));
    expect_e(e.register(2), E::InvalidState);

    expect_e(e.reveal(2, 1), E::WrongGroup);
    expect_e(e.reveal(0, 0), E::WrongRevealOrder); // C18: verifier first
    expect_e(e.reveal(1, 0), E::WrongRevealOrder);
    let v = e.ver[0].insecure_clone();
    expect_e(e.reveal_as(&v, 2, 0, [9; 32]), E::HashChainMismatch);
    expect_e(e.reveal_as(&v, 0, 0, e.reveal_value(0, 0)), E::Unauthorized);
    ok(e.reveal(2, 0));
    expect_e(e.reveal(2, 0), E::AlreadyRevealed);
    ok(e.reveal(0, 0));
    expect_e(e.commit(0, 0), E::GroupNotResolved);
    expect_e(e.resolve_missing(0), E::TimeoutNotReached);
    e.warp_timeout();
    let rv = e.bal(e.vault(V_RESERVE));
    ok(e.resolve_missing(0)); // investor rep missing
    let c = e.camp_state();
    assert_eq!(c.status[0], sch::pack_status(sch::NO_OFF, 0b010));
    assert_eq!(e.bal(e.vault(V_RESERVE)) - rv, SLASH);
    assert_eq!(e.deal_state().reserve, SLASH);
    assert_eq!(c.bond_left, [BOND, BOND - SLASH, BOND]);
    expect_e(e.resolve_missing(0), E::AlreadyResolved);
    expect_e(e.reveal(1, 0), E::AlreadyResolved);
    // group 1: the verifier withholds; only the verifier is marked missing (C18)
    e.warp_timeout();
    ok(e.resolve_missing(1));
    assert_eq!(e.camp_state().status[1], sch::pack_status(sch::NO_OFF, 0b100));
    assert_eq!(e.camp_state().bond_left, [BOND, BOND - SLASH, BOND - SLASH]);
    // data commits
    let m = e.mfr.insecure_clone();
    let p = e.payer.insecure_clone();
    expect_e(e.commit_as(&m, 0, 0, [0; 32]), E::Unauthorized);
    expect_e(e.commit_as(&p, 2, 0, [0; 32]), E::InvalidParams);
    expect_e(e.commit(0, 1), E::WrongDataOrder);
    e.commit_both(0);
    e.commit_both(1);
    let c = e.camp_state();
    let mut acc = [0u8; 32];
    for g in 0..2u16 {
        acc = sch::data_acc_next(&acc, g as u32, &root_of(0, g));
    }
    assert_eq!((c.period_snap[0][0], c.snap_ready[0]), (acc, 0b11));
    expect_e(e.release_bonds(), E::InvalidState); // campaign not finished
                                                  // group 2: every role skips one group; payer and investor rep (blocked at group 1) still reveal
    e.resolve_all(2);
    assert!(sch::is_resolved(e.camp_state().status[2]));
}

#[test]
fn t1_skip_limit() {
    let mut e = Env::new(Cfg::new(12, 12, 1));
    e.goto_active();
    for g in 0..8u16 {
        e.play(g, 0b010); // investor rep misses 8 groups in a row
    }
    ok(e.reveal(2, 8));
    ok(e.reveal(0, 8));
    expect_e(e.reveal(1, 8), E::SkipTooLarge);
    assert_eq!(e.camp_state().bond_left[1], 0); // slashed down to zero, never negative
}

#[test]
fn t1_verifier_withholding_does_not_lock_others() {
    let mut e = Env::new(Cfg::new(30, 30, 1));
    e.goto_active();
    for g in 0..12u16 {
        e.play(g, 0b100); // verifier withholds 12 groups
    }
    expect_e(e.reveal(2, 12), E::SkipTooLarge); // the verifier itself is locked out after 8
    let c = e.camp_state();
    assert_eq!(c.bond_left, [BOND, BOND, 0]);
    // payer and investor rep were never allowed to reveal: their skip is not charged, but the group
    // still needs the verifier, so it times out again
    expect_e(e.reveal(0, 12), E::WrongRevealOrder);
    e.warp_timeout();
    ok(e.resolve_missing(12));
    assert_eq!(e.camp_state().status[12], sch::pack_status(sch::NO_OFF, 0b100));
}

#[test]
fn t1_blocked_groups_not_counted_for_payer() {
    let mut f = Env::new(Cfg::new(30, 30, 1));
    f.goto_active();
    for g in 0..7u16 {
        f.play(g, 0b100);
    }
    f.play(7, 0b001); // payer withholds group 7 as well
                      // payer last revealed at the anchor: k = 9 at group 8, own misses = 1 (group 7), blocked = 7
    f.resolve_all(8);
    assert_eq!(f.camp_state().last_group, [8, 8, 8]);
}

#[test]
fn t1_attestation_settle_distribute_close() {
    let mut e = Env::new(Cfg::a4());
    e.goto_active();
    let early = AttestationPayload {
        period: 0,
        lb_bps: 1,
        est_bps: 2,
        ub_bps: 3,
        envelope_hash: ENV_HASH,
        ..Default::default()
    };
    ok(e.approve(0, early));
    expect_e(e.approve(0, early), E::AlreadyApproved);
    expect_e(e.approve(1, early), E::PeriodNotReady);
    e.period_ready(0);
    let x = Keypair::new();
    e.svm.airdrop(&x.pubkey(), 1_000_000_000).unwrap();
    let good = e.payload(0, 3_980_000, 520, 690, 860, 10_000);
    expect_e(e.approve_as(&x, good), E::Unauthorized);
    expect_e(e.approve(1, AttestationPayload { lb_bps: 700, ..good }), E::InvalidRateBounds);
    expect_e(e.approve(1, AttestationPayload { est_bps: 900, ..good }), E::InvalidRateBounds);
    expect_e(e.approve(1, AttestationPayload { ub_bps: 5000, ..good }), E::InvalidRateBounds);
    expect_e(e.approve(1, AttestationPayload { lb_bps: -10_000, ..good }), E::InvalidRateBounds);
    expect_e(e.approve(1, AttestationPayload { compliance_bps: 10_001, ..good }), E::InvalidParams);
    expect_e(e.approve(1, AttestationPayload { flags: 0b1_0000, ..good }), E::InvalidParams);
    expect_e(e.approve(1, AttestationPayload { envelope_hash: [1; 32], ..good }), E::SnapshotMismatch);
    expect_e(e.approve(1, AttestationPayload { period: 1, ..good }), E::PreviousPeriodOpen);
    let bad_root = AttestationPayload { data_root_ops: [1; 32], ..good };
    ok(e.approve(1, bad_root));
    expect_e(e.approve(2, bad_root), E::SnapshotMismatch);
    let bad_ctrl = AttestationPayload { data_root_ctrl: [1; 32], ..good };
    ok(e.approve(1, bad_ctrl));
    expect_e(e.approve(2, bad_ctrl), E::SnapshotMismatch);
    let bad_ovr = AttestationPayload { override_groups: 3, ..good };
    ok(e.approve(1, bad_ovr));
    expect_e(e.approve(2, bad_ovr), E::InvalidParams);
    let bad_inv = AttestationPayload { kg_miss_inv: 1, ..good };
    ok(e.approve(1, bad_inv));
    expect_e(e.approve(2, bad_inv), E::MissingKgInconsistent);
    let huge = AttestationPayload {
        kg_on: u64::MAX,
        price_micro_per_t: u64::MAX,
        lb_bps: 4999,
        est_bps: 4999,
        ub_bps: 4999,
        ..good
    };
    ok(e.approve(1, huge));
    expect_e(e.approve(2, huge), E::MathOverflow);
    // C8: verifier 1 replaces its vote; verifiers 1 and 2 agree
    ok(e.approve(1, good));
    ok(e.approve(2, good));
    let ps = e.period_state(0);
    assert!(ps.finalized && ps.outcome == st::OUT_NORMAL);
    assert_eq!(ps.due, 147_360_759_493);
    expect_e(e.approve(0, good), E::InvalidState); // finalized result cannot be overwritten (T4-8)

    expect_e(e.distribute(0), E::NotSettled);
    let inv = e.inv[0].insecure_clone();
    expect_e(e.settle_as(&inv, 0), E::Unauthorized); // signer swap (T4-2)
    ok(e.settle(0));
    expect_e(e.settle(0), E::AlreadySettled);

    // another deal's vault cannot be substituted (T4-1)
    let (d1, c1) = (e.deal, e.campaign);
    e.cfg.deal_id = 2;
    e.deal = pda(&[SEED_DEAL, e.admin.pubkey().as_ref(), &2u64.to_le_bytes()]);
    e.campaign = pda(&[SEED_CAMPAIGN, e.deal.as_ref(), &[0]]);
    e.goto_fundraising();
    let foreign = e.vault(V_COLLECT);
    e.deal = d1;
    e.campaign = c1;
    e.cfg.deal_id = 1;
    expect_ae(e.distribute_with(0, foreign), AE::ConstraintAddress);
    let own_token = e.t(&e.inv[0]);
    expect_ae(e.distribute_with(0, own_token), AE::ConstraintAddress);
    ok(e.distribute(0));
    expect_e(e.distribute(0), E::AlreadyDistributed);
    expect_ae(e.claim(0, 1), AE::AccountNotInitialized); // no junior position for investor A
    expect_e(e.close_deal(), E::DealNotEnded);

    // period 1: compliance violation, no previous bound change
    e.period_ready(1);
    let p1 = e.payload(1, 3_850_000, 600, 700, 800, 9_000);
    ok(e.approve(0, p1));
    expect_e(e.settle(1), E::NotFinalized);
    ok(e.approve(1, p1));
    let ps = e.period_state(1);
    assert_eq!(ps.outcome, st::OUT_PAYER_VIOLATION);
    assert_eq!(e.deal_state().last_lb_bps, Some(520));
    let want = (w(3_980_000) * 520 / 9480 + w(3_850_000) * 800 / 9200) * 9000 / 10_000;
    assert_eq!(e.deal_state().entitlement as u128, want);
    ok(e.settle(1));
    ok(e.distribute(1));
    expect_e(e.approve(0, AttestationPayload { period: 2, ..p1 }), E::PeriodNotReady);
    ok(e.close_deal());
    expect_e(e.close_deal(), E::InvalidState);
    expect_e(e.reveal(2, 3), E::InvalidState); // closed deal
}

#[test]
fn t1_payer_miss_triggers_violation() {
    let mut e = Env::new(Cfg::a4());
    e.goto_active();
    e.play(0, 0b001); // payer withholds group 0 -> 1/2 groups = 50% > 5%
    e.commit_both(0);
    e.resolve_all(1); // payer returns with k = 2
    e.commit_both(1);
    let p = AttestationPayload { kg_miss_payer: 130_000, ..e.payload(0, 3_850_000, 600, 700, 800, 10_000) };
    let ps = e.finalize(p);
    assert_eq!(ps.outcome, st::OUT_PAYER_VIOLATION);
    // C14: ON fuel at max(ub 800, ref 600); payer-missing fuel at ub
    let want = (w(3_850_000) * 800 / 9200 + w(130_000) * 800 / 9200) * 9000 / 10_000;
    assert_eq!(ps.due as u128, want);
    assert_eq!(e.deal_state().last_lb_bps, None);
}

#[test]
fn t1_flag_bits_on_chain() {
    for (flags, want) in [
        (0b0001u8, st::OUT_PAYER_VIOLATION),
        (0b0010, st::OUT_PAYER_VIOLATION),
        (0b0100, st::OUT_NO_BOUND),
        (0b1000, st::OUT_PAYER_VIOLATION),
        (0b0101, st::OUT_PAYER_VIOLATION),
    ] {
        let mut e = Env::new(Cfg::new(2, 2, 1));
        e.goto_active();
        e.period_ready(0);
        let ps = e.finalize(AttestationPayload { flags, ..e.payload(0, KG, 600, 700, 800, 10_000) });
        assert_eq!(ps.outcome, want, "flags {flags:#b}");
    }
}

#[test]
fn t1_release_bonds_after_campaign() {
    let mut e = Env::new(Cfg::new(2, 2, 1));
    e.goto_active();
    e.play(0, 0b010);
    e.commit_both(0);
    e.resolve_all(1);
    e.commit_both(1);
    expect_e(e.release_bonds(), E::InvalidState); // period not distributed yet
    e.close_period(e.payload(0, KG, 600, 700, 800, 10_000));
    let before = [e.bal(e.t(&e.payer)), e.bal(e.t(&e.rep)), e.bal(e.t(&e.ver[0]))];
    ok(e.release_bonds());
    let after = [e.bal(e.t(&e.payer)), e.bal(e.t(&e.rep)), e.bal(e.t(&e.ver[0]))];
    assert_eq!([after[0] - before[0], after[1] - before[1], after[2] - before[2]], [BOND, BOND - SLASH, BOND]);
    expect_e(e.release_bonds(), E::InvalidState);
    let v = e.ver[0].insecure_clone();
    expect_e(e.reveal_as(&v, 2, 2, [0; 32]), E::WrongGroup); // campaign has 2 groups
}

// =====================================================================================
// T4: security
// =====================================================================================
#[test]
fn t4_foreign_owner_and_fake_accounts() {
    let mut e = Env::new(Cfg::a4());
    e.goto_active();
    let fake = Pubkey::new_unique();
    let real = e.svm.get_account(&e.deal).unwrap();
    e.svm.set_account(fake, SolAccount { owner: system_program::ID, ..real }).unwrap();
    let v = e.ver[0].insecure_clone();
    let val = e.reveal_value(2, 0);
    let i = e.ix(
        ix::Reveal { role: 2, group: 0, value: val },
        ac::Reveal { party: v.pubkey(), deal: fake, campaign: e.campaign },
    );
    expect_ae(e.send("x", i, &[&v]), AE::AccountOwnedByWrongProgram); // T4-4
    let i = e.ix(
        ix::Reveal { role: 2, group: 0, value: val },
        ac::Reveal { party: v.pubkey(), deal: e.deal, campaign: e.vault(V_BOND) },
    );
    expect_fail(e.send("x", i, &[&v]), "campaign of another address");
    ok(e.reveal(2, 0));
}

/// K6: one unauthorized attempt per command (15). Permissionless commands (resolve_missing, distribute,
/// release_bonds) are checked by trying to redirect funds to an attacker account.
#[test]
fn t4_unauthorized_per_command() {
    let mut e = Env::new(Cfg::a4());
    let x = Keypair::new();
    e.svm.airdrop(&x.pubkey(), 10_000_000_000).unwrap();
    let xt = e.add_token(x.pubkey(), 10_000_000 * M);
    let un = |name: &str| rec(format!("UNAUTH {name}"));

    // 1 initialize_deal at the admin's deal address signed by someone else
    let p = e.params();
    let i = e.ix(
        ix::InitializeDeal { params: p.clone() },
        ac::InitializeDeal {
            admin: x.pubkey(),
            deal: e.deal,
            campaign: e.campaign,
            payment_mint: e.mint,
            system_program: system_program::ID,
        },
    );
    expect_ae(e.send("x", i, &[&x]), AE::ConstraintSeeds);
    un("initialize_deal");
    ok(e.init_deal(p));
    // 2 init_vaults
    let i = e.ix(
        ix::InitVaults {},
        ac::InitVaults {
            admin: x.pubkey(),
            deal: e.deal,
            payment_mint: e.mint,
            raise_vault: e.vault(V_RAISE),
            collection_vault: e.vault(V_COLLECT),
            senior_vault: e.vault(V_SENIOR),
            junior_vault: e.vault(V_JUNIOR),
            reserve_vault: e.vault(V_RESERVE),
            bond_vault: e.vault(V_BOND),
            token_program: spl_token::ID,
            system_program: system_program::ID,
        },
    );
    expect_e(e.send_with_vaults("x", i, &x), E::Unauthorized);
    un("init_vaults");
    ok(e.init_vaults());
    // 3 set_kyc
    expect_e(e.set_kyc_as(&x, x.pubkey(), TIER_PRO, u64::MAX), E::Unauthorized);
    un("set_kyc");
    // 4 invest without KYC
    expect_e(e.invest_with(&x, 1, M, xt), E::KycRequired);
    un("invest");
    let ids: Vec<Pubkey> = e.inv.iter().map(|k| k.pubkey()).collect();
    ok(e.set_kyc(ids[0], TIER_RETAIL, 500_000 * M));
    ok(e.set_kyc(ids[1], TIER_RETAIL, 500_000 * M));
    ok(e.set_kyc(ids[2], TIER_PRO, 4_000_000 * M));
    ok(e.invest(0, 0, 400_000 * M));
    ok(e.invest(1, 0, SENIOR - 400_000 * M));
    ok(e.invest(2, 1, JUNIOR));
    // 5 close_fundraising
    let i = e.ix(
        ix::CloseFundraising {},
        ac::CloseFundraising {
            admin: x.pubkey(),
            deal: e.deal,
            payment_mint: e.mint,
            raise_vault: e.vault(V_RAISE),
            manufacturer_token: xt,
            token_program: spl_token::ID,
        },
    );
    expect_e(e.send("x", i, &[&x]), E::Unauthorized);
    un("close_fundraising");
    ok(e.close_fundraising());
    // 6 register_anchor
    let i = e.ix(
        ix::RegisterAnchor { role: 0, anchor: [1; 32] },
        ac::RegisterAnchor {
            party: x.pubkey(),
            deal: e.deal,
            campaign: e.campaign,
            payment_mint: e.mint,
            party_token: xt,
            bond_vault: e.vault(V_BOND),
            token_program: spl_token::ID,
        },
    );
    expect_e(e.send("x", i, &[&x]), E::Unauthorized);
    un("register_anchor");
    for r in 0..3 {
        ok(e.register(r));
    }
    // 7 reveal
    let v = e.reveal_value(2, 0);
    expect_e(e.reveal_as(&x, 2, 0, v), E::Unauthorized);
    un("reveal");
    // 8 resolve_missing: slashed bond cannot be redirected
    e.warp_timeout();
    let i = e.ix(
        ix::ResolveMissing { group: 0 },
        ac::ResolveMissing {
            caller: x.pubkey(),
            deal: e.deal,
            campaign: e.campaign,
            payment_mint: e.mint,
            bond_vault: e.vault(V_BOND),
            reserve_vault: xt,
            token_program: spl_token::ID,
        },
    );
    expect_ae(e.send("x", i, &[&x]), AE::ConstraintAddress);
    un("resolve_missing");
    e.resolve_all(0);
    // 9 commit_group_data
    expect_e(e.commit_as(&x, 0, 0, [0; 32]), E::Unauthorized);
    un("commit_group_data");
    e.commit_both(0);
    e.resolve_all(1);
    e.commit_both(1);
    // 10 approve_attestation
    let pl = e.payload(0, KG, 600, 700, 800, 10_000);
    expect_e(e.approve_as(&x, pl), E::Unauthorized);
    un("approve_attestation");
    e.finalize(pl);
    // 11 settle
    let i = e.ix(
        ix::Settle { period: 0 },
        ac::Settle {
            payer: x.pubkey(),
            deal: e.deal,
            period_state: e.period_pda(0),
            payment_mint: e.mint,
            payer_token: xt,
            collection_vault: e.vault(V_COLLECT),
            token_program: spl_token::ID,
        },
    );
    expect_e(e.send("x", i, &[&x]), E::Unauthorized);
    un("settle");
    ok(e.settle(0));
    // 12 distribute: opex destination fixed
    let i = e.ix(
        ix::Distribute { period: 0 },
        ac::Distribute {
            caller: x.pubkey(),
            deal: e.deal,
            period_state: e.period_pda(0),
            payment_mint: e.mint,
            collection_vault: e.vault(V_COLLECT),
            senior_vault: e.vault(V_SENIOR),
            junior_vault: e.vault(V_JUNIOR),
            reserve_vault: e.vault(V_RESERVE),
            opex_token: xt,
            token_program: spl_token::ID,
        },
    );
    expect_e(e.send("x", i, &[&x]), E::Unauthorized);
    un("distribute");
    ok(e.distribute(0));
    // 13 claim another investor's position
    let pos_a = pda(&[SEED_POS, e.deal.as_ref(), &[0], e.inv[0].pubkey().as_ref()]);
    let i = e.ix(
        ix::Claim { tranche: 0 },
        ac::Claim {
            owner: x.pubkey(),
            deal: e.deal,
            position: pos_a,
            payment_mint: e.mint,
            tranche_vault: e.vault(V_SENIOR),
            owner_token: xt,
            token_program: spl_token::ID,
        },
    );
    expect_ae(e.send("x", i, &[&x]), AE::ConstraintSeeds);
    un("claim");
    // finish the campaign
    e.period_ready(1);
    e.close_period(e.payload(1, KG, 600, 700, 800, 10_000));
    // 14 release_bonds: payer bond cannot be sent to an attacker account
    let i = e.ix(
        ix::ReleaseBonds {},
        ac::ReleaseBonds {
            caller: x.pubkey(),
            deal: e.deal,
            campaign: e.campaign,
            payment_mint: e.mint,
            bond_vault: e.vault(V_BOND),
            payer_token: xt,
            investor_rep_token: e.t(&e.rep),
            verifier_token: e.t(&e.ver[0]),
            token_program: spl_token::ID,
        },
    );
    expect_ae(e.send("x", i, &[&x]), AE::ConstraintTokenOwner);
    un("release_bonds");
    // 15 close_deal
    let i = e.ix(
        ix::CloseDeal {},
        ac::CloseDeal {
            admin: x.pubkey(),
            deal: e.deal,
            payment_mint: e.mint,
            reserve_vault: e.vault(V_RESERVE),
            senior_vault: e.vault(V_SENIOR),
            junior_vault: e.vault(V_JUNIOR),
            opex_token: e.t(&e.opex_owner),
            payer_token: e.t(&e.payer),
            token_program: spl_token::ID,
        },
    );
    expect_e(e.send("x", i, &[&x]), E::Unauthorized);
    un("close_deal");
    ok(e.close_deal());
}

// =====================================================================================
// T6: worst-case compute units
// =====================================================================================
#[test]
fn t6_worst_case_reveal_and_attestation() {
    // payer reveals after 7 own misses plus 7 verifier-blocked groups (largest allowed k = 15)
    let mut e = Env::new(Cfg::new(30, 30, 1));
    e.goto_active();
    for g in 0..7u16 {
        e.play(g, 0b100);
    }
    for g in 7..14u16 {
        e.play(g, 0b001);
    }
    e.resolve_all(14);
    let cu = e.cu["reveal"];
    assert!(cu <= 100_000, "reveal {cu}");
    // attestation over the largest period (512 groups)
    let mut f = Env::new(Cfg::new(512, 512, 1));
    f.goto_active();
    f.period_ready(0);
    f.finalize(f.payload(0, KG, 600, 700, 800, 10_000));
    let cu = f.cu["approve_attestation"];
    assert!(cu <= 100_000, "approve_attestation {cu}");
}

// =====================================================================================
// T3 ⑧: 300 random on-chain flows; vault balances equal the ledger and the host rule model
// =====================================================================================
#[test]
fn t3_onchain_random_flows_300() {
    let mut r = Rng(0x0123_4567_89AB_CDEF);
    for flow in 0..300u64 {
        let gpp = 1 + r.below(4) as u16;
        let periods = 1 + r.below(4) as u16;
        let cfg = Cfg {
            deal_id: flow + 1,
            miss_max: [0u16, 50, 2500, 10_000][r.below(4) as usize],
            opex: r.below(30_000) * M,
            reserve_target: r.below(100_000) * M,
            schedule: (0..periods).map(|_| r.below(60_000) * M).collect(),
            ..Cfg::new(gpp * periods, gpp, periods as usize)
        };
        let mut e = Env::new(cfg.clone());
        e.goto_active();
        let opex0 = e.bal(e.t(&e.opex_owner));
        let mut a = st::Accrual::default();
        let (mut sum_dist, mut claimed) = ([0u64; 3], [0u64; 2]); // opex, senior, junior
        for p in 0..periods {
            let mut withheld = vec![];
            let mut n = st::Counts { groups: gpp as u32, ..Default::default() };
            for g in p * gpp..(p + 1) * gpp {
                if r.below(5) == 0 {
                    let m = [0b001u8, 0b010, 0b100, 0b011][r.below(4) as usize];
                    withheld.push((g, m));
                    n.payer_miss += (m & 1) as u32;
                    n.inv_miss += ((m >> 1) & 1) as u32;
                    n.ver_miss += ((m >> 2) & 1) as u32;
                }
            }
            e.period_with(p, &withheld);
            let lb = r.below(3000) as i32 - 500;
            let ub = (lb.max(0) + r.below(1500) as i32).min(4999);
            let flags = if r.below(4) == 0 { 1u8 << r.below(4) } else { 0 };
            let comp = if r.below(6) == 0 { 9_000 } else { 10_000 };
            let free = gpp as u32 - withheld.len() as u32;
            n.override_groups = (free > 0 && r.below(5) == 0) as u32;
            let miss = if n.payer_miss + n.override_groups > 0 { r.below(200_000) } else { 0 };
            let inv = if n.inv_miss > 0 { r.below(200_000) } else { 0 };
            let pl = AttestationPayload {
                kg_miss_payer: miss,
                kg_miss_inv: inv,
                override_groups: n.override_groups as u16,
                flags,
                ..e.payload(p, r.below(5_000_000), lb, lb.max(0), ub, comp)
            };
            let outcome = st::classify(comp, 9500, flags, &n, cfg.miss_max);
            let pin = st::PeriodInput {
                kg_on: pl.kg_on,
                kg_miss_payer: miss,
                price_micro_per_t: PRICE,
                lb_bps: lb,
                ub_bps: ub,
                outcome,
            };
            let due = st::finalize(&mut a, &pin, 9000, 600).unwrap();
            a.paid_cum += due;
            let ps = e.close_period(pl);
            assert_eq!((ps.due, ps.outcome), (due, outcome), "flow {flow} period {p}");
            let d = e.deal_state();
            assert_eq!((d.entitlement, d.paid_cum, d.last_lb_bps), (a.entitlement, a.paid_cum, a.last_lb_bps));
            sum_dist[0] += ps.dist[0];
            sum_dist[1] += ps.dist[1];
            sum_dist[2] += ps.dist[4];
            for (k, t) in [(0usize, 0u8), (1, 0), (2, 1)] {
                if r.below(2) == 0 {
                    let before = e.bal(e.t(&e.inv[k]));
                    if e.claim(k, t).is_ok() {
                        claimed[t as usize] += e.bal(e.t(&e.inv[k])) - before;
                    }
                }
            }
            // vault balances equal the ledger after every period
            assert_eq!(e.bal(e.vault(V_COLLECT)), 0, "flow {flow}");
            assert_eq!(e.bal(e.vault(V_RESERVE)), d.reserve, "flow {flow}");
            assert_eq!(e.bal(e.vault(V_SENIOR)), sum_dist[1] - claimed[0], "flow {flow}");
            assert_eq!(e.bal(e.vault(V_JUNIOR)), sum_dist[2] - claimed[1], "flow {flow}");
            assert_eq!(e.bal(e.t(&e.opex_owner)) - opex0, sum_dist[0], "flow {flow}");
        }
        // conservation: payer payments plus slashed bonds = opex + tranche vaults + claims + reserve
        let c = e.camp_state();
        let slashed = 3 * BOND - c.bond_left.iter().sum::<u64>();
        let out = sum_dist[0]
            + e.bal(e.vault(V_SENIOR))
            + e.bal(e.vault(V_JUNIOR))
            + e.bal(e.vault(V_RESERVE))
            + claimed[0]
            + claimed[1];
        assert_eq!(out, a.paid_cum + slashed, "flow {flow}");
        rec("T3ONCHAIN 1".to_string());
    }
}
