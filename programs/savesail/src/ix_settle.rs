//! Verification, settlement and payout: approve_attestation, settle, distribute, claim, close_deal.

use crate::{
    error::SaveSailError as E,
    events::*,
    logic::{schedule as sch, settle as st, waterfall as wf},
    state::*,
    util::*,
};
use anchor_lang::prelude::*;
use anchor_spl::token_interface::{Mint, TokenAccount, TokenInterface};

#[derive(Accounts)]
#[instruction(payload: AttestationPayload)]
pub struct ApproveAttestation<'info> {
    #[account(mut)]
    pub verifier: Signer<'info>,
    #[account(mut)]
    pub deal: Box<Account<'info, Deal>>,
    #[account(seeds = [SEED_CAMPAIGN, deal.key().as_ref(), &[0u8]], bump = campaign.bump)]
    pub campaign: Box<Account<'info, Campaign>>,
    #[account(init_if_needed, payer = verifier, space = 8 + PeriodState::INIT_SPACE,
        seeds = [SEED_PERIOD, deal.key().as_ref(), &payload.period.to_le_bytes()], bump)]
    pub period_state: Box<Account<'info, PeriodState>>,
    pub system_program: Program<'info, System>,
}

/// Each verifier votes for sha256(borsh(payload)); two equal votes finalize (2-of-3).
pub fn approve_attestation_handler(ctx: Context<ApproveAttestation>, payload: AttestationPayload) -> Result<()> {
    let vkey = ctx.accounts.verifier.key();
    let vi = ctx.accounts.deal.verifier_index(&vkey).ok_or(E::Unauthorized)?;
    let deal_key = ctx.accounts.deal.key();
    let d = &mut ctx.accounts.deal;
    let c = &ctx.accounts.campaign;
    let ps = &mut ctx.accounts.period_state;
    require!(d.status == ST_ACTIVE, E::InvalidState);
    require!(payload.period >= d.next_period, E::InvalidState);
    require!(payload.period == d.next_period, E::PreviousPeriodOpen);
    require!(payload.period < c.periods(), E::PeriodNotReady);
    // payload-intrinsic checks at every vote; chain-state checks at finalization
    require!(payload.envelope_hash == d.envelope_hash, E::SnapshotMismatch);
    require!(st::rates_valid(payload.lb_bps, payload.est_bps, payload.ub_bps), E::InvalidRateBounds);
    require!(payload.compliance_bps <= 10_000 && payload.flags & !st::F_ALLOWED == 0, E::InvalidParams);
    if ps.deal == Pubkey::default() {
        ps.deal = deal_key;
        ps.period = payload.period;
        ps.bump = ctx.bumps.period_state;
    }
    require!(!ps.finalized, E::InvalidState);
    let bit = 1u8 << vi;
    let ph = sch::h(&[&borsh::to_vec(&payload).map_err(|_| E::InvalidParams)?]);
    // a verifier may replace its own vote until finalization; repeating the same vote is rejected
    require!(ps.voted & bit == 0 || ps.votes[vi] != ph, E::AlreadyApproved);
    ps.votes[vi] = ph;
    ps.voted |= bit;
    emit!(AttestationApproved { deal: deal_key, period: payload.period, verifier: vkey });

    let agree = (0..3).filter(|&i| ps.voted & (1 << i) != 0 && ps.votes[i] == ph).count();
    if agree < 2 {
        return Ok(());
    }
    let p = payload.period as usize;
    let gpp = c.groups_per_period as usize;
    require!(c.snap_ready[p] == 0b11, E::PeriodNotReady);
    require!(
        payload.data_root_ops == c.period_snap[p][0] && payload.data_root_ctrl == c.period_snap[p][1],
        E::SnapshotMismatch
    );
    let mut n =
        st::Counts { groups: gpp as u32, override_groups: payload.override_groups as u32, ..Default::default() };
    let mut missing_groups = 0u32;
    for &sb in &c.status[p * gpp..(p + 1) * gpp] {
        let m = sch::missing_of(sb);
        n.payer_miss += (m & 1) as u32;
        n.inv_miss += ((m >> 1) & 1) as u32;
        n.ver_miss += ((m >> 2) & 1) as u32;
        missing_groups += (m != 0) as u32;
    }
    // missing groups have no schedule, so they cannot be safety overrides (C17)
    require!(n.override_groups + missing_groups <= n.groups, E::InvalidParams);
    require!(
        (n.payer_miss + n.override_groups > 0 || payload.kg_miss_payer == 0)
            && (n.inv_miss > 0 || payload.kg_miss_inv == 0),
        E::MissingKgInconsistent
    );
    let outcome = st::classify(payload.compliance_bps, d.comp_min_bps, payload.flags, &n, d.miss_max_bps);
    let mut a = d.accrual();
    let due = st::finalize(
        &mut a,
        &st::PeriodInput {
            kg_on: payload.kg_on,
            kg_miss_payer: payload.kg_miss_payer,
            price_micro_per_t: payload.price_micro_per_t,
            lb_bps: payload.lb_bps,
            ub_bps: payload.ub_bps,
            outcome,
        },
        d.phi_bps,
        d.ref_rate_bps,
    )
    .map_err(E::from)?;
    d.set_accrual(&a);
    ps.payload = payload;
    ps.payload_hash = ph;
    ps.finalized = true;
    ps.outcome = outcome;
    ps.due = due;
    emit!(AttestationFinalized { deal: deal_key, period: payload.period, due, entitlement: a.entitlement, outcome });
    Ok(())
}

#[derive(Accounts)]
#[instruction(period: u16)]
pub struct Settle<'info> {
    pub payer: Signer<'info>,
    #[account(mut, has_one = payer @ E::Unauthorized, has_one = payment_mint)]
    pub deal: Box<Account<'info, Deal>>,
    #[account(mut, seeds = [SEED_PERIOD, deal.key().as_ref(), &period.to_le_bytes()], bump = period_state.bump)]
    pub period_state: Box<Account<'info, PeriodState>>,
    pub payment_mint: Box<InterfaceAccount<'info, Mint>>,
    #[account(mut, token::mint = payment_mint, token::authority = payer)]
    pub payer_token: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(mut, seeds = [SEED_VAULT, deal.key().as_ref(), &[V_COLLECT]], bump)]
    pub collection_vault: Box<InterfaceAccount<'info, TokenAccount>>,
    pub token_program: Interface<'info, TokenInterface>,
}

pub fn settle_handler(ctx: Context<Settle>, period: u16) -> Result<()> {
    let a = &ctx.accounts;
    require!(a.period_state.finalized, E::NotFinalized);
    require!(!a.period_state.settled, E::AlreadySettled);
    let due = a.period_state.due;
    xfer(
        &a.token_program,
        &a.payment_mint,
        a.payer_token.to_account_info(),
        a.collection_vault.to_account_info(),
        a.payer.to_account_info(),
        due,
        None,
    )?;
    let d = &mut ctx.accounts.deal;
    d.paid_cum = d.paid_cum.checked_add(due).ok_or(E::MathOverflow)?;
    ctx.accounts.period_state.settled = true;
    emit!(Settled { deal: d.key(), period, amount: due });
    Ok(())
}

#[derive(Accounts)]
#[instruction(period: u16)]
pub struct Distribute<'info> {
    pub caller: Signer<'info>,
    #[account(mut, has_one = payment_mint)]
    pub deal: Box<Account<'info, Deal>>,
    #[account(mut, seeds = [SEED_PERIOD, deal.key().as_ref(), &period.to_le_bytes()], bump = period_state.bump)]
    pub period_state: Box<Account<'info, PeriodState>>,
    pub payment_mint: Box<InterfaceAccount<'info, Mint>>,
    #[account(mut, seeds = [SEED_VAULT, deal.key().as_ref(), &[V_COLLECT]], bump)]
    pub collection_vault: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(mut, seeds = [SEED_VAULT, deal.key().as_ref(), &[V_SENIOR]], bump)]
    pub senior_vault: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(mut, seeds = [SEED_VAULT, deal.key().as_ref(), &[V_JUNIOR]], bump)]
    pub junior_vault: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(mut, seeds = [SEED_VAULT, deal.key().as_ref(), &[V_RESERVE]], bump)]
    pub reserve_vault: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(mut, address = deal.opex_token @ E::Unauthorized, token::mint = payment_mint)]
    pub opex_token: Box<InterfaceAccount<'info, TokenAccount>>,
    pub token_program: Interface<'info, TokenInterface>,
}

pub fn distribute_handler(ctx: Context<Distribute>, period: u16) -> Result<()> {
    let a = &ctx.accounts;
    require!(a.period_state.settled, E::NotSettled);
    require!(!a.period_state.distributed, E::AlreadyDistributed);
    let d = &a.deal;
    let mut b = d.book();
    let x = wf::distribute(
        &mut b,
        a.collection_vault.amount,
        d.opex_per_period,
        d.senior_schedule[period as usize],
        d.reserve_target,
        d.caps,
    )
    .map_err(E::from)?;
    let ds = DealSeeds::of(d);
    let s = ds.seeds();
    let sig: &[&[&[u8]]] = &[&s];
    let (col, res, auth) =
        (a.collection_vault.to_account_info(), a.reserve_vault.to_account_info(), a.deal.to_account_info());
    let moves = [
        (col.clone(), a.opex_token.to_account_info(), x.opex),
        (col.clone(), a.senior_vault.to_account_info(), x.senior - x.reserve_draw),
        (res.clone(), a.senior_vault.to_account_info(), x.reserve_draw),
        (col.clone(), res.clone(), x.reserve_in),
        (col, a.junior_vault.to_account_info(), x.junior),
    ];
    for (from, to, amt) in moves {
        xfer(&a.token_program, &a.payment_mint, from, to, auth.clone(), amt, Some(sig))?;
    }
    let deal_key = a.deal.key();
    let d = &mut ctx.accounts.deal;
    d.set_book(&b);
    d.next_period += 1;
    let ps = &mut ctx.accounts.period_state;
    ps.distributed = true;
    ps.dist = [x.opex, x.senior, x.reserve_draw, x.reserve_in, x.junior];
    emit!(Distributed {
        deal: deal_key,
        period,
        opex: x.opex,
        senior: x.senior,
        reserve_draw: x.reserve_draw,
        reserve_in: x.reserve_in,
        junior: x.junior
    });
    Ok(())
}

#[derive(Accounts)]
#[instruction(tranche: u8)]
pub struct Claim<'info> {
    pub owner: Signer<'info>,
    #[account(has_one = payment_mint)]
    pub deal: Box<Account<'info, Deal>>,
    #[account(mut, seeds = [SEED_POS, deal.key().as_ref(), &[tranche], owner.key().as_ref()], bump = position.bump)]
    pub position: Box<Account<'info, Position>>,
    pub payment_mint: Box<InterfaceAccount<'info, Mint>>,
    #[account(mut, seeds = [SEED_VAULT, deal.key().as_ref(), &[V_SENIOR.wrapping_add(tranche)]], bump)]
    pub tranche_vault: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(mut, token::mint = payment_mint, token::authority = owner)]
    pub owner_token: Box<InterfaceAccount<'info, TokenAccount>>,
    pub token_program: Interface<'info, TokenInterface>,
}

pub fn claim_handler(ctx: Context<Claim>, tranche: u8) -> Result<()> {
    let a = &ctx.accounts;
    require!(tranche <= 1, E::InvalidParams);
    let acc = a.deal.acc[tranche as usize];
    let amount = wf::claimable(a.position.units, acc, a.position.acc_snapshot).map_err(E::from)?;
    require!(amount > 0, E::NothingToClaim);
    let ds = DealSeeds::of(&a.deal);
    let s = ds.seeds();
    xfer(
        &a.token_program,
        &a.payment_mint,
        a.tranche_vault.to_account_info(),
        a.owner_token.to_account_info(),
        a.deal.to_account_info(),
        amount,
        Some(&[&s]),
    )?;
    let (deal_key, owner) = (a.deal.key(), a.owner.key());
    let pos = &mut ctx.accounts.position;
    pos.acc_snapshot = acc;
    pos.claimed = pos.claimed.checked_add(amount).ok_or(E::MathOverflow)?;
    emit!(Claimed { deal: deal_key, tranche, owner, amount });
    Ok(())
}

#[derive(Accounts)]
pub struct CloseDeal<'info> {
    pub admin: Signer<'info>,
    #[account(mut, has_one = admin @ E::Unauthorized, has_one = payment_mint)]
    pub deal: Box<Account<'info, Deal>>,
    pub payment_mint: Box<InterfaceAccount<'info, Mint>>,
    #[account(mut, seeds = [SEED_VAULT, deal.key().as_ref(), &[V_RESERVE]], bump)]
    pub reserve_vault: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(mut, seeds = [SEED_VAULT, deal.key().as_ref(), &[V_SENIOR]], bump)]
    pub senior_vault: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(mut, seeds = [SEED_VAULT, deal.key().as_ref(), &[V_JUNIOR]], bump)]
    pub junior_vault: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(mut, address = deal.opex_token @ E::Unauthorized, token::mint = payment_mint)]
    pub opex_token: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(mut, token::mint = payment_mint, token::authority = deal.payer)]
    pub payer_token: Box<InterfaceAccount<'info, TokenAccount>>,
    pub token_program: Interface<'info, TokenInterface>,
}

pub fn close_deal_handler(ctx: Context<CloseDeal>) -> Result<()> {
    let a = &ctx.accounts;
    let d = &a.deal;
    require!(d.status == ST_ACTIVE, E::InvalidState);
    require!(d.next_period == d.n_periods, E::DealNotEnded);
    let mut b = d.book();
    let x = wf::close(&mut b, d.paid_cum, d.entitlement, d.caps).map_err(E::from)?;
    let ds = DealSeeds::of(d);
    let s = ds.seeds();
    let sig: &[&[&[u8]]] = &[&s];
    let (res, auth) = (a.reserve_vault.to_account_info(), a.deal.to_account_info());
    let moves = [
        (a.payer_token.to_account_info(), x.refund),
        (a.opex_token.to_account_info(), x.opex_arrears_paid),
        (a.senior_vault.to_account_info(), x.senior_arrears_paid),
        (a.junior_vault.to_account_info(), x.reserve_to_junior),
    ];
    for (to, amt) in moves {
        xfer(&a.token_program, &a.payment_mint, res.clone(), to, auth.clone(), amt, Some(sig))?;
    }
    let deal_key = a.deal.key();
    let d = &mut ctx.accounts.deal;
    d.set_book(&b);
    d.status = ST_CLOSED;
    emit!(DealClosed { deal: deal_key, refund: x.refund });
    Ok(())
}
