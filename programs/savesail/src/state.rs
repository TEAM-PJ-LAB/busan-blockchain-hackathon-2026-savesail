use anchor_lang::prelude::*;

pub const MAX_GROUPS: usize = 512;
pub const MAX_PERIODS: usize = 40;
pub const MAX_CAMPAIGN_PERIODS: usize = 4;
pub const YEAR_SECS: i64 = 365 * 24 * 3600;

pub const SEED_DEAL: &[u8] = b"deal";
pub const SEED_CAMPAIGN: &[u8] = b"campaign";
pub const SEED_VAULT: &[u8] = b"vault";
pub const SEED_KYC: &[u8] = b"kyc";
pub const SEED_POS: &[u8] = b"pos";
pub const SEED_PERIOD: &[u8] = b"period";

/// Vault kinds (PRD §6.2).
pub const V_RAISE: u8 = 0;
pub const V_COLLECT: u8 = 1;
pub const V_SENIOR: u8 = 2;
pub const V_JUNIOR: u8 = 3;
pub const V_RESERVE: u8 = 4;
pub const V_BOND: u8 = 5;

pub const ST_FUNDRAISING: u8 = 0;
pub const ST_FUNDED: u8 = 1;
pub const ST_ACTIVE: u8 = 2;
pub const ST_CLOSED: u8 = 3;

pub const TIER_RETAIL: u8 = 0;
pub const TIER_PRO: u8 = 1;

pub const ROLE_PAYER: u8 = 0;
pub const ROLE_INVESTOR_REP: u8 = 1;
pub const ROLE_VERIFIER: u8 = 2;

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct DealParams {
    pub deal_id: u64,
    pub payer: Pubkey,
    pub investor_rep: Pubkey,
    pub verifiers: [Pubkey; 3],
    pub manufacturer: Pubkey,
    pub opex_token: Pubkey,
    pub envelope_hash: [u8; 32],
    pub phi_bps: u16,
    pub ref_rate_bps: i32,
    pub comp_min_bps: u16,
    pub miss_max_bps: u16,
    pub install_cost: u64,
    pub caps: [u64; 2],
    pub opex_per_period: u64,
    pub reserve_target: u64,
    pub senior_schedule: Vec<u64>,
    pub n_groups: u16,
    pub groups_per_period: u16,
    pub reveal_timeout_slots: u64,
    pub bond_amount: u64,
    pub slash_per_miss: u64,
}

#[account]
#[derive(InitSpace)]
pub struct Deal {
    pub admin: Pubkey,
    pub deal_id: u64,
    pub payment_mint: Pubkey,
    pub payer: Pubkey,
    pub investor_rep: Pubkey,
    pub verifiers: [Pubkey; 3],
    pub manufacturer: Pubkey,
    pub opex_token: Pubkey,
    pub envelope_hash: [u8; 32],
    pub phi_bps: u16,
    pub ref_rate_bps: i32,
    pub comp_min_bps: u16,
    pub miss_max_bps: u16,
    pub install_cost: u64,
    pub caps: [u64; 2],
    pub raised: [u64; 2],
    pub opex_per_period: u64,
    pub reserve_target: u64,
    pub n_periods: u16,
    pub senior_schedule: [u64; MAX_PERIODS],
    pub status: u8,
    pub vaults_ready: bool,
    // accrual (logic::settle::Accrual)
    pub w_norm: u128,
    pub sv_fixed: u128,
    pub last_lb_bps: Option<i32>,
    pub entitlement: u64,
    pub paid_cum: u64,
    pub next_period: u16,
    // book (logic::waterfall::Book)
    pub opex_arrears: u64,
    pub senior_arrears: u64,
    pub reserve: u64,
    pub acc: [u128; 2],
    pub bump: u8,
}

impl Deal {
    pub fn role_key(&self, role: u8) -> Option<Pubkey> {
        match role {
            ROLE_PAYER => Some(self.payer),
            ROLE_INVESTOR_REP => Some(self.investor_rep),
            ROLE_VERIFIER => Some(self.verifiers[0]),
            _ => None,
        }
    }
    pub fn verifier_index(&self, k: &Pubkey) -> Option<usize> {
        self.verifiers.iter().position(|v| v == k)
    }
    pub fn accrual(&self) -> crate::logic::settle::Accrual {
        crate::logic::settle::Accrual {
            w_norm: self.w_norm,
            sv_fixed: self.sv_fixed,
            last_lb_bps: self.last_lb_bps,
            entitlement: self.entitlement,
            paid_cum: self.paid_cum,
        }
    }
    pub fn set_accrual(&mut self, a: &crate::logic::settle::Accrual) {
        self.w_norm = a.w_norm;
        self.sv_fixed = a.sv_fixed;
        self.last_lb_bps = a.last_lb_bps;
        self.entitlement = a.entitlement;
        self.paid_cum = a.paid_cum;
    }
    pub fn book(&self) -> crate::logic::waterfall::Book {
        crate::logic::waterfall::Book {
            opex_arrears: self.opex_arrears,
            senior_arrears: self.senior_arrears,
            reserve: self.reserve,
            acc: self.acc,
        }
    }
    pub fn set_book(&mut self, b: &crate::logic::waterfall::Book) {
        self.opex_arrears = b.opex_arrears;
        self.senior_arrears = b.senior_arrears;
        self.reserve = b.reserve;
        self.acc = b.acc;
    }
}

#[account]
#[derive(InitSpace)]
pub struct Campaign {
    pub deal: Pubkey,
    pub n_groups: u16,
    pub groups_per_period: u16,
    pub reveal_timeout_slots: u64,
    pub bond_amount: u64,
    pub slash_per_miss: u64,
    pub anchors_set: u8,
    pub last_hash: [[u8; 32]; 3],
    pub last_group: [i32; 3],
    pub bond_left: [u64; 3],
    pub current_group: u16,
    pub group_opened_slot: u64,
    pub pending: u8,
    pub pending_val: [[u8; 32]; 3],
    pub status: [u8; MAX_GROUPS],
    pub data_acc: [[u8; 32]; 2],
    pub next_data_group: [u16; 2],
    pub period_snap: [[[u8; 32]; 2]; MAX_CAMPAIGN_PERIODS],
    pub snap_ready: [u8; MAX_CAMPAIGN_PERIODS],
    pub bonds_released: bool,
    pub bump: u8,
}

impl Campaign {
    pub fn periods(&self) -> u16 {
        self.n_groups / self.groups_per_period
    }
}

#[account]
#[derive(InitSpace)]
pub struct Kyc {
    pub deal: Pubkey,
    pub investor: Pubkey,
    pub tier: u8,
    pub annual_limit: u64,
    pub used: u64,
    pub window_start: i64,
    pub bump: u8,
}

#[account]
#[derive(InitSpace)]
pub struct Position {
    pub deal: Pubkey,
    pub owner: Pubkey,
    pub tranche: u8,
    pub units: u64,
    pub acc_snapshot: u128,
    pub claimed: u64,
    pub bump: u8,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, Default, PartialEq, Eq, InitSpace)]
pub struct AttestationPayload {
    pub period: u16,
    pub data_root_ops: [u8; 32],
    pub data_root_ctrl: [u8; 32],
    pub kg_on: u64,
    pub kg_miss_payer: u64,
    pub kg_miss_inv: u64,
    /// Safety overrides below the weather threshold (C17); counted as payer-missing groups.
    pub override_groups: u16,
    pub price_micro_per_t: u64,
    pub lb_bps: i32,
    pub est_bps: i32,
    pub ub_bps: i32,
    pub compliance_bps: u16,
    /// bit 0 covariate imbalance, 1 meter check, 2 too few groups, 3 data quality; bits 4-7 zero.
    pub flags: u8,
    /// sha256 of the verifier envelope; must equal `Deal.envelope_hash`.
    pub envelope_hash: [u8; 32],
    pub report_hash: [u8; 32],
}

#[account]
#[derive(InitSpace)]
pub struct PeriodState {
    pub deal: Pubkey,
    pub period: u16,
    /// sha256(borsh(payload)) voted by each verifier; zero = not voted.
    pub votes: [[u8; 32]; 3],
    pub voted: u8,
    /// Set when two votes match.
    pub payload: AttestationPayload,
    pub payload_hash: [u8; 32],
    pub finalized: bool,
    /// logic::settle::OUT_* (0 normal, 1 payer violation, 2 investor over-cap, 3 no bound).
    pub outcome: u8,
    pub due: u64,
    pub settled: bool,
    pub distributed: bool,
    /// opex, senior, reserve_draw, reserve_in, junior
    pub dist: [u64; 5],
    pub bump: u8,
}
