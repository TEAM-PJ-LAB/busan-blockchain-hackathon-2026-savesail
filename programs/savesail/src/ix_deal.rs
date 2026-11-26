//! Deal setup and fundraising: initialize_deal, init_vaults, set_kyc, invest, close_fundraising.

use crate::{error::SaveSailError as E, events::*, state::*, util::*};
use anchor_lang::prelude::*;
use anchor_spl::token_interface::{Mint, TokenAccount, TokenInterface};

#[derive(Accounts)]
#[instruction(params: DealParams)]
pub struct InitializeDeal<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,
    #[account(init, payer = admin, space = 8 + Deal::INIT_SPACE,
        seeds = [SEED_DEAL, admin.key().as_ref(), &params.deal_id.to_le_bytes()], bump)]
    pub deal: Box<Account<'info, Deal>>,
    #[account(init, payer = admin, space = 8 + Campaign::INIT_SPACE,
        seeds = [SEED_CAMPAIGN, deal.key().as_ref(), &[0u8]], bump)]
    pub campaign: Box<Account<'info, Campaign>>,
    pub payment_mint: Box<InterfaceAccount<'info, Mint>>,
    pub system_program: Program<'info, System>,
}

pub fn initialize_deal_handler(ctx: Context<InitializeDeal>, params: DealParams) -> Result<()> {
    let p = params;
    let (gpp, ng, n) = (p.groups_per_period, p.n_groups, p.senior_schedule.len());
    require!(gpp > 0 && ng > 0 && ng as usize <= MAX_GROUPS && ng % gpp == 0, E::InvalidParams);
    let cp = (ng / gpp) as usize;
    require!(cp <= MAX_CAMPAIGN_PERIODS && n >= cp && n <= MAX_PERIODS, E::InvalidParams);
    require!(
        p.caps[0] > 0 && p.caps[1] > 0 && p.caps[0].checked_add(p.caps[1]) == Some(p.install_cost),
        E::InvalidParams
    );
    require!(
        p.phi_bps <= 10_000
            && (0..crate::logic::settle::RATE_MAX_BPS).contains(&p.ref_rate_bps)
            && p.comp_min_bps <= 10_000
            && p.miss_max_bps <= 10_000
            && p.reveal_timeout_slots > 0,
        E::InvalidParams
    );
    let v = p.verifiers;
    require!(v[0] != v[1] && v[0] != v[2] && v[1] != v[2], E::InvalidParams);

    let d = &mut ctx.accounts.deal;
    d.admin = ctx.accounts.admin.key();
    d.deal_id = p.deal_id;
    d.payment_mint = ctx.accounts.payment_mint.key();
    d.payer = p.payer;
    d.investor_rep = p.investor_rep;
    d.verifiers = v;
    d.manufacturer = p.manufacturer;
    d.opex_token = p.opex_token;
    d.envelope_hash = p.envelope_hash;
    d.phi_bps = p.phi_bps;
    d.ref_rate_bps = p.ref_rate_bps;
    d.comp_min_bps = p.comp_min_bps;
    d.miss_max_bps = p.miss_max_bps;
    d.install_cost = p.install_cost;
    d.caps = p.caps;
    d.opex_per_period = p.opex_per_period;
    d.reserve_target = p.reserve_target;
    d.n_periods = n as u16;
    d.senior_schedule[..n].copy_from_slice(&p.senior_schedule);
    d.status = ST_FUNDRAISING;
    d.bump = ctx.bumps.deal;

    let c = &mut ctx.accounts.campaign;
    c.deal = d.key();
    c.n_groups = ng;
    c.groups_per_period = gpp;
    c.reveal_timeout_slots = p.reveal_timeout_slots;
    c.bond_amount = p.bond_amount;
    c.slash_per_miss = p.slash_per_miss;
    c.last_group = [-1; 3];
    c.bump = ctx.bumps.campaign;
    emit!(DealInitialized { deal: d.key(), campaign: c.key() });
    Ok(())
}

#[derive(Accounts)]
pub struct InitVaults<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,
    #[account(mut, has_one = admin @ E::Unauthorized, has_one = payment_mint)]
    pub deal: Box<Account<'info, Deal>>,
    pub payment_mint: Box<InterfaceAccount<'info, Mint>>,
    #[account(init, payer = admin, seeds = [SEED_VAULT, deal.key().as_ref(), &[V_RAISE]], bump,
        token::mint = payment_mint, token::authority = deal, token::token_program = token_program)]
    pub raise_vault: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(init, payer = admin, seeds = [SEED_VAULT, deal.key().as_ref(), &[V_COLLECT]], bump,
        token::mint = payment_mint, token::authority = deal, token::token_program = token_program)]
    pub collection_vault: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(init, payer = admin, seeds = [SEED_VAULT, deal.key().as_ref(), &[V_SENIOR]], bump,
        token::mint = payment_mint, token::authority = deal, token::token_program = token_program)]
    pub senior_vault: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(init, payer = admin, seeds = [SEED_VAULT, deal.key().as_ref(), &[V_JUNIOR]], bump,
        token::mint = payment_mint, token::authority = deal, token::token_program = token_program)]
    pub junior_vault: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(init, payer = admin, seeds = [SEED_VAULT, deal.key().as_ref(), &[V_RESERVE]], bump,
        token::mint = payment_mint, token::authority = deal, token::token_program = token_program)]
    pub reserve_vault: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(init, payer = admin, seeds = [SEED_VAULT, deal.key().as_ref(), &[V_BOND]], bump,
        token::mint = payment_mint, token::authority = deal, token::token_program = token_program)]
    pub bond_vault: Box<InterfaceAccount<'info, TokenAccount>>,
    pub token_program: Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
}

pub fn init_vaults_handler(ctx: Context<InitVaults>) -> Result<()> {
    let d = &mut ctx.accounts.deal;
    require!(d.status == ST_FUNDRAISING && !d.vaults_ready, E::InvalidState);
    d.vaults_ready = true;
    emit!(VaultsInitialized { deal: d.key() });
    Ok(())
}

#[derive(Accounts)]
#[instruction(investor: Pubkey)]
pub struct SetKyc<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,
    #[account(has_one = admin @ E::Unauthorized)]
    pub deal: Box<Account<'info, Deal>>,
    #[account(init_if_needed, payer = admin, space = 8 + Kyc::INIT_SPACE,
        seeds = [SEED_KYC, deal.key().as_ref(), investor.as_ref()], bump)]
    pub kyc: Box<Account<'info, Kyc>>,
    pub system_program: Program<'info, System>,
}

pub fn set_kyc_handler(ctx: Context<SetKyc>, investor: Pubkey, tier: u8, annual_limit: u64) -> Result<()> {
    require!(tier <= TIER_PRO, E::InvalidParams);
    let k = &mut ctx.accounts.kyc;
    if k.deal == Pubkey::default() {
        k.deal = ctx.accounts.deal.key();
        k.investor = investor;
        k.window_start = Clock::get()?.unix_timestamp;
        k.bump = ctx.bumps.kyc;
    }
    k.tier = tier;
    k.annual_limit = annual_limit;
    emit!(KycSet { deal: k.deal, investor, tier, annual_limit });
    Ok(())
}

#[derive(Accounts)]
#[instruction(tranche: u8)]
pub struct Invest<'info> {
    #[account(mut)]
    pub investor: Signer<'info>,
    #[account(mut, has_one = payment_mint)]
    pub deal: Box<Account<'info, Deal>>,
    /// CHECK: address fixed by seeds; read in the handler so a missing record maps to `KycRequired`.
    #[account(mut, seeds = [SEED_KYC, deal.key().as_ref(), investor.key().as_ref()], bump)]
    pub kyc: UncheckedAccount<'info>,
    #[account(init_if_needed, payer = investor, space = 8 + Position::INIT_SPACE,
        seeds = [SEED_POS, deal.key().as_ref(), &[tranche], investor.key().as_ref()], bump)]
    pub position: Box<Account<'info, Position>>,
    pub payment_mint: Box<InterfaceAccount<'info, Mint>>,
    #[account(mut, token::mint = payment_mint, token::authority = investor)]
    pub investor_token: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(mut, seeds = [SEED_VAULT, deal.key().as_ref(), &[V_RAISE]], bump)]
    pub raise_vault: Box<InterfaceAccount<'info, TokenAccount>>,
    pub token_program: Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
}

pub fn invest_handler(ctx: Context<Invest>, tranche: u8, amount: u64) -> Result<()> {
    let a = &ctx.accounts;
    require!(a.deal.status == ST_FUNDRAISING && a.deal.vaults_ready, E::InvalidState);
    require!(tranche <= 1 && amount > 0, E::InvalidParams);
    let t = tranche as usize;

    let ki = a.kyc.to_account_info();
    require!(!ki.data_is_empty() && ki.owner == &crate::ID, E::KycRequired);
    let mut kyc = Kyc::try_deserialize(&mut &ki.data.borrow()[..])?;
    require!(tranche == 0 || kyc.tier == TIER_PRO, E::TierNotAllowed);
    let now = Clock::get()?.unix_timestamp;
    if now >= kyc.window_start.saturating_add(YEAR_SECS) {
        kyc.used = 0;
        kyc.window_start = now;
    }
    let used = kyc.used.checked_add(amount).ok_or(E::MathOverflow)?;
    require!(used <= kyc.annual_limit, E::LimitExceeded);
    let raised = a.deal.raised[t].checked_add(amount).ok_or(E::MathOverflow)?;
    require!(raised <= a.deal.caps[t], E::CapExceeded);

    xfer(
        &a.token_program,
        &a.payment_mint,
        a.investor_token.to_account_info(),
        a.raise_vault.to_account_info(),
        a.investor.to_account_info(),
        amount,
        None,
    )?;
    kyc.used = used;
    kyc.try_serialize(&mut &mut ki.data.borrow_mut()[..])?;

    let (deal_key, owner, acc_t, bump) = (a.deal.key(), a.investor.key(), a.deal.acc[t], ctx.bumps.position);
    let d = &mut ctx.accounts.deal;
    d.raised[t] = raised;
    let pos = &mut ctx.accounts.position;
    if pos.deal == Pubkey::default() {
        pos.deal = deal_key;
        pos.owner = owner;
        pos.tranche = tranche;
        pos.bump = bump;
    }
    pos.units = pos.units.checked_add(amount).ok_or(E::MathOverflow)?;
    pos.acc_snapshot = acc_t;
    emit!(Invested { deal: deal_key, investor: owner, tranche, amount });
    Ok(())
}

#[derive(Accounts)]
pub struct CloseFundraising<'info> {
    pub admin: Signer<'info>,
    #[account(mut, has_one = admin @ E::Unauthorized, has_one = payment_mint)]
    pub deal: Box<Account<'info, Deal>>,
    pub payment_mint: Box<InterfaceAccount<'info, Mint>>,
    #[account(mut, seeds = [SEED_VAULT, deal.key().as_ref(), &[V_RAISE]], bump)]
    pub raise_vault: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(mut, token::mint = payment_mint, token::authority = deal.manufacturer)]
    pub manufacturer_token: Box<InterfaceAccount<'info, TokenAccount>>,
    pub token_program: Interface<'info, TokenInterface>,
}

pub fn close_fundraising_handler(ctx: Context<CloseFundraising>) -> Result<()> {
    let a = &ctx.accounts;
    require!(a.deal.status == ST_FUNDRAISING, E::InvalidState);
    require!(a.deal.raised == a.deal.caps, E::NotFullySubscribed);
    let ds = DealSeeds::of(&a.deal);
    let s = ds.seeds();
    xfer(
        &a.token_program,
        &a.payment_mint,
        a.raise_vault.to_account_info(),
        a.manufacturer_token.to_account_info(),
        a.deal.to_account_info(),
        a.deal.install_cost,
        Some(&[&s]),
    )?;
    let d = &mut ctx.accounts.deal;
    d.status = ST_FUNDED;
    emit!(FundraisingClosed { deal: d.key(), install_cost: d.install_cost });
    Ok(())
}
