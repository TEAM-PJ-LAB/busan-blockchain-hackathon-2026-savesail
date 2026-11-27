// Shared LiteSVM harness for the program tests (`program.rs`, `p1.rs`), pulled in with `include!`.

use anchor_lang::{
    error::ErrorCode as AE,
    prelude::*,
    solana_program::{instruction::Instruction, program_option::COption, program_pack::Pack, system_program},
    AccountDeserialize, InstructionData, ToAccountMetas,
};
use anchor_spl::token::spl_token::{
    self,
    state::{Account as TokAcc, AccountState, Mint as TokMint},
};
use litesvm::{
    types::{FailedTransactionMetadata, TransactionMetadata},
    LiteSVM,
};
use savesail::{
    accounts as ac,
    error::SaveSailError as E,
    instruction as ix,
    logic::{schedule as sch, settle as st},
    state::*,
};
use solana_account::Account as SolAccount;
use solana_keypair::Keypair;
use solana_message::{Message, VersionedMessage};
use solana_signer::Signer;
use solana_transaction::versioned::VersionedTransaction;
use std::{collections::HashMap, io::Write, sync::Mutex};

type Res = std::result::Result<TransactionMetadata, Box<FailedTransactionMetadata>>;

const M: u64 = 1_000_000; // 1 dUSD
const PRICE: u64 = 750_000_000;
const SCHED: u64 = 23_494_160_637;
const SENIOR: u64 = 736_000 * M;
const JUNIOR: u64 = 3_264_000 * M;
const BOND: u64 = 1_000 * M;
const SLASH: u64 = 300 * M;
const TIMEOUT: u64 = 150;
const KG: u64 = 3_980_000;
const ENV_HASH: [u8; 32] = [0xE7; 32];

static HITS: Mutex<()> = Mutex::new(());

fn rec(line: String) {
    let _g = HITS.lock().unwrap();
    let path = concat!(env!("CARGO_TARGET_TMPDIR"), "/k2_hits.txt");
    let mut f = std::fs::OpenOptions::new().create(true).append(true).open(path).unwrap();
    writeln!(f, "{line}").unwrap();
}

fn pda(seeds: &[&[u8]]) -> Pubkey {
    Pubkey::find_program_address(seeds, &savesail::ID).0
}

fn custom(r: &Res) -> Option<u32> {
    let s = format!("{:?}", r.as_ref().err()?.err);
    let i = s.find("Custom(")? + 7;
    s[i..].split(')').next()?.parse().ok()
}

#[track_caller]
fn expect_e(r: Res, e: E) {
    let code = 6000 + e as u32;
    assert_eq!(custom(&r), Some(code), "{:#?}", r.err().map(|f| f.meta.logs));
    rec(format!("ERR {code} {e:?}"));
}

#[track_caller]
fn expect_ae(r: Res, e: AE) {
    assert_eq!(custom(&r), Some(e as u32), "{:#?}", r.err().map(|f| f.meta.logs));
    rec(format!("AERR {} {e:?}", e as u32));
}

/// Any rejection (for checks where the runtime, not the program, rejects).
#[track_caller]
fn expect_fail(r: Res, what: &str) {
    assert!(r.is_err(), "{what} should fail");
    rec(format!("FAIL {what}"));
}

#[track_caller]
fn ok(r: Res) -> TransactionMetadata {
    match r {
        Ok(m) => m,
        Err(f) => panic!("tx failed: {:?}\n{:#?}", f.err, f.meta.logs),
    }
}

#[derive(Clone)]
struct Cfg {
    deal_id: u64,
    n_groups: u16,
    gpp: u16,
    schedule: Vec<u64>,
    miss_max: u16,
    opex: u64,
    reserve_target: u64,
}

impl Cfg {
    fn new(n_groups: u16, gpp: u16, periods: usize) -> Self {
        Cfg {
            deal_id: 1,
            n_groups,
            gpp,
            schedule: vec![SCHED; periods],
            miss_max: 50,
            opex: 20_000 * M,
            reserve_target: 2 * SCHED,
        }
    }
    /// Appendix A.4: two periods of two groups, close-out tested.
    fn a4() -> Self {
        Cfg { miss_max: 500, ..Self::new(4, 2, 2) }
    }
}

struct Env {
    svm: LiteSVM,
    admin: Keypair,
    payer: Keypair,
    rep: Keypair,
    ver: [Keypair; 3],
    mfr: Keypair,
    opex_owner: Keypair,
    inv: [Keypair; 3], // A, B retail senior; C pro junior
    mint: Pubkey,
    deal: Pubkey,
    campaign: Pubkey,
    tok: HashMap<Pubkey, Pubkey>,
    chains: [Vec<[u8; 32]>; 3],
    cfg: Cfg,
    cu: HashMap<&'static str, u64>,
}

fn chain(tag: &[u8], n: usize) -> Vec<[u8; 32]> {
    let mut c = vec![sch::h(&[tag])];
    for _ in 0..n {
        let x = sch::h(&[c.last().unwrap()]);
        c.push(x);
    }
    c
}

impl Env {
    fn new(cfg: Cfg) -> Self {
        let mut svm = LiteSVM::new();
        let so = include_bytes!(concat!(env!("CARGO_TARGET_TMPDIR"), "/../deploy/savesail.so"));
        svm.add_program(savesail::ID, so).unwrap();
        let kp = || Keypair::new();
        let (admin, payer, rep, mfr, opex_owner) = (kp(), kp(), kp(), kp(), kp());
        let ver = [kp(), kp(), kp()];
        let inv = [kp(), kp(), kp()];
        let mint = Pubkey::new_unique();
        let mut m = [0u8; TokMint::LEN];
        TokMint::pack(
            TokMint {
                mint_authority: COption::Some(admin.pubkey()),
                supply: u64::MAX / 2,
                decimals: 6,
                is_initialized: true,
                freeze_authority: COption::None,
            },
            &mut m,
        )
        .unwrap();
        svm.set_account(
            mint,
            SolAccount {
                lamports: 1_000_000_000,
                data: m.to_vec(),
                owner: spl_token::ID,
                executable: false,
                rent_epoch: 0,
            },
        )
        .unwrap();
        let deal = pda(&[SEED_DEAL, admin.pubkey().as_ref(), &cfg.deal_id.to_le_bytes()]);
        let campaign = pda(&[SEED_CAMPAIGN, deal.as_ref(), &[0]]);
        let n = cfg.n_groups as usize;
        let mut e = Env {
            svm,
            admin,
            payer,
            rep,
            ver,
            mfr,
            opex_owner,
            inv,
            mint,
            deal,
            campaign,
            tok: HashMap::new(),
            chains: [chain(b"payer", n), chain(b"investor_rep", n), chain(b"verifier", n)],
            cfg,
            cu: HashMap::new(),
        };
        let owners: Vec<Pubkey> = [&e.admin, &e.payer, &e.rep, &e.mfr, &e.opex_owner]
            .into_iter()
            .chain(e.ver.iter())
            .chain(e.inv.iter())
            .map(|k| k.pubkey())
            .collect();
        for o in owners {
            e.svm.airdrop(&o, 10_000_000_000).unwrap();
            e.add_token(o, 10_000_000 * M);
        }
        e
    }

    fn add_token(&mut self, owner: Pubkey, amount: u64) -> Pubkey {
        let a = Pubkey::new_unique();
        self.set_token(a, self.mint, owner, amount);
        self.tok.insert(owner, a);
        a
    }

    fn set_token(&mut self, addr: Pubkey, mint: Pubkey, owner: Pubkey, amount: u64) {
        let mut d = [0u8; TokAcc::LEN];
        TokAcc::pack(
            TokAcc {
                mint,
                owner,
                amount,
                delegate: COption::None,
                state: AccountState::Initialized,
                is_native: COption::None,
                delegated_amount: 0,
                close_authority: COption::None,
            },
            &mut d,
        )
        .unwrap();
        self.svm
            .set_account(
                addr,
                SolAccount {
                    lamports: 1_000_000_000,
                    data: d.to_vec(),
                    owner: spl_token::ID,
                    executable: false,
                    rent_epoch: 0,
                },
            )
            .unwrap();
    }

    fn t(&self, owner: &Keypair) -> Pubkey {
        self.tok[&owner.pubkey()]
    }
    fn bal(&self, a: Pubkey) -> u64 {
        TokAcc::unpack(&self.svm.get_account(&a).unwrap().data).unwrap().amount
    }
    /// Vault keypairs are derived from the deal address, so a test can name another deal's vaults.
    fn vault_kp(&self, k: u8) -> Keypair {
        Keypair::new_from_array(sch::h(&[b"vault", self.deal.as_ref(), &[k]]))
    }
    fn vault(&self, k: u8) -> Pubkey {
        self.vault_kp(k).pubkey()
    }
    fn deal_state(&self) -> Deal {
        Deal::try_deserialize(&mut &self.svm.get_account(&self.deal).unwrap().data[..]).unwrap()
    }
    fn camp_state(&self) -> Campaign {
        Campaign::try_deserialize(&mut &self.svm.get_account(&self.campaign).unwrap().data[..]).unwrap()
    }
    fn period_pda(&self, p: u16) -> Pubkey {
        pda(&[SEED_PERIOD, self.deal.as_ref(), &p.to_le_bytes()])
    }
    fn period_state(&self, p: u16) -> PeriodState {
        PeriodState::try_deserialize(&mut &self.svm.get_account(&self.period_pda(p)).unwrap().data[..]).unwrap()
    }
    fn slot(&self) -> u64 {
        self.svm.get_sysvar::<Clock>().slot
    }

    fn send(&mut self, name: &'static str, i: Instruction, signers: &[&Keypair]) -> Res {
        self.send_ixs(name, &[i], signers)
    }

    /// One transaction; records the CU of a success under `name` ("x" = not recorded).
    fn send_ixs(&mut self, name: &'static str, ixs: &[Instruction], signers: &[&Keypair]) -> Res {
        self.svm.expire_blockhash();
        let bh = self.svm.latest_blockhash();
        let msg = Message::new_with_blockhash(ixs, Some(&signers[0].pubkey()), &bh);
        let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), signers).unwrap();
        let r = self.svm.send_transaction(tx).map_err(Box::new);
        if let Ok(m) = &r {
            let e = self.cu.entry(name).or_insert(0);
            *e = (*e).max(m.compute_units_consumed);
            if name != "x" {
                rec(format!("OK {name} {}", m.compute_units_consumed));
            }
        }
        r
    }

    fn params(&self) -> DealParams {
        let c = &self.cfg;
        DealParams {
            deal_id: c.deal_id,
            payer: self.payer.pubkey(),
            investor_rep: self.rep.pubkey(),
            verifiers: [self.ver[0].pubkey(), self.ver[1].pubkey(), self.ver[2].pubkey()],
            manufacturer: self.mfr.pubkey(),
            opex_token: self.t(&self.opex_owner),
            envelope_hash: ENV_HASH,
            phi_bps: 9000,
            ref_rate_bps: 600,
            comp_min_bps: 9500,
            miss_max_bps: c.miss_max,
            install_cost: SENIOR + JUNIOR,
            caps: [SENIOR, JUNIOR],
            opex_per_period: c.opex,
            reserve_target: c.reserve_target,
            senior_schedule: c.schedule.clone(),
            n_groups: c.n_groups,
            groups_per_period: c.gpp,
            reveal_timeout_slots: TIMEOUT,
            bond_amount: BOND,
            slash_per_miss: SLASH,
        }
    }

    // ---------- instruction builders ----------
    fn ix(&self, data: impl InstructionData, accts: impl ToAccountMetas) -> Instruction {
        Instruction::new_with_bytes(savesail::ID, &data.data(), accts.to_account_metas(None))
    }
    fn init_deal(&mut self, p: DealParams) -> Res {
        let i = self.ix(
            ix::InitializeDeal { params: p },
            ac::InitializeDeal {
                admin: self.admin.pubkey(),
                deal: self.deal,
                campaign: self.campaign,
                payment_mint: self.mint,
                system_program: system_program::ID,
            },
        );
        let a = self.admin.insecure_clone();
        self.send("initialize_deal", i, &[&a])
    }
    fn init_vaults(&mut self) -> Res {
        let i = self.ix(
            ix::InitVaults {},
            ac::InitVaults {
                admin: self.admin.pubkey(),
                deal: self.deal,
                payment_mint: self.mint,
                raise_vault: self.vault(V_RAISE),
                collection_vault: self.vault(V_COLLECT),
                senior_vault: self.vault(V_SENIOR),
                junior_vault: self.vault(V_JUNIOR),
                reserve_vault: self.vault(V_RESERVE),
                bond_vault: self.vault(V_BOND),
                token_program: spl_token::ID,
                system_program: system_program::ID,
            },
        );
        let a = self.admin.insecure_clone();
        self.send_with_vaults("init_vaults", i, &a)
    }
    /// init_vaults needs the six new vault accounts to sign their creation.
    fn send_with_vaults(&mut self, name: &'static str, i: Instruction, signer: &Keypair) -> Res {
        let v: Vec<Keypair> = (V_RAISE..=V_BOND).map(|k| self.vault_kp(k)).collect();
        self.send(name, i, &[signer, &v[0], &v[1], &v[2], &v[3], &v[4], &v[5]])
    }
    fn set_kyc_as(&mut self, signer: &Keypair, investor: Pubkey, tier: u8, limit: u64) -> Res {
        let kyc = pda(&[SEED_KYC, self.deal.as_ref(), investor.as_ref()]);
        let i = self.ix(
            ix::SetKyc { investor, tier, annual_limit: limit },
            ac::SetKyc { admin: signer.pubkey(), deal: self.deal, kyc, system_program: system_program::ID },
        );
        self.send("set_kyc", i, &[signer])
    }
    fn set_kyc(&mut self, investor: Pubkey, tier: u8, limit: u64) -> Res {
        let a = self.admin.insecure_clone();
        self.set_kyc_as(&a, investor, tier, limit)
    }
    fn invest_with(&mut self, who: &Keypair, tranche: u8, amount: u64, token: Pubkey) -> Res {
        let o = who.pubkey();
        let i = self.ix(
            ix::Invest { tranche, amount },
            ac::Invest {
                investor: o,
                deal: self.deal,
                kyc: pda(&[SEED_KYC, self.deal.as_ref(), o.as_ref()]),
                position: pda(&[SEED_POS, self.deal.as_ref(), &[tranche], o.as_ref()]),
                payment_mint: self.mint,
                investor_token: token,
                raise_vault: self.vault(V_RAISE),
                token_program: spl_token::ID,
                system_program: system_program::ID,
            },
        );
        self.send("invest", i, &[who])
    }
    fn invest(&mut self, k: usize, tranche: u8, amount: u64) -> Res {
        let w = self.inv[k].insecure_clone();
        let t = self.t(&w);
        self.invest_with(&w, tranche, amount, t)
    }
    fn close_fundraising(&mut self) -> Res {
        let i = self.ix(
            ix::CloseFundraising {},
            ac::CloseFundraising {
                admin: self.admin.pubkey(),
                deal: self.deal,
                payment_mint: self.mint,
                raise_vault: self.vault(V_RAISE),
                manufacturer_token: self.t(&self.mfr),
                token_program: spl_token::ID,
            },
        );
        let a = self.admin.insecure_clone();
        self.send("close_fundraising", i, &[&a])
    }
    fn role_kp(&self, role: u8) -> Keypair {
        [&self.payer, &self.rep, &self.ver[0]][role as usize].insecure_clone()
    }
    fn register_as(&mut self, who: &Keypair, role: u8, anchor: [u8; 32]) -> Res {
        let i = self.ix(
            ix::RegisterAnchor { role, anchor },
            ac::RegisterAnchor {
                party: who.pubkey(),
                deal: self.deal,
                campaign: self.campaign,
                payment_mint: self.mint,
                party_token: self.t(who),
                bond_vault: self.vault(V_BOND),
                token_program: spl_token::ID,
            },
        );
        self.send("register_anchor", i, &[who])
    }
    fn register(&mut self, role: u8) -> Res {
        let k = self.role_kp(role);
        let n = self.cfg.n_groups as usize;
        self.register_as(&k, role, self.chains[role as usize][n])
    }
    fn reveal_as(&mut self, who: &Keypair, role: u8, group: u16, value: [u8; 32]) -> Res {
        let i = self.ix(
            ix::Reveal { role, group, value },
            ac::Reveal { party: who.pubkey(), deal: self.deal, campaign: self.campaign },
        );
        self.send("reveal", i, &[who])
    }
    fn reveal_value(&self, role: u8, group: u16) -> [u8; 32] {
        let n = self.cfg.n_groups as usize;
        self.chains[role as usize][n - 1 - group as usize]
    }
    fn reveal(&mut self, role: u8, group: u16) -> Res {
        let k = self.role_kp(role);
        let v = self.reveal_value(role, group);
        self.reveal_as(&k, role, group, v)
    }
    fn resolve_missing(&mut self, group: u16) -> Res {
        let c = self.admin.insecure_clone();
        let i = self.ix(
            ix::ResolveMissing { group },
            ac::ResolveMissing {
                caller: c.pubkey(),
                deal: self.deal,
                campaign: self.campaign,
                payment_mint: self.mint,
                bond_vault: self.vault(V_BOND),
                reserve_vault: self.vault(V_RESERVE),
                token_program: spl_token::ID,
            },
        );
        self.send("resolve_missing", i, &[&c])
    }
    fn commit_as(&mut self, who: &Keypair, kind: u8, group: u16, root: [u8; 32]) -> Res {
        let i = self.ix(
            ix::CommitGroupData { kind, group, root },
            ac::CommitGroupData { committer: who.pubkey(), deal: self.deal, campaign: self.campaign },
        );
        self.send("commit_group_data", i, &[who])
    }
    fn commit(&mut self, kind: u8, group: u16) -> Res {
        let k = if kind == 0 { self.payer.insecure_clone() } else { self.mfr.insecure_clone() };
        self.commit_as(&k, kind, group, root_of(kind, group))
    }
    fn approve(&mut self, v: usize, payload: AttestationPayload) -> Res {
        let k = self.ver[v].insecure_clone();
        self.approve_as(&k, payload)
    }
    fn approve_as(&mut self, k: &Keypair, payload: AttestationPayload) -> Res {
        let i = self.ix(
            ix::ApproveAttestation { payload },
            ac::ApproveAttestation {
                verifier: k.pubkey(),
                deal: self.deal,
                campaign: self.campaign,
                period_state: self.period_pda(payload.period),
                system_program: system_program::ID,
            },
        );
        self.send("approve_attestation", i, &[k])
    }
    fn settle_as(&mut self, k: &Keypair, period: u16) -> Res {
        let i = self.ix(
            ix::Settle { period },
            ac::Settle {
                payer: k.pubkey(),
                deal: self.deal,
                period_state: self.period_pda(period),
                payment_mint: self.mint,
                payer_token: self.t(k),
                collection_vault: self.vault(V_COLLECT),
                token_program: spl_token::ID,
            },
        );
        self.send("settle", i, &[k])
    }
    fn settle(&mut self, period: u16) -> Res {
        let k = self.payer.insecure_clone();
        self.settle_as(&k, period)
    }
    fn distribute_with(&mut self, period: u16, collection: Pubkey) -> Res {
        let c = self.admin.insecure_clone();
        let i = self.ix(
            ix::Distribute { period },
            ac::Distribute {
                caller: c.pubkey(),
                deal: self.deal,
                period_state: self.period_pda(period),
                payment_mint: self.mint,
                collection_vault: collection,
                senior_vault: self.vault(V_SENIOR),
                junior_vault: self.vault(V_JUNIOR),
                reserve_vault: self.vault(V_RESERVE),
                opex_token: self.t(&self.opex_owner),
                token_program: spl_token::ID,
            },
        );
        self.send("distribute", i, &[&c])
    }
    fn distribute(&mut self, period: u16) -> Res {
        let v = self.vault(V_COLLECT);
        self.distribute_with(period, v)
    }
    fn claim(&mut self, k: usize, tranche: u8) -> Res {
        let w = self.inv[k].insecure_clone();
        let i = self.ix(
            ix::Claim { tranche },
            ac::Claim {
                owner: w.pubkey(),
                deal: self.deal,
                position: pda(&[SEED_POS, self.deal.as_ref(), &[tranche], w.pubkey().as_ref()]),
                payment_mint: self.mint,
                tranche_vault: self.vault(V_SENIOR + tranche),
                owner_token: self.t(&w),
                token_program: spl_token::ID,
            },
        );
        self.send("claim", i, &[&w])
    }
    fn release_bonds(&mut self) -> Res {
        let c = self.admin.insecure_clone();
        let i = self.ix(
            ix::ReleaseBonds {},
            ac::ReleaseBonds {
                caller: c.pubkey(),
                deal: self.deal,
                campaign: self.campaign,
                payment_mint: self.mint,
                bond_vault: self.vault(V_BOND),
                payer_token: self.t(&self.payer),
                investor_rep_token: self.t(&self.rep),
                verifier_token: self.t(&self.ver[0]),
                token_program: spl_token::ID,
            },
        );
        self.send("release_bonds", i, &[&c])
    }
    fn close_deal(&mut self) -> Res {
        let a = self.admin.insecure_clone();
        let i = self.ix(
            ix::CloseDeal {},
            ac::CloseDeal {
                admin: a.pubkey(),
                deal: self.deal,
                payment_mint: self.mint,
                reserve_vault: self.vault(V_RESERVE),
                senior_vault: self.vault(V_SENIOR),
                junior_vault: self.vault(V_JUNIOR),
                opex_token: self.t(&self.opex_owner),
                payer_token: self.t(&self.payer),
                token_program: spl_token::ID,
            },
        );
        self.send("close_deal", i, &[&a])
    }

    // ---------- scenario steps ----------
    fn goto_fundraising(&mut self) {
        let p = self.params();
        ok(self.init_deal(p));
        ok(self.init_vaults());
        let ids: Vec<Pubkey> = self.inv.iter().map(|k| k.pubkey()).collect();
        ok(self.set_kyc(ids[0], TIER_RETAIL, 500_000 * M));
        ok(self.set_kyc(ids[1], TIER_RETAIL, 500_000 * M));
        ok(self.set_kyc(ids[2], TIER_PRO, 4_000_000 * M));
    }
    fn goto_funded(&mut self) {
        self.goto_fundraising();
        ok(self.invest(0, 0, 400_000 * M));
        ok(self.invest(1, 0, SENIOR - 400_000 * M));
        ok(self.invest(2, 1, JUNIOR));
        ok(self.close_fundraising());
    }
    fn goto_active(&mut self) {
        self.goto_funded();
        for r in 0..3 {
            ok(self.register(r));
        }
    }
    /// Verifier first (C18), then payer and investor rep.
    fn resolve_all(&mut self, g: u16) {
        for r in [2, 0, 1] {
            ok(self.reveal(r, g));
        }
    }
    fn warp_timeout(&mut self) {
        let s = self.slot();
        self.svm.warp_to_slot(s + TIMEOUT);
    }
    /// Plays group `g`; roles in `withhold` (bit per role) do not reveal and the group times out.
    fn play(&mut self, g: u16, withhold: u8) {
        if withhold & 0b100 != 0 {
            self.warp_timeout();
            ok(self.resolve_missing(g));
            return;
        }
        for r in [2u8, 0, 1] {
            if withhold & (1 << r) == 0 {
                ok(self.reveal(r, g));
            }
        }
        if withhold != 0 {
            self.warp_timeout();
            ok(self.resolve_missing(g));
        }
    }
    /// Resolves and commits every group of period `p`, withholding as listed.
    fn period_with(&mut self, p: u16, withheld: &[(u16, u8)]) {
        let gpp = self.cfg.gpp;
        for g in p * gpp..(p + 1) * gpp {
            let w = withheld.iter().find(|x| x.0 == g).map(|x| x.1).unwrap_or(0);
            self.play(g, w);
            self.commit_both(g);
        }
    }
    /// Two matching votes (verifiers 0 and 1).
    fn finalize(&mut self, p: AttestationPayload) -> PeriodState {
        ok(self.approve(0, p));
        ok(self.approve(1, p));
        let ps = self.period_state(p.period);
        assert!(ps.finalized);
        ps
    }
    /// finalize + settle + distribute; returns due.
    fn close_period(&mut self, p: AttestationPayload) -> PeriodState {
        self.finalize(p);
        ok(self.settle(p.period));
        ok(self.distribute(p.period));
        self.period_state(p.period)
    }
    fn commit_both(&mut self, g: u16) {
        ok(self.commit(0, g));
        ok(self.commit(1, g));
    }
    fn payload(&self, p: u16, kg_on: u64, lb: i32, est: i32, ub: i32, comp: u16) -> AttestationPayload {
        let c = self.camp_state();
        AttestationPayload {
            period: p,
            data_root_ops: c.period_snap[p as usize][0],
            data_root_ctrl: c.period_snap[p as usize][1],
            kg_on,
            kg_miss_payer: 0,
            kg_miss_inv: 0,
            override_groups: 0,
            price_micro_per_t: PRICE,
            lb_bps: lb,
            est_bps: est,
            ub_bps: ub,
            compliance_bps: comp,
            flags: 0,
            envelope_hash: ENV_HASH,
            report_hash: [7u8; 32],
        }
    }
    fn period_ready(&mut self, p: u16) {
        self.period_with(p, &[]);
    }
}

fn root_of(kind: u8, group: u16) -> [u8; 32] {
    sch::h(&[&[kind], &group.to_le_bytes()])
}

fn hex(b: &[u8; 32]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn w(kg: u64) -> u128 {
    kg as u128 * PRICE as u128 / 1000
}

/// xorshift64 for reproducible random flows.
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
