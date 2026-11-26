//! SaveSail — performance-linked wind-sail financing on Solana.
//! Rules: docs/PRD.md §6. Reference model: offchain/reference/savesail_ref.py.

use anchor_lang::prelude::*;

pub mod error;
pub mod events;
pub mod ix_deal;
pub mod ix_schedule;
pub mod ix_settle;
pub mod logic;
pub mod state;
pub mod util;

pub use ix_deal::*;
pub use ix_schedule::*;
pub use ix_settle::*;
use state::{AttestationPayload, DealParams};

declare_id!("9yjBhvjDC31vYuhc84ajpB9aMtfoYHvoU8GtAoicQaV5");

#[program]
pub mod savesail {
    use super::*;

    pub fn initialize_deal(ctx: Context<InitializeDeal>, params: DealParams) -> Result<()> {
        ix_deal::initialize_deal_handler(ctx, params)
    }
    pub fn init_vaults(ctx: Context<InitVaults>) -> Result<()> {
        ix_deal::init_vaults_handler(ctx)
    }
    pub fn set_kyc(ctx: Context<SetKyc>, investor: Pubkey, tier: u8, annual_limit: u64) -> Result<()> {
        ix_deal::set_kyc_handler(ctx, investor, tier, annual_limit)
    }
    pub fn invest(ctx: Context<Invest>, tranche: u8, amount: u64) -> Result<()> {
        ix_deal::invest_handler(ctx, tranche, amount)
    }
    pub fn close_fundraising(ctx: Context<CloseFundraising>) -> Result<()> {
        ix_deal::close_fundraising_handler(ctx)
    }
    pub fn register_anchor(ctx: Context<RegisterAnchor>, role: u8, anchor: [u8; 32]) -> Result<()> {
        ix_schedule::register_anchor_handler(ctx, role, anchor)
    }
    pub fn reveal(ctx: Context<Reveal>, role: u8, group: u16, value: [u8; 32]) -> Result<()> {
        ix_schedule::reveal_handler(ctx, role, group, value)
    }
    pub fn resolve_missing(ctx: Context<ResolveMissing>, group: u16) -> Result<()> {
        ix_schedule::resolve_missing_handler(ctx, group)
    }
    pub fn commit_group_data(ctx: Context<CommitGroupData>, kind: u8, group: u16, root: [u8; 32]) -> Result<()> {
        ix_schedule::commit_group_data_handler(ctx, kind, group, root)
    }
    pub fn release_bonds(ctx: Context<ReleaseBonds>) -> Result<()> {
        ix_schedule::release_bonds_handler(ctx)
    }
    pub fn approve_attestation(ctx: Context<ApproveAttestation>, payload: AttestationPayload) -> Result<()> {
        ix_settle::approve_attestation_handler(ctx, payload)
    }
    pub fn settle(ctx: Context<Settle>, period: u16) -> Result<()> {
        ix_settle::settle_handler(ctx, period)
    }
    pub fn distribute(ctx: Context<Distribute>, period: u16) -> Result<()> {
        ix_settle::distribute_handler(ctx, period)
    }
    pub fn claim(ctx: Context<Claim>, tranche: u8) -> Result<()> {
        ix_settle::claim_handler(ctx, tranche)
    }
    pub fn close_deal(ctx: Context<CloseDeal>) -> Result<()> {
        ix_settle::close_deal_handler(ctx)
    }
}
