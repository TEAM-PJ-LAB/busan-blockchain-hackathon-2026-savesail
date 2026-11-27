//! P1 tests (PRD §2.1): P1-1 Token-2022 tranche tokens gated by Token ACL (4 tests), P1-2 settlement
//! in the Transfer Hook (3 tests), P1-3 Ed25519 proxy approval (6 tests). Build first: `cargo build-sbf`.
//! Token ACL and its ABL gate run from `tests/fixtures/` (sources in the README there); set
//! `P1_FIXTURES=<dir>` to run the same tests on the programs dumped from devnet.
#![allow(dead_code, unused_imports, clippy::needless_range_loop)]

include!("common/harness.rs");

use anchor_lang::{
    compat::solana_instructions_sysvar::ID as IX_SYSVAR,
    solana_program::{instruction::AccountMeta, system_instruction},
};
use anchor_spl::{
    associated_token::{
        get_associated_token_address_with_program_id,
        spl_associated_token_account::instruction::create_associated_token_account,
    },
    compat::spl_token_metadata_interface::{
        instruction as md_ix,
        state::{Field, TokenMetadata},
    },
    token_2022::{
        spl_token_2022::{
            extension::{
                default_account_state::{instruction as das_ix, DefaultAccountState},
                metadata_pointer::instruction as mp_ix,
                transfer_hook::{instruction as th_ix, TransferHook},
                BaseStateWithExtensions, ExtensionType, StateWithExtensions,
            },
            instruction::{self as t22_ix, AuthorityType},
            state::{Account as T22Acc, AccountState as T22State, Mint as T22Mint},
        },
        ID as T22,
    },
};
use savesail::ix_p1::{hook_metas, ED25519_ID, EXECUTE_DISCRIMINATOR, HOOK_METAS_LEN};

const GATE: Pubkey = pubkey!("GATEzzqxhJnsWF6vHRsgtixxSB8PaQdcqGEVTEHWiULz");
const DEC: u8 = 6;
const ACC: u128 = 1_000_000_000_000;

fn fixture(name: &str) -> Vec<u8> {
    let dir =
        std::env::var("P1_FIXTURES").unwrap_or_else(|_| concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures").into());
    std::fs::read(format!("{dir}/{name}")).unwrap_or_else(|e| panic!("{dir}/{name}: {e}"))
}
fn find(seeds: &[&[u8]], prog: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(seeds, prog).0
}
fn mint_config(m: &Pubkey) -> Pubkey {
    find(&[b"MINT_CONFIG", m.as_ref()], &TOKEN_ACL_ID)
}
fn gate_metas(m: &Pubkey) -> Pubkey {
    find(&[b"thaw_extra_account_metas", m.as_ref()], &GATE)
}
fn wallet_entry(list: &Pubkey, w: &Pubkey) -> Pubkey {
    find(&[b"wallet_entry", list.as_ref(), w.as_ref()], &GATE)
}
fn flag(ta: &Pubkey) -> Pubkey {
    find(&[b"FLAG_ACCOUNT", ta.as_ref()], &TOKEN_ACL_ID)
}
fn ata(owner: &Pubkey, mint: &Pubkey) -> Pubkey {
    get_associated_token_address_with_program_id(owner, mint, &T22)
}
fn hook_pda(m: &Pubkey) -> Pubkey {
    pda(&[SEED_HOOK_METAS, m.as_ref()])
}
fn meta(k: Pubkey, w: bool, s: bool) -> AccountMeta {
    if w {
        AccountMeta::new(k, s)
    } else {
        AccountMeta::new_readonly(k, s)
    }
}

/// Knobs for one tranche mint; `good()` meets every P1-1 ① rule.
#[derive(Clone)]
struct MintSpec {
    state: T22State,
    hook: Option<Pubkey>,
    hook_auth: Option<Pubkey>,
    acl: Option<String>,
    to_deal: bool,
    config: bool,
    thaw: bool,
    close_auth: bool,
    decimals: u8,
}
impl MintSpec {
    fn good() -> Self {
        MintSpec {
            state: T22State::Frozen,
            hook: Some(savesail::ID),
            hook_auth: None,
            acl: Some(GATE.to_string()),
            to_deal: true,
            config: true,
            thaw: true,
            close_auth: false,
            decimals: DEC,
        }
    }
}

struct P1 {
    e: Env,
    list: Pubkey,
    mints: [Pubkey; 2],
}

impl P1 {
    /// Fresh environment with Token ACL and the ABL gate loaded, deal in fundraising, one allow list.
    fn bare(cfg: Cfg) -> Self {
        let mut e = Env::new(cfg);
        e.svm.add_program(TOKEN_ACL_ID, &fixture("token_acl.so")).unwrap();
        e.svm.add_program(GATE, &fixture("token_acl_gate.so")).unwrap();
        e.goto_fundraising();
        let mut p = P1 { e, list: Pubkey::default(), mints: [Pubkey::default(); 2] };
        p.list = p.create_list();
        p
    }
    /// `bare` + both tranche mints registered, investors A, B, C allow-listed.
    fn fundraising(cfg: Cfg) -> Self {
        let mut p = Self::bare(cfg);
        let m = [p.create_mint(&MintSpec::good()), p.create_mint(&MintSpec::good())];
        ok(p.register(m));
        p.mints = m;
        for k in 0..3 {
            let w = p.e.inv[k].pubkey();
            p.allow(&w);
        }
        p
    }
    /// `fundraising` + A, B (senior) and C (junior) thaw, invest the full caps; campaign active.
    fn active(cfg: Cfg) -> Self {
        let mut p = Self::fundraising(cfg);
        for (k, t) in [(0usize, 0usize), (1, 0), (2, 1)] {
            let w = p.e.inv[k].insecure_clone();
            p.open_ata(&w, t);
            ok(p.thaw(&w, t));
        }
        let (a, b, c) = (p.e.inv[0].insecure_clone(), p.e.inv[1].insecure_clone(), p.e.inv[2].insecure_clone());
        ok(p.invest_t(&a, 0, 400_000 * M));
        ok(p.invest_t(&b, 0, SENIOR - 400_000 * M));
        ok(p.invest_t(&c, 1, JUNIOR));
        ok(p.e.close_fundraising());
        for r in 0..3 {
            ok(p.e.register(r));
        }
        p
    }

    fn admin(&self) -> Keypair {
        self.e.admin.insecure_clone()
    }

    // ---------- Token ACL / ABL gate (raw instructions; layouts from the program sources) ----------
    fn create_list(&mut self) -> Pubkey {
        let a = self.admin();
        let seed = Pubkey::new_unique();
        let list = find(&[b"list_config", a.pubkey().as_ref(), seed.as_ref()], &GATE);
        let mut d = vec![1u8, 0]; // CreateList, mode Allow
        d.extend_from_slice(seed.as_ref());
        let i = Instruction {
            program_id: GATE,
            accounts: vec![
                meta(a.pubkey(), false, true),
                meta(a.pubkey(), true, true),
                meta(list, true, false),
                meta(system_program::ID, false, false),
            ],
            data: d,
        };
        ok(self.e.send("x", i, &[&a]));
        list
    }
    fn allow(&mut self, w: &Pubkey) {
        let a = self.admin();
        let i = Instruction {
            program_id: GATE,
            accounts: vec![
                meta(a.pubkey(), false, true),
                meta(a.pubkey(), true, true),
                meta(self.list, true, false),
                meta(*w, false, false),
                meta(wallet_entry(&self.list, w), true, false),
                meta(system_program::ID, false, false),
            ],
            data: vec![2], // AddWallet
        };
        ok(self.e.send("x", i, &[&a]));
    }
    /// Token-2022 mint per `spec`, then its Token ACL MintConfig (freeze authority moves to it),
    /// permissionless thaw and the gate's thaw extra metas pointing at the allow list.
    fn create_mint(&mut self, spec: &MintSpec) -> Pubkey {
        let a = self.admin();
        let kp = Keypair::new();
        let (m, ak) = (kp.pubkey(), a.pubkey());
        let mut exts =
            vec![ExtensionType::MetadataPointer, ExtensionType::TransferHook, ExtensionType::DefaultAccountState];
        if spec.close_auth {
            exts.push(ExtensionType::MintCloseAuthority);
        }
        let space = ExtensionType::try_calculate_account_len::<T22Mint>(&exts).unwrap();
        let lamports = self.e.svm.minimum_balance_for_rent_exemption(space + 1024);
        let mut v = vec![
            system_instruction::create_account(&ak, &m, lamports, space as u64, &T22),
            mp_ix::initialize(&T22, &m, Some(ak), Some(m)).unwrap(),
            th_ix::initialize(&T22, &m, spec.hook_auth, spec.hook).unwrap(),
            das_ix::initialize_default_account_state(&T22, &m, &spec.state).unwrap(),
        ];
        if spec.close_auth {
            v.push(t22_ix::initialize_mint_close_authority(&T22, &m, Some(&ak)).unwrap());
        }
        v.push(t22_ix::initialize_mint2(&T22, &m, &ak, Some(&ak), spec.decimals).unwrap());
        v.push(md_ix::initialize(
            &T22,
            &m,
            &ak,
            &m,
            &ak,
            "SaveSail tranche".into(),
            "SST".into(),
            "https://example.com/sst.json".into(),
        ));
        if let Some(val) = &spec.acl {
            v.push(md_ix::update_field(&T22, &m, &ak, Field::Key("token_acl".into()), val.clone()));
        }
        if spec.to_deal {
            v.push(t22_ix::set_authority(&T22, &m, Some(&self.e.deal), AuthorityType::MintTokens, &ak, &[]).unwrap());
        }
        ok(self.e.send_ixs("x", &v, &[&a, &kp]));
        if spec.config {
            let cfg = mint_config(&m);
            let mut d = vec![0u8]; // CreateConfig(gating program)
            d.extend_from_slice(GATE.as_ref());
            let mut v = vec![Instruction {
                program_id: TOKEN_ACL_ID,
                accounts: vec![
                    meta(ak, true, true),
                    meta(ak, false, true),
                    meta(m, true, false),
                    meta(cfg, true, false),
                    meta(system_program::ID, false, false),
                    meta(T22, false, false),
                ],
                data: d,
            }];
            if spec.thaw {
                v.push(Instruction {
                    program_id: TOKEN_ACL_ID,
                    accounts: vec![meta(ak, false, true), meta(cfg, true, false)],
                    data: vec![8, 0, 1], // TogglePermissionlessInstructions(freeze off, thaw on)
                });
            }
            v.push(Instruction {
                program_id: GATE,
                accounts: vec![
                    meta(ak, false, true),
                    meta(ak, true, true),
                    meta(cfg, false, false),
                    meta(m, false, false),
                    meta(gate_metas(&m), true, false),
                    meta(system_program::ID, false, false),
                    meta(self.list, false, false),
                ],
                data: vec![4], // SetupExtraMetas
            });
            ok(self.e.send_ixs("x", &v, &[&a]));
        }
        m
    }
    fn open_ata(&mut self, w: &Keypair, t: usize) -> Pubkey {
        let m = self.mints[t];
        let i = create_associated_token_account(&w.pubkey(), &w.pubkey(), &m, &T22);
        ok(self.e.send("x", i, &[w]));
        ata(&w.pubkey(), &m)
    }
    /// Token ACL permissionless thaw of `owner`'s account `ta`, signed by `signer`.
    fn thaw_ix(&self, signer: &Pubkey, owner: &Pubkey, ta: &Pubkey, t: usize) -> Instruction {
        let m = self.mints[t];
        Instruction {
            program_id: TOKEN_ACL_ID,
            accounts: vec![
                meta(*signer, true, true),
                meta(m, false, false),
                meta(*ta, true, false),
                meta(flag(ta), true, false),
                meta(*owner, false, false),
                meta(mint_config(&m), false, false),
                meta(T22, false, false),
                meta(system_program::ID, false, false),
                meta(GATE, false, false),
                meta(gate_metas(&m), false, false),
                meta(self.list, false, false),
                meta(wallet_entry(&self.list, owner), false, false),
            ],
            data: vec![6], // ThawPermissionless
        }
    }
    fn thaw(&mut self, w: &Keypair, t: usize) -> Res {
        let ta = ata(&w.pubkey(), &self.mints[t]);
        let i = self.thaw_ix(&w.pubkey(), &w.pubkey(), &ta, t);
        self.e.send("x", i, &[w])
    }
    /// Allow-listed holder with payment account, both associated accounts thawed, both positions open.
    fn new_holder(&mut self) -> Keypair {
        let w = Keypair::new();
        self.e.svm.airdrop(&w.pubkey(), 10_000_000_000).unwrap();
        self.e.add_token(w.pubkey(), 0);
        self.allow(&w.pubkey());
        for t in 0..2 {
            self.open_ata(&w, t);
            ok(self.thaw(&w, t));
            let p = w.insecure_clone();
            ok(self.open_position(&p, t as u8, w.pubkey()));
        }
        w
    }

    // ---------- SaveSail P1 commands ----------
    fn register_as(&mut self, who: &Keypair, m: [Pubkey; 2], cfg: [Pubkey; 2]) -> Res {
        let i = self.e.ix(
            ix::RegisterTrancheMints {},
            ac::RegisterTrancheMints {
                admin: who.pubkey(),
                deal: self.e.deal,
                payment_mint: self.e.mint,
                senior_mint: m[0],
                junior_mint: m[1],
                senior_config: cfg[0],
                junior_config: cfg[1],
                senior_metas: hook_pda(&m[0]),
                junior_metas: hook_pda(&m[1]),
                system_program: system_program::ID,
            },
        );
        self.e.send("register_tranche_mints", i, &[who])
    }
    fn register(&mut self, m: [Pubkey; 2]) -> Res {
        let a = self.admin();
        self.register_as(&a, m, [mint_config(&m[0]), mint_config(&m[1])])
    }
    fn open_position(&mut self, payer: &Keypair, t: u8, owner: Pubkey) -> Res {
        let i = self.e.ix(
            ix::OpenPosition { tranche: t, owner },
            ac::OpenPosition {
                payer: payer.pubkey(),
                deal: self.e.deal,
                position: pda(&[SEED_POS, self.e.deal.as_ref(), &[t], owner.as_ref()]),
                system_program: system_program::ID,
            },
        );
        self.e.send("open_position", i, &[payer])
    }
    fn invest_to(&mut self, w: &Keypair, t: u8, amount: u64, to: Pubkey) -> Res {
        let o = w.pubkey();
        let tok = self.e.t(w);
        let mint = self.mints.get(t as usize).copied().unwrap_or_default();
        let i = self.e.ix(
            ix::InvestTokenized { tranche: t, amount },
            ac::InvestTokenized {
                investor: o,
                deal: self.e.deal,
                kyc: pda(&[SEED_KYC, self.e.deal.as_ref(), o.as_ref()]),
                position: pda(&[SEED_POS, self.e.deal.as_ref(), &[t], o.as_ref()]),
                payment_mint: self.e.mint,
                investor_token: tok,
                raise_vault: self.e.vault(V_RAISE),
                token_program: spl_token::ID,
                tranche_mint: mint,
                investor_tranche_token: to,
                tranche_token_program: T22,
                system_program: system_program::ID,
            },
        );
        self.e.send("invest_tokenized", i, &[w])
    }
    fn invest_t(&mut self, w: &Keypair, t: u8, amount: u64) -> Res {
        let to = ata(&w.pubkey(), &self.mints[t as usize]);
        self.invest_to(w, t, amount, to)
    }
    /// `transfer_checked` with the hook's extra accounts, as spl-transfer-hook-interface's
    /// offchain resolver appends them: deal, both positions, hook program, validation account.
    fn xfer_ix(&self, auth: &Pubkey, from: &Pubkey, to: &Pubkey, t: usize, amount: u64) -> Instruction {
        let m = self.mints[t];
        let tb = [t as u8];
        let mut i = t22_ix::transfer_checked(&T22, &ata(from, &m), &m, &ata(to, &m), auth, &[], amount, DEC).unwrap();
        i.accounts.extend([
            meta(self.e.deal, false, false),
            meta(pda(&[SEED_POS, self.e.deal.as_ref(), &tb, from.as_ref()]), true, false),
            meta(pda(&[SEED_POS, self.e.deal.as_ref(), &tb, to.as_ref()]), true, false),
            meta(savesail::ID, false, false),
            meta(hook_pda(&m), false, false),
        ]);
        i
    }
    fn xfer(&mut self, from: &Keypair, to: &Pubkey, t: usize, amount: u64) -> Res {
        let i = self.xfer_ix(&from.pubkey(), &from.pubkey(), to, t, amount);
        self.e.send("transfer_hook", i, &[from])
    }
    fn claim_t(&mut self, w: &Keypair, t: u8) -> Res {
        let o = w.pubkey();
        let mut i = self.e.ix(
            ix::Claim { tranche: t },
            ac::Claim {
                owner: o,
                deal: self.e.deal,
                position: pda(&[SEED_POS, self.e.deal.as_ref(), &[t], o.as_ref()]),
                payment_mint: self.e.mint,
                tranche_vault: self.e.vault(V_SENIOR + t),
                owner_token: self.e.t(w),
                token_program: spl_token::ID,
            },
        );
        i.accounts.push(meta(ata(&o, &self.mints[t as usize]), false, false));
        self.e.send("claim", i, &[w])
    }

    // ---------- reads ----------
    fn pos(&self, t: u8, o: &Pubkey) -> Option<Position> {
        let a = self.e.svm.get_account(&pda(&[SEED_POS, self.e.deal.as_ref(), &[t], o.as_ref()]))?;
        Position::try_deserialize(&mut &a.data[..]).ok()
    }
    fn units(&self, t: u8, o: &Pubkey) -> u64 {
        self.pos(t, o).map(|p| p.units).unwrap_or(0)
    }
    fn tbal_at(&self, a: &Pubkey) -> u64 {
        match self.e.svm.get_account(a) {
            Some(x) if !x.data.is_empty() => StateWithExtensions::<T22Acc>::unpack(&x.data).unwrap().base.amount,
            _ => 0,
        }
    }
    fn tbal(&self, o: &Pubkey, t: usize) -> u64 {
        self.tbal_at(&ata(o, &self.mints[t]))
    }
    fn tstate(&self, o: &Pubkey, t: usize) -> T22State {
        let a = self.e.svm.get_account(&ata(o, &self.mints[t])).unwrap();
        StateWithExtensions::<T22Acc>::unpack(&a.data).unwrap().base.state
    }
    fn supply(&self, t: usize) -> u64 {
        let a = self.e.svm.get_account(&self.mints[t]).unwrap();
        StateWithExtensions::<T22Mint>::unpack(&a.data).unwrap().base.supply
    }
    /// P1-1 ④ for every listed holder and both tranches; supply = raised.
    #[track_caller]
    fn check_units(&self, holders: &[Pubkey]) {
        let d = self.e.deal_state();
        for t in 0..2 {
            for h in holders {
                assert_eq!(self.units(t as u8, h), self.tbal(h, t), "units != balance, tranche {t}, holder {h}");
            }
            assert_eq!(self.supply(t), d.raised[t]);
        }
    }
    /// One full period: groups resolved and committed, two votes, settle, distribute.
    fn period(&mut self, p: u16) {
        self.e.period_ready(p);
        let pl = self.e.payload(p, 3_980_000, 520, 690, 860, 10_000);
        self.e.close_period(pl);
    }
}

// =====================================================================================
// P1-1 Token-2022 tranche tokens + Token ACL
// =====================================================================================

/// ① one mint per tranche, DefaultAccountState=Frozen, `token_acl` metadata; register rejects any
/// mint that breaks a rule, and P0/P1 commands refuse the other mode.
#[test]
fn p1_1a_mints_frozen_default_with_token_acl_metadata() {
    let mut p = P1::bare(Cfg::a4());
    let good = || MintSpec::good();
    let g = [p.create_mint(&good()), p.create_mint(&good())];
    // each broken rule, as senior, with a good junior
    let bad: Vec<(&str, MintSpec)> = vec![
        ("default state Initialized", MintSpec { state: T22State::Initialized, ..good() }),
        ("hook of another program", MintSpec { hook: Some(Pubkey::new_unique()), ..good() }),
        ("hook without program", MintSpec { hook: None, hook_auth: Some(p.e.admin.pubkey()), ..good() }),
        ("hook authority set", MintSpec { hook_auth: Some(p.e.admin.pubkey()), ..good() }),
        ("no token_acl field", MintSpec { acl: None, ..good() }),
        ("token_acl not the gate", MintSpec { acl: Some(Pubkey::new_unique().to_string()), ..good() }),
        ("mint authority not the deal", MintSpec { to_deal: false, ..good() }),
        ("no Token ACL config", MintSpec { config: false, ..good() }),
        ("permissionless thaw off", MintSpec { thaw: false, ..good() }),
        ("extra extension", MintSpec { close_auth: true, ..good() }),
        ("decimals differ", MintSpec { decimals: 9, ..good() }),
    ];
    for (what, spec) in bad {
        let m = p.create_mint(&spec);
        let r = p.register([m, g[1]]);
        assert_eq!(
            custom(&r),
            Some(6000 + E::InvalidTrancheMint as u32),
            "{what}: {:?}",
            r.as_ref().err().map(|f| &f.meta.logs)
        );
        rec(format!("ERR {} InvalidTrancheMint", 6000 + E::InvalidTrancheMint as u32));
    }
    // MintConfig of the other mint
    let a = p.admin();
    expect_e(p.register_as(&a, g, [mint_config(&g[1]), mint_config(&g[1])]), E::InvalidTrancheMint);
    // same mint for both tranches
    expect_e(p.register(g.map(|_| g[0])), E::InvalidTrancheMint);
    // only the admin
    let x = p.e.inv[0].insecure_clone();
    expect_e(p.register_as(&x, g, [mint_config(&g[0]), mint_config(&g[1])]), E::Unauthorized);
    rec("UNAUTH register_tranche_mints".into());
    // P1 commands on a P0 deal
    let w = p.e.inv[0].insecure_clone();
    expect_e(p.open_position(&w, 0, w.pubkey()), E::TrancheTokenMode);
    p.mints = g;
    p.allow(&w.pubkey());
    p.open_ata(&w, 0);
    ok(p.thaw(&w, 0));
    expect_e(p.invest_t(&w, 0, M), E::InvalidTrancheMint); // P0 deal: no tranche mint to match

    ok(p.register(g));
    let d = p.e.deal_state();
    assert_eq!(d.tranche_mints, g);
    expect_e(p.register(g), E::InvalidState);
    // P0 invest refuses a tokenized deal
    expect_e(p.e.invest(0, 0, M), E::TrancheTokenMode);

    for (t, m) in g.iter().enumerate() {
        let acc = p.e.svm.get_account(m).unwrap();
        assert_eq!(acc.owner, T22);
        let s = StateWithExtensions::<T22Mint>::unpack(&acc.data).unwrap();
        let mut types: Vec<u16> = s.get_extension_types().unwrap().into_iter().map(|x| x as u16).collect();
        types.sort();
        let mut want: Vec<u16> = [
            ExtensionType::DefaultAccountState,
            ExtensionType::TransferHook,
            ExtensionType::MetadataPointer,
            ExtensionType::TokenMetadata,
        ]
        .into_iter()
        .map(|x| x as u16)
        .collect();
        want.sort();
        assert_eq!(types, want);
        assert_eq!(s.get_extension::<DefaultAccountState>().unwrap().state, T22State::Frozen as u8);
        assert_eq!(Option::<Pubkey>::from(s.get_extension::<TransferHook>().unwrap().program_id), Some(savesail::ID));
        let md = s.get_variable_len_extension::<TokenMetadata>().unwrap();
        assert!(md.additional_metadata.contains(&("token_acl".to_string(), GATE.to_string())));
        assert_eq!(s.base.mint_authority, COption::Some(p.e.deal));
        assert_eq!(s.base.freeze_authority, COption::Some(mint_config(m)));
        // hook validation account = what spl-tlv-account-resolution writes for the same metas
        let v = p.e.svm.get_account(&hook_pda(m)).unwrap();
        assert_eq!((v.owner, v.data.len()), (savesail::ID, HOOK_METAS_LEN));
        assert_eq!(v.data, hook_metas(&p.e.deal, t as u8));
        assert_eq!(v.data, reference_metas(&p.e.deal, t as u8));
    }
}

/// The validation data built with the interface crates.
fn reference_metas(deal: &Pubkey, t: u8) -> Vec<u8> {
    use spl_tlv_account_resolution::{account::ExtraAccountMeta, seeds::Seed, state::ExtraAccountMetaList};
    use spl_transfer_hook_interface::instruction::ExecuteInstruction;
    let pos = |i: u8| {
        ExtraAccountMeta::new_with_seeds(
            &[
                Seed::Literal { bytes: SEED_POS.to_vec() },
                Seed::AccountKey { index: 5 },
                Seed::Literal { bytes: vec![t] },
                Seed::AccountData { account_index: i, data_index: 32, length: 32 },
            ],
            false,
            true,
        )
        .unwrap()
    };
    let metas = [ExtraAccountMeta::new_with_pubkey(deal, false, false).unwrap(), pos(0), pos(2)];
    let mut d = vec![0u8; ExtraAccountMetaList::size_of(3).unwrap()];
    ExtraAccountMetaList::init::<ExecuteInstruction>(&mut d, &metas).unwrap();
    d
}

/// ② Token ACL MintConfig with the ABL gate, permissionless thaw on, allow list with the wallets,
/// the gate's thaw metas resolving the list (the same setup the devnet run records).
#[test]
fn p1_1b_token_acl_mint_config_and_allow_list() {
    let p = P1::fundraising(Cfg::a4());
    for m in p.mints {
        let c = p.e.svm.get_account(&mint_config(&m)).unwrap();
        assert_eq!((c.owner, c.data.len(), c.data[0]), (TOKEN_ACL_ID, 100, 1));
        assert_eq!((c.data[2], c.data[3]), (1, 0), "thaw on, freeze off");
        assert_eq!(&c.data[4..36], m.as_ref());
        assert_eq!(&c.data[36..68], p.e.admin.pubkey().as_ref());
        assert_eq!(&c.data[68..100], GATE.as_ref());
        let gm = p.e.svm.get_account(&gate_metas(&m)).unwrap();
        assert_eq!(gm.owner, GATE);
        assert!(gm.data.windows(32).any(|w| w == p.list.as_ref()), "gate metas name the list");
    }
    let l = p.e.svm.get_account(&p.list).unwrap();
    assert_eq!((l.owner, l.data[0]), (GATE, 1));
    assert_eq!(l.data[65], 0, "mode Allow");
    assert_eq!(u64::from_le_bytes(l.data[66..74].try_into().unwrap()), 3);
    for k in 0..3 {
        let we = p.e.svm.get_account(&wallet_entry(&p.list, &p.e.inv[k].pubkey())).unwrap();
        assert_eq!(we.owner, GATE);
        assert_eq!(&we.data[1..33], p.e.inv[k].pubkey().as_ref());
    }
}

/// ③ allow-listed wallets thaw their own account and receive (mint and transfer); a wallet off the
/// list cannot thaw (by itself or by anyone else) and cannot receive.
#[test]
fn p1_1c_listed_thaw_and_receive_unlisted_fail() {
    let mut p = P1::fundraising(Cfg::a4());
    let a = p.e.inv[0].insecure_clone();
    p.open_ata(&a, 0);
    assert_eq!(p.tstate(&a.pubkey(), 0), T22State::Frozen, "new accounts start frozen");
    expect_fail(p.invest_t(&a, 0, M), "mint into a frozen account");
    ok(p.thaw(&a, 0));
    assert_eq!(p.tstate(&a.pubkey(), 0), T22State::Initialized);
    ok(p.invest_t(&a, 0, 400_000 * M));
    assert_eq!(p.tbal(&a.pubkey(), 0), 400_000 * M);

    // U: KYC-approved for the deal but not on the allow list
    let u = Keypair::new();
    p.e.svm.airdrop(&u.pubkey(), 10_000_000_000).unwrap();
    p.e.add_token(u.pubkey(), 1_000_000 * M);
    ok(p.e.set_kyc(u.pubkey(), TIER_RETAIL, 500_000 * M));
    p.open_ata(&u, 0);
    let r = p.thaw(&u, 0);
    expect_fail(r, "unlisted self-thaw");
    let ua = ata(&u.pubkey(), &p.mints[0]);
    let i = p.thaw_ix(&a.pubkey(), &u.pubkey(), &ua, 0);
    expect_fail(p.e.send("x", i, &[&a]), "unlisted thaw by a listed wallet");
    assert_eq!(p.tstate(&u.pubkey(), 0), T22State::Frozen);
    expect_fail(p.invest_t(&u, 0, M), "unlisted mint");
    let up = u.insecure_clone();
    ok(p.open_position(&up, 0, u.pubkey()));
    expect_fail(p.xfer(&a, &u.pubkey(), 0, M), "transfer to unlisted");
    assert_eq!(p.tbal(&u.pubkey(), 0), 0);

    // B: listed, thaws, receives by transfer
    let b = p.e.inv[1].insecure_clone();
    p.open_ata(&b, 0);
    ok(p.thaw(&b, 0));
    expect_fail(p.xfer(&a, &b.pubkey(), 0, M), "receiver without a position");
    let bp = b.insecure_clone();
    ok(p.open_position(&bp, 0, b.pubkey()));
    ok(p.xfer(&a, &b.pubkey(), 0, 10_000 * M));
    assert_eq!(p.tbal(&b.pubkey(), 0), 10_000 * M);
    p.check_units(&[a.pubkey(), b.pubkey(), u.pubkey()]);
}

/// ④ position units = token balance after every command: invest, transfer (full, partial, zero,
/// self, by a delegate), distribution, claim. Tokens stay in associated accounts; a holder's own
/// burn (Token-2022 calls no hook) stops earning and is matched at the next transfer or claim.
#[test]
fn p1_1d_units_equal_token_balance() {
    let mut p = P1::active(Cfg::new(8, 2, 4));
    let (a, b, c) = (p.e.inv[0].insecure_clone(), p.e.inv[1].insecure_clone(), p.e.inv[2].insecure_clone());
    let d = p.new_holder();
    let hs = [a.pubkey(), b.pubkey(), c.pubkey(), d.pubkey()];
    p.check_units(&hs);
    ok(p.xfer(&a, &d.pubkey(), 0, 100_000 * M));
    p.check_units(&hs);
    ok(p.xfer(&c, &d.pubkey(), 1, 1_000 * M));
    ok(p.xfer(&c, &d.pubkey(), 1, 1));
    p.check_units(&hs);
    ok(p.xfer(&a, &a.pubkey(), 0, 5 * M)); // self
    ok(p.xfer(&a, &b.pubkey(), 0, 0)); // zero
    p.check_units(&hs);
    p.period(0);
    p.check_units(&hs);
    // delegate D moves A's tokens to B
    let ap = ata(&a.pubkey(), &p.mints[0]);
    let i = t22_ix::approve_checked(&T22, &ap, &p.mints[0], &d.pubkey(), &a.pubkey(), &[], 50_000 * M, DEC).unwrap();
    ok(p.e.send("x", i, &[&a]));
    let i = p.xfer_ix(&d.pubkey(), &a.pubkey(), &b.pubkey(), 0, 50_000 * M);
    ok(p.e.send("transfer_hook", i, &[&d]));
    p.check_units(&hs);
    ok(p.claim_t(&a, 0));
    ok(p.claim_t(&d, 1));
    p.check_units(&hs);
    // full balance out
    let all = p.tbal(&d.pubkey(), 0);
    ok(p.xfer(&d, &b.pubkey(), 0, all));
    expect_fail(p.xfer(&d, &b.pubkey(), 0, 1), "more than the balance");
    p.check_units(&hs);

    // non-associated accounts cannot hold tranche tokens
    let extra = Keypair::new();
    let m = p.mints[0];
    let space = ExtensionType::try_calculate_account_len::<T22Acc>(&[
        ExtensionType::ImmutableOwner,
        ExtensionType::TransferHookAccount,
    ])
    .unwrap();
    let lam = p.e.svm.minimum_balance_for_rent_exemption(space);
    let v = vec![
        system_instruction::create_account(&b.pubkey(), &extra.pubkey(), lam, space as u64, &T22),
        t22_ix::initialize_immutable_owner(&T22, &extra.pubkey()).unwrap(),
        t22_ix::initialize_account3(&T22, &extra.pubkey(), &m, &b.pubkey()).unwrap(),
    ];
    ok(p.e.send_ixs("x", &v, &[&b, &extra]));
    let i = p.thaw_ix(&b.pubkey(), &b.pubkey(), &extra.pubkey(), 0);
    ok(p.e.send("x", i, &[&b]));
    let mut i = p.xfer_ix(&b.pubkey(), &b.pubkey(), &b.pubkey(), 0, M);
    i.accounts[2].pubkey = extra.pubkey();
    expect_e(p.e.send("x", i, &[&b]), E::NotAssociatedTokenAccount);
    let mut i = p.e.ix(
        ix::Claim { tranche: 0 },
        ac::Claim {
            owner: b.pubkey(),
            deal: p.e.deal,
            position: pda(&[SEED_POS, p.e.deal.as_ref(), &[0], b.pubkey().as_ref()]),
            payment_mint: p.e.mint,
            tranche_vault: p.e.vault(V_SENIOR),
            owner_token: p.e.t(&b),
            token_program: spl_token::ID,
        },
    );
    expect_e(p.e.send("x", i.clone(), &[&b]), E::NotAssociatedTokenAccount);
    i.accounts.push(meta(extra.pubkey(), false, false));
    expect_e(p.e.send("x", i, &[&b]), E::NotAssociatedTokenAccount);
    p.check_units(&hs);

    // burn: units above the balance earn nothing and are matched at the next claim
    let burn = 1_000 * M;
    let i = t22_ix::burn_checked(&T22, &ata(&b.pubkey(), &m), &m, &b.pubkey(), &[], burn, DEC).unwrap();
    ok(p.e.send("x", i, &[&b]));
    assert_eq!(p.units(0, &b.pubkey()), p.tbal(&b.pubkey(), 0) + burn);
    let bal_b = p.tbal(&b.pubkey(), 0);
    let acc0 = p.e.deal_state().acc[0];
    let snap = p.pos(0, &b.pubkey()).unwrap();
    p.period(1);
    let before = p.e.bal(p.e.t(&b));
    ok(p.claim_t(&b, 0));
    let got = p.e.bal(p.e.t(&b)) - before;
    let acc1 = p.e.deal_state().acc[0];
    let want = snap.pending as u128 + bal_b as u128 * (acc1 - snap.acc_snapshot) / ACC;
    assert_eq!(got as u128, want, "accrual on the balance, not on burned units");
    assert!(acc1 > acc0);
    assert_eq!(p.units(0, &b.pubkey()), p.tbal(&b.pubkey(), 0));
}

// =====================================================================================
// P1-2 Transfer Hook settlement
// =====================================================================================

/// ① each transfer first settles the sender's and the receiver's unclaimed distributions; the hook
/// refuses calls outside a Token-2022 transfer.
#[test]
fn p1_2a_transfer_settles_both_sides_first() {
    let mut p = P1::active(Cfg::new(8, 2, 4));
    let (a, b) = (p.e.inv[0].insecure_clone(), p.e.inv[1].insecure_clone());
    let d = p.new_holder();
    ok(p.xfer(&a, &d.pubkey(), 0, 100_000 * M));
    p.period(0);
    let acc = p.e.deal_state().acc[0];
    let (pa, pb) = (p.pos(0, &a.pubkey()).unwrap(), p.pos(0, &b.pubkey()).unwrap());
    let ua = pa.units as u128 * (acc - pa.acc_snapshot) / ACC;
    let ub = pb.units as u128 * (acc - pb.acc_snapshot) / ACC;
    assert!(ua > 0 && ub > 0 && pa.pending == 0 && pb.pending == 0);
    ok(p.xfer(&a, &b.pubkey(), 0, 123_456 * M));
    let (qa, qb) = (p.pos(0, &a.pubkey()).unwrap(), p.pos(0, &b.pubkey()).unwrap());
    assert_eq!((qa.pending as u128, qa.acc_snapshot), (ua, acc));
    assert_eq!((qb.pending as u128, qb.acc_snapshot), (ub, acc));
    assert_eq!(qa.units, pa.units - 123_456 * M);
    assert_eq!(qb.units, pb.units + 123_456 * M);
    // the sender keeps what it earned before the transfer, the receiver earns on the new units after
    let ca = p.e.bal(p.e.t(&a));
    ok(p.claim_t(&a, 0));
    assert_eq!((p.e.bal(p.e.t(&a)) - ca) as u128, ua);
    expect_e(p.claim_t(&a, 0), E::NothingToClaim);
    p.period(1);
    let acc2 = p.e.deal_state().acc[0];
    let cb = p.e.bal(p.e.t(&b));
    ok(p.claim_t(&b, 0));
    assert_eq!((p.e.bal(p.e.t(&b)) - cb) as u128, ub + qb.units as u128 * (acc2 - acc) / ACC);

    // direct calls of the hook
    let m = p.mints[0];
    let direct = |p: &P1, mint: Pubkey| Instruction {
        program_id: savesail::ID,
        accounts: vec![
            meta(ata(&a.pubkey(), &m), false, false),
            meta(mint, false, false),
            meta(ata(&b.pubkey(), &m), false, false),
            meta(a.pubkey(), false, true),
            meta(hook_pda(&m), false, false),
            meta(p.e.deal, false, false),
            meta(pda(&[SEED_POS, p.e.deal.as_ref(), &[0], a.pubkey().as_ref()]), true, false),
            meta(pda(&[SEED_POS, p.e.deal.as_ref(), &[0], b.pubkey().as_ref()]), true, false),
        ],
        data: [EXECUTE_DISCRIMINATOR.as_slice(), &(50_000 * M).to_le_bytes()].concat(),
    };
    let i = direct(&p, m);
    expect_e(p.e.send("x", i, &[&a]), E::NotTransferring);
    let i = direct(&p, p.e.mint);
    expect_e(p.e.send("x", i, &[&a]), E::InvalidTrancheMint);
    p.check_units(&[a.pubkey(), b.pubkey(), d.pubkey()]);
}

/// Holder share by exact arithmetic: Σ over distributions of units held × amount / tranche units.
struct Model {
    num: Vec<[u128; 2]>,
    settles: Vec<[u64; 2]>,
    claimed: Vec<[u64; 2]>,
    paid_in: [u128; 2],
    dists: u64,
}

/// ② 1,000 random transfers, distributions and claims in one deal on chain (4 periods + close-out):
/// every holder's claims equal its holding-period share up to rounding, and claims never exceed
/// the vault. The same property over 1,000 random sequences of the settlement rule on the host.
#[test]
fn p1_2b_random_transfers_and_distributions_1000() {
    let mut p = P1::active(Cfg::new(8, 2, 4));
    let mut hs: Vec<Keypair> = (0..3).map(|k| p.e.inv[k].insecure_clone()).collect();
    hs.push(p.new_holder());
    hs.push(p.new_holder());
    // A, B hold no junior; C holds no senior: open the missing positions and accounts
    for (k, t) in [(0usize, 1usize), (1, 1), (2, 0)] {
        let w = hs[k].insecure_clone();
        p.open_ata(&w, t);
        ok(p.thaw(&w, t));
        ok(p.open_position(&w, t as u8, w.pubkey()));
    }
    let keys: Vec<Pubkey> = hs.iter().map(|k| k.pubkey()).collect();
    let units_t = p.e.deal_state().raised;
    let mut md = Model {
        num: vec![[0; 2]; hs.len()],
        settles: vec![[0; 2]; hs.len()],
        claimed: vec![[0; 2]; hs.len()],
        paid_in: [0; 2],
        dists: 0,
    };
    let mut r = Rng(0x51A5_E5EE_D001_2B2B);
    const OPS: usize = 1_000;
    // distribution events at random op indices: periods 0..3, then close-out
    let mut at: Vec<usize> = Vec::new();
    while at.len() < 5 {
        let i = r.below(OPS as u64) as usize;
        if !at.contains(&i) {
            at.push(i);
        }
    }
    at.sort();
    let (mut n_xfer, mut n_claim, mut ev) = (0, 0, 0u16);
    for op in 0..OPS {
        if at.contains(&op) {
            let v0 = [p.e.bal(p.e.vault(V_SENIOR)), p.e.bal(p.e.vault(V_JUNIOR))];
            if ev < 4 {
                p.period(ev);
            } else {
                ok(p.e.release_bonds());
                ok(p.e.close_deal());
            }
            ev += 1;
            let v1 = [p.e.bal(p.e.vault(V_SENIOR)), p.e.bal(p.e.vault(V_JUNIOR))];
            for t in 0..2 {
                let amt = (v1[t] - v0[t]) as u128;
                md.paid_in[t] += amt;
                for (h, k) in keys.iter().enumerate() {
                    md.num[h][t] += p.tbal(k, t) as u128 * amt;
                }
            }
            md.dists += 1;
        } else if r.below(10) < 8 {
            let t = r.below(2) as usize;
            let from: Vec<usize> = (0..hs.len()).filter(|&h| p.tbal(&keys[h], t) > 0).collect();
            let f = from[r.below(from.len() as u64) as usize];
            let to = r.below(hs.len() as u64) as usize;
            let bal = p.tbal(&keys[f], t);
            let amt = match r.below(8) {
                0 => bal,
                1 => 0,
                _ => 1 + r.below(bal),
            };
            let fk = hs[f].insecure_clone();
            ok(p.xfer(&fk, &keys[to], t, amt));
            md.settles[f][t] += 1;
            md.settles[to][t] += 1;
            n_xfer += 1;
        } else {
            let (h, t) = (r.below(hs.len() as u64) as usize, r.below(2) as u8);
            let w = hs[h].insecure_clone();
            let before = p.e.bal(p.e.t(&w));
            match p.claim_t(&w, t) {
                Ok(_) => {
                    md.claimed[h][t as usize] += p.e.bal(p.e.t(&w)) - before;
                    md.settles[h][t as usize] += 1;
                }
                Err(f) => {
                    assert_eq!(custom(&Err(f)), Some(6000 + E::NothingToClaim as u32));
                }
            }
            n_claim += 1;
        }
        p.check_units(&keys);
    }
    assert_eq!(ev, 5);
    for (h, w) in hs.iter().enumerate() {
        for t in 0..2u8 {
            let w2 = w.insecure_clone();
            let before = p.e.bal(p.e.t(&w2));
            if p.claim_t(&w2, t).is_ok() {
                md.claimed[h][t as usize] += p.e.bal(p.e.t(&w2)) - before;
                md.settles[h][t as usize] += 1;
            }
        }
    }
    let mut total = [0u128; 2];
    for h in 0..hs.len() {
        for t in 0..2 {
            let exact = md.num[h][t] / units_t[t] as u128;
            let got = md.claimed[h][t] as u128;
            // rounding: one base unit per settlement, plus the per-unit accumulator floor per distribution
            let tol = md.settles[h][t] as u128 + md.dists as u128 * (units_t[t] as u128 / ACC + 1) + 1;
            assert!(got <= exact && exact - got <= tol, "holder {h} tranche {t}: got {got}, exact {exact}, tol {tol}");
            total[t] += got;
        }
    }
    for t in 0..2 {
        assert!(total[t] <= md.paid_in[t], "claims {} > vault in {}", total[t], md.paid_in[t]);
        assert_eq!(p.e.bal(p.e.vault(V_SENIOR + t as u8)) as u128, md.paid_in[t] - total[t]);
    }
    rec(format!("P1_2B ops {OPS} transfers {n_xfer} claims {n_claim} distributions {}", md.dists));
    println!("P1-2 ② on chain: {OPS} ops, {n_xfer} transfers, {n_claim} claims, {} distributions", md.dists);

    // host: 1,000 random sequences of the same rule (Position::settle + accumulator)
    let mut r = Rng(0xA11C_E5EE_D000_0001);
    for seq in 0..1_000 {
        let n = 2 + r.below(4) as usize;
        let total_units = 1 + r.below(5_000_000_000_000);
        // initial split
        let mut ps: Vec<Position> = vec![dummy_pos(); n];
        let mut left = total_units;
        for i in 0..n {
            let u = if i == n - 1 { left } else { r.below(left + 1) };
            ps[i].units = u;
            left -= u;
        }
        let (mut acc, mut num, mut settles, mut paid) = (0u128, vec![0u128; n], vec![0u64; n], 0u128);
        let steps = 1 + r.below(200);
        for _ in 0..steps {
            if r.below(4) == 0 {
                let amt = r.below(200_000_000_000) as u128;
                acc += amt * ACC / total_units as u128;
                paid += amt;
                for i in 0..n {
                    num[i] += ps[i].units as u128 * amt;
                }
            } else {
                let (f, to) = (r.below(n as u64) as usize, r.below(n as u64) as usize);
                let amt = if ps[f].units == 0 { 0 } else { r.below(ps[f].units + 1) };
                ps[f].settle(acc).unwrap();
                ps[to].settle(acc).unwrap();
                settles[f] += 1;
                settles[to] += 1;
                if f != to {
                    ps[f].units -= amt;
                    ps[to].units += amt;
                }
            }
        }
        let mut sum = 0u128;
        for i in 0..n {
            ps[i].settle(acc).unwrap();
            let got = ps[i].pending as u128;
            let exact = num[i] / total_units as u128;
            let tol = settles[i] as u128 + 1 + steps as u128 * (total_units as u128 / ACC + 1);
            assert!(got <= exact && exact - got <= tol, "seq {seq} holder {i}: got {got} exact {exact}");
            sum += got;
        }
        assert!(sum <= paid, "seq {seq}: {sum} > {paid}");
    }
}

fn dummy_pos() -> Position {
    Position {
        deal: Pubkey::default(),
        owner: Pubkey::default(),
        tranche: 0,
        units: 0,
        acc_snapshot: 0,
        claimed: 0,
        bump: 0,
        pending: 0,
        token_account: Pubkey::default(),
    }
}

/// ③ transfer compute: the whole `transfer_checked` (Token-2022 + hook) and the hook alone stay
/// within 100,000 CU, on the first transfer and after settling many distributions.
#[test]
fn p1_2c_hook_compute_units() {
    let mut p = P1::active(Cfg::new(8, 2, 4));
    let (a, b) = (p.e.inv[0].insecure_clone(), p.e.inv[1].insecure_clone());
    let d = p.new_holder();
    let mut worst = (0u64, 0u64);
    let mut measure = |p: &mut P1, from: &Keypair, to: &Pubkey, t: usize, amt: u64| {
        let i = p.xfer_ix(&from.pubkey(), &from.pubkey(), to, t, amt);
        let m = ok(p.e.send("transfer_hook", i, &[from]));
        let hook = hook_cu(&m.logs);
        worst = (worst.0.max(m.compute_units_consumed), worst.1.max(hook));
    };
    measure(&mut p, &a, &d.pubkey(), 0, 10 * M);
    measure(&mut p, &a, &b.pubkey(), 0, 1);
    for per in 0..4 {
        p.period(per);
        measure(&mut p, &b, &d.pubkey(), 0, 7 * M);
        measure(&mut p, &d, &a.pubkey(), 0, 3 * M);
        measure(&mut p, &a, &a.pubkey(), 0, M);
    }
    let c = p.e.inv[2].insecure_clone();
    measure(&mut p, &c, &d.pubkey(), 1, 99 * M);
    println!("P1-2 ③ transfer {} CU, hook {} CU", worst.0, worst.1);
    rec(format!("P1_2C transfer_cu {} hook_cu {}", worst.0, worst.1));
    assert!(worst.1 > 0 && worst.1 <= 100_000 && worst.0 <= 100_000, "{worst:?}");
}

/// CU the hook program itself reported ("Program <savesail> consumed N of M compute units").
fn hook_cu(logs: &[String]) -> u64 {
    let tag = format!("Program {} consumed ", savesail::ID);
    logs.iter()
        .filter_map(|l| l.strip_prefix(&tag))
        .filter_map(|r| r.split(' ').next()?.parse::<u64>().ok())
        .max()
        .unwrap_or(0)
}

// =====================================================================================
// P1-3 Ed25519 proxy approval
// =====================================================================================

fn msg_of(p: &AttestationPayload) -> [u8; 32] {
    let mut v = Vec::new();
    p.serialize(&mut v).unwrap();
    sch::h(&[&v])
}
/// Ed25519 precompile instruction, layout of `new_ed25519_instruction_with_signature`.
fn ed_ix(signer: &Keypair, msg: &[u8]) -> Instruction {
    let sig = signer.sign_message(msg);
    ed_ix_raw(&signer.pubkey(), sig.as_ref(), msg)
}
fn ed_ix_raw(pk: &Pubkey, sig: &[u8], msg: &[u8]) -> Instruction {
    let mut d = vec![1u8, 0];
    for x in [48u16, u16::MAX, 16, u16::MAX, 112, msg.len() as u16, u16::MAX] {
        d.extend_from_slice(&x.to_le_bytes());
    }
    d.extend_from_slice(pk.as_ref());
    d.extend_from_slice(sig);
    d.extend_from_slice(msg);
    Instruction { program_id: ED25519_ID, accounts: vec![], data: d }
}
fn signed_ix(e: &Env, submitter: &Pubkey, payload: AttestationPayload) -> Instruction {
    e.ix(
        ix::ApproveAttestationSigned { payload },
        ac::ApproveAttestationSigned {
            submitter: *submitter,
            deal: e.deal,
            campaign: e.campaign,
            period_state: e.period_pda(payload.period),
            instructions: IX_SYSVAR,
            system_program: system_program::ID,
        },
    )
}
/// Relayer submits verifier `v`'s off-chain signature over `payload`.
fn relay(e: &mut Env, relayer: &Keypair, v: usize, signed: AttestationPayload, submitted: AttestationPayload) -> Res {
    let pre = ed_ix(&e.ver[v].insecure_clone(), &msg_of(&signed));
    let i = signed_ix(e, &relayer.pubkey(), submitted);
    e.send_ixs("approve_attestation_signed", &[pre, i], &[relayer])
}
/// Votes recorded for period `p` (no account yet = none).
fn voted(e: &Env, p: u16) -> u8 {
    e.svm.get_account(&e.period_pda(p)).map(|_| e.period_state(p).voted).unwrap_or(0)
}
fn relayer(e: &mut Env) -> Keypair {
    let x = Keypair::new();
    e.svm.airdrop(&x.pubkey(), 1_000_000_000).unwrap();
    x
}
fn ready() -> (Env, AttestationPayload) {
    let mut e = Env::new(Cfg::a4());
    e.goto_active();
    e.period_ready(0);
    let pl = e.payload(0, 3_980_000, 520, 690, 860, 10_000);
    (e, pl)
}

/// ① a relayer submits two verifiers' signed votes; the period finalizes exactly as with direct votes.
#[test]
fn p1_3a_proxy_submission_finalizes() {
    let (mut e, pl) = ready();
    let x = relayer(&mut e);
    ok(relay(&mut e, &x, 0, pl, pl));
    assert!(!e.period_state(0).finalized);
    ok(relay(&mut e, &x, 2, pl, pl));
    let ps = e.period_state(0);
    assert!(ps.finalized && ps.voted == 0b101);
    // same due as the direct path (t1_attestation_settle_distribute_close)
    assert_eq!(ps.due, 147_360_759_493);
    // mixed: one direct vote, one relayed
    let (mut f, pl) = ready();
    ok(f.approve(1, pl));
    let x = relayer(&mut f);
    ok(relay(&mut f, &x, 0, pl, pl));
    assert_eq!(f.period_state(0).due, 147_360_759_493);
    ok(f.settle(0));
    ok(f.distribute(0));
}

/// ② a signature by a key that is not a verifier, or a verifier key with someone else's signature.
#[test]
fn p1_3b_other_signer_rejected() {
    let (mut e, pl) = ready();
    let x = relayer(&mut e);
    let pre = ed_ix(&x, &msg_of(&pl));
    let i = signed_ix(&e, &x.pubkey(), pl);
    expect_e(e.send_ixs("x", &[pre, i], &[&x]), E::Unauthorized);
    rec("UNAUTH approve_attestation_signed".into());
    // verifier 0's key, signature made by x: the precompile fails the transaction
    let sig = x.sign_message(&msg_of(&pl));
    let pre = ed_ix_raw(&e.ver[0].pubkey(), sig.as_ref(), &msg_of(&pl));
    let i = signed_ix(&e, &x.pubkey(), pl);
    expect_fail(e.send_ixs("x", &[pre, i], &[&x]), "forged verifier signature");
    assert_eq!(voted(&e, 0), 0);
}

/// ② a valid verifier signature over a different message (other payload, other deal data, raw bytes).
#[test]
fn p1_3c_other_message_rejected() {
    let (mut e, pl) = ready();
    let x = relayer(&mut e);
    let other = AttestationPayload { kg_on: pl.kg_on + 1, ..pl };
    expect_e(relay(&mut e, &x, 0, other, pl), E::InvalidSignature);
    let other = AttestationPayload { report_hash: [9; 32], ..pl };
    expect_e(relay(&mut e, &x, 0, other, pl), E::InvalidSignature);
    // signature over the borsh bytes instead of their sha256
    let mut raw = Vec::new();
    pl.serialize(&mut raw).unwrap();
    let pre = ed_ix(&e.ver[0].insecure_clone(), &raw);
    let i = signed_ix(&e, &x.pubkey(), pl);
    expect_e(e.send_ixs("x", &[pre, i], &[&x]), E::InvalidSignature);
    assert_eq!(voted(&e, 0), 0);
}

/// ② no Ed25519 instruction, or one that is not right before, or whose offsets point elsewhere.
#[test]
fn p1_3d_missing_precompile_rejected() {
    let (mut e, pl) = ready();
    let x = relayer(&mut e);
    let i = signed_ix(&e, &x.pubkey(), pl);
    expect_e(e.send_ixs("x", std::slice::from_ref(&i), &[&x]), E::InvalidSignature);
    // valid signature, but another instruction in between
    let pre = ed_ix(&e.ver[0].insecure_clone(), &msg_of(&pl));
    let memo = system_instruction::transfer(&x.pubkey(), &x.pubkey(), 0);
    expect_e(e.send_ixs("x", &[pre.clone(), memo, i.clone()], &[&x]), E::InvalidSignature);
    // offsets naming instruction 0 explicitly: the precompile accepts, the program wants "this instruction"
    let mut idx = pre.clone();
    idx.data[14..16].copy_from_slice(&0u16.to_le_bytes());
    expect_e(e.send_ixs("x", &[idx, i.clone()], &[&x]), E::InvalidSignature);
    // offsets into another instruction's data (index 1 = the approval itself)
    let mut bad = pre.clone();
    bad.data[14..16].copy_from_slice(&1u16.to_le_bytes());
    expect_fail(e.send_ixs("x", &[bad, i.clone()], &[&x]), "message offsets into another instruction");
    // two signatures in one precompile instruction
    let mut two = pre.clone();
    two.data[0] = 2;
    expect_fail(e.send_ixs("x", &[two, i], &[&x]), "two signatures");
    assert_eq!(voted(&e, 0), 0);
}

/// ② a verifier's signature for another period cannot vote for this one; a replay is rejected.
#[test]
fn p1_3e_other_period_rejected() {
    let (mut e, pl) = ready();
    let x = relayer(&mut e);
    let p1 = AttestationPayload { period: 1, ..pl };
    expect_e(relay(&mut e, &x, 0, p1, pl), E::InvalidSignature);
    // signed for period 1 and submitted as period 1 while period 0 is open
    expect_e(relay(&mut e, &x, 0, p1, p1), E::PreviousPeriodOpen);
    ok(relay(&mut e, &x, 0, pl, pl));
    expect_e(relay(&mut e, &x, 0, pl, pl), E::AlreadyApproved);
    assert_eq!(e.period_state(0).voted, 0b001);
}

/// ③ compute of a relayed vote, the finalizing one over the largest period (512 groups) included.
#[test]
fn p1_3f_proxy_compute_units() {
    let mut f = Env::new(Cfg::new(512, 512, 1));
    f.goto_active();
    f.period_ready(0);
    let pl = f.payload(0, KG, 600, 700, 800, 10_000);
    let x = relayer(&mut f);
    ok(relay(&mut f, &x, 0, pl, pl));
    let first = f.cu["approve_attestation_signed"];
    ok(relay(&mut f, &x, 1, pl, pl));
    assert!(f.period_state(0).finalized);
    let cu = f.cu["approve_attestation_signed"];
    println!("P1-3 ③ relayed vote {first} CU, finalizing {cu} CU");
    rec(format!("P1_3F first_cu {first} final_cu {cu}"));
    assert!(cu <= 100_000, "approve_attestation_signed {cu}");
}
