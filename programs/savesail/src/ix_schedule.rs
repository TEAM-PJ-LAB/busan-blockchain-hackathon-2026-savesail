//! Randomized ON/OFF schedule and data commitments:
//! register_anchor, reveal, resolve_missing, commit_group_data, release_bonds.

use crate::{
    error::SaveSailError as E,
    events::*,
    logic::schedule::{self as sch, MAX_SKIP, NO_OFF},
    state::*,
    util::*,
};
use anchor_lang::prelude::*;
use anchor_spl::token_interface::{Mint, TokenAccount, TokenInterface};

fn open_next(c: &mut Campaign) -> Result<()> {
    c.current_group += 1;
    c.pending = 0;
    c.pending_val = [[0u8; 32]; 3];
    c.group_opened_slot = Clock::get()?.slot;
    Ok(())
}

fn check_current(c: &Campaign, group: u16) -> Result<()> {
    require!(group >= c.current_group, E::AlreadyResolved);
    require!(group == c.current_group && group < c.n_groups, E::WrongGroup);
    Ok(())
}

#[derive(Accounts)]
pub struct RegisterAnchor<'info> {
    pub party: Signer<'info>,
    #[account(mut, has_one = payment_mint)]
    pub deal: Box<Account<'info, Deal>>,
    #[account(mut, seeds = [SEED_CAMPAIGN, deal.key().as_ref(), &[0u8]], bump = campaign.bump)]
    pub campaign: Box<Account<'info, Campaign>>,
    pub payment_mint: Box<InterfaceAccount<'info, Mint>>,
    #[account(mut, token::mint = payment_mint, token::authority = party)]
    pub party_token: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(mut, seeds = [SEED_VAULT, deal.key().as_ref(), &[V_BOND]], bump)]
    pub bond_vault: Box<InterfaceAccount<'info, TokenAccount>>,
    pub token_program: Interface<'info, TokenInterface>,
}

pub fn register_anchor_handler(ctx: Context<RegisterAnchor>, role: u8, anchor: [u8; 32]) -> Result<()> {
    let a = &ctx.accounts;
    require!(a.deal.role_key(role) == Some(a.party.key()), E::Unauthorized);
    require!(a.deal.status == ST_FUNDED, E::InvalidState);
    let bit = 1u8 << role;
    require!(a.campaign.anchors_set & bit == 0, E::AlreadyRegistered);
    xfer(
        &a.token_program,
        &a.payment_mint,
        a.party_token.to_account_info(),
        a.bond_vault.to_account_info(),
        a.party.to_account_info(),
        a.campaign.bond_amount,
        None,
    )?;
    let r = role as usize;
    let c = &mut ctx.accounts.campaign;
    c.last_hash[r] = anchor;
    c.last_group[r] = -1;
    c.bond_left[r] = c.bond_amount;
    c.anchors_set |= bit;
    let deal_key = ctx.accounts.deal.key();
    if c.anchors_set == 0b111 {
        c.current_group = 0;
        c.group_opened_slot = Clock::get()?.slot;
        ctx.accounts.deal.status = ST_ACTIVE;
    }
    emit!(AnchorRegistered { deal: deal_key, role, anchor });
    Ok(())
}

#[derive(Accounts)]
pub struct Reveal<'info> {
    pub party: Signer<'info>,
    pub deal: Box<Account<'info, Deal>>,
    #[account(mut, seeds = [SEED_CAMPAIGN, deal.key().as_ref(), &[0u8]], bump = campaign.bump)]
    pub campaign: Box<Account<'info, Campaign>>,
}

pub fn reveal_handler(ctx: Context<Reveal>, role: u8, group: u16, value: [u8; 32]) -> Result<()> {
    let d = &ctx.accounts.deal;
    require!(d.role_key(role) == Some(ctx.accounts.party.key()), E::Unauthorized);
    require!(d.status == ST_ACTIVE, E::InvalidState);
    let campaign_key = ctx.accounts.campaign.key();
    let c = &mut ctx.accounts.campaign;
    check_current(c, group)?;
    let (r, bit) = (role as usize, 1u8 << role);
    require!(c.pending & bit == 0, E::AlreadyRevealed);
    let k = group as i32 - c.last_group[r];
    require!(k <= MAX_SKIP, E::SkipTooLarge);
    require!(sch::verify_reveal(&c.last_hash[r], &value, k), E::HashChainMismatch);
    c.last_hash[r] = value;
    c.last_group[r] = group as i32;
    c.pending |= bit;
    c.pending_val[r] = value;
    emit!(Revealed { deal: d.key(), role, group, value });
    if c.pending == 0b111 {
        let pv = c.pending_val;
        let off = sch::off_index(&campaign_key.to_bytes(), group as u32, [&pv[0], &pv[1], &pv[2]]);
        c.status[group as usize] = sch::pack_status(off, 0);
        open_next(c)?;
        emit!(GroupResolved { deal: d.key(), group, off_index: off, missing_mask: 0 });
    }
    Ok(())
}

#[derive(Accounts)]
pub struct ResolveMissing<'info> {
    pub caller: Signer<'info>,
    #[account(mut, has_one = payment_mint)]
    pub deal: Box<Account<'info, Deal>>,
    #[account(mut, seeds = [SEED_CAMPAIGN, deal.key().as_ref(), &[0u8]], bump = campaign.bump)]
    pub campaign: Box<Account<'info, Campaign>>,
    pub payment_mint: Box<InterfaceAccount<'info, Mint>>,
    #[account(mut, seeds = [SEED_VAULT, deal.key().as_ref(), &[V_BOND]], bump)]
    pub bond_vault: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(mut, seeds = [SEED_VAULT, deal.key().as_ref(), &[V_RESERVE]], bump)]
    pub reserve_vault: Box<InterfaceAccount<'info, TokenAccount>>,
    pub token_program: Interface<'info, TokenInterface>,
}

pub fn resolve_missing_handler(ctx: Context<ResolveMissing>, group: u16) -> Result<()> {
    require!(ctx.accounts.deal.status == ST_ACTIVE, E::InvalidState);
    let deal_key = ctx.accounts.deal.key();
    let c = &mut ctx.accounts.campaign;
    check_current(c, group)?;
    let slot = Clock::get()?.slot;
    require!(slot >= c.group_opened_slot.saturating_add(c.reveal_timeout_slots), E::TimeoutNotReached);
    let missing = !c.pending & 0b111;
    let mut total = 0u64;
    for r in 0..3u8 {
        if missing & (1 << r) != 0 {
            let s = c.slash_per_miss.min(c.bond_left[r as usize]);
            c.bond_left[r as usize] -= s;
            total += s;
            emit!(BondSlashed { deal: deal_key, role: r, amount: s });
        }
    }
    c.status[group as usize] = sch::pack_status(NO_OFF, missing);
    open_next(c)?;
    emit!(GroupResolved { deal: deal_key, group, off_index: NO_OFF, missing_mask: missing });

    let a = &ctx.accounts;
    let ds = DealSeeds::of(&a.deal);
    let s = ds.seeds();
    xfer(
        &a.token_program,
        &a.payment_mint,
        a.bond_vault.to_account_info(),
        a.reserve_vault.to_account_info(),
        a.deal.to_account_info(),
        total,
        Some(&[&s]),
    )?;
    let d = &mut ctx.accounts.deal;
    d.reserve = d.reserve.checked_add(total).ok_or(E::MathOverflow)?;
    Ok(())
}

#[derive(Accounts)]
pub struct CommitGroupData<'info> {
    pub committer: Signer<'info>,
    pub deal: Box<Account<'info, Deal>>,
    #[account(mut, seeds = [SEED_CAMPAIGN, deal.key().as_ref(), &[0u8]], bump = campaign.bump)]
    pub campaign: Box<Account<'info, Campaign>>,
}

/// kind 0 = operating data (payer), kind 1 = sail controller log (manufacturer).
pub fn commit_group_data_handler(ctx: Context<CommitGroupData>, kind: u8, group: u16, root: [u8; 32]) -> Result<()> {
    let d = &ctx.accounts.deal;
    let who = match kind {
        0 => d.payer,
        1 => d.manufacturer,
        _ => return err!(E::InvalidParams),
    };
    require_keys_eq!(ctx.accounts.committer.key(), who, E::Unauthorized);
    let c = &mut ctx.accounts.campaign;
    let k = kind as usize;
    require!(group == c.next_data_group[k] && group < c.n_groups, E::WrongDataOrder);
    require!(sch::is_resolved(c.status[group as usize]), E::GroupNotResolved);
    c.data_acc[k] = sch::data_acc_next(&c.data_acc[k], group as u32, &root);
    c.next_data_group[k] += 1;
    let gpp = c.groups_per_period;
    if (group + 1) % gpp == 0 {
        let p = (group / gpp) as usize;
        c.period_snap[p][k] = c.data_acc[k];
        c.snap_ready[p] |= 1 << kind;
    }
    emit!(DataCommitted { deal: d.key(), kind, group, root });
    Ok(())
}

#[derive(Accounts)]
pub struct ReleaseBonds<'info> {
    pub caller: Signer<'info>,
    #[account(has_one = payment_mint)]
    pub deal: Box<Account<'info, Deal>>,
    #[account(mut, seeds = [SEED_CAMPAIGN, deal.key().as_ref(), &[0u8]], bump = campaign.bump)]
    pub campaign: Box<Account<'info, Campaign>>,
    pub payment_mint: Box<InterfaceAccount<'info, Mint>>,
    #[account(mut, seeds = [SEED_VAULT, deal.key().as_ref(), &[V_BOND]], bump)]
    pub bond_vault: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(mut, token::mint = payment_mint, token::authority = deal.payer)]
    pub payer_token: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(mut, token::mint = payment_mint, token::authority = deal.investor_rep)]
    pub investor_rep_token: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(mut, token::mint = payment_mint, token::authority = deal.verifiers[0])]
    pub verifier_token: Box<InterfaceAccount<'info, TokenAccount>>,
    pub token_program: Interface<'info, TokenInterface>,
}

pub fn release_bonds_handler(ctx: Context<ReleaseBonds>) -> Result<()> {
    let a = &ctx.accounts;
    let c = &a.campaign;
    require!(
        !c.bonds_released && c.current_group == c.n_groups && a.deal.next_period >= c.periods(),
        E::InvalidState
    );
    let ds = DealSeeds::of(&a.deal);
    let s = ds.seeds();
    let to = [
        a.payer_token.to_account_info(),
        a.investor_rep_token.to_account_info(),
        a.verifier_token.to_account_info(),
    ];
    for (r, dst) in to.into_iter().enumerate() {
        xfer(
            &a.token_program,
            &a.payment_mint,
            a.bond_vault.to_account_info(),
            dst,
            a.deal.to_account_info(),
            c.bond_left[r],
            Some(&[&s]),
        )?;
    }
    let deal_key = a.deal.key();
    let c = &mut ctx.accounts.campaign;
    c.bond_left = [0; 3];
    c.bonds_released = true;
    emit!(BondsReleased { deal: deal_key });
    Ok(())
}
