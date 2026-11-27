//! P1 (PRD §2.1): Token-2022 tranche tokens gated by Token ACL (P1-1), settlement in the Transfer
//! Hook (P1-2) and proxy submission of verifier votes signed with Ed25519 (P1-3).

use crate::{
    error::SaveSailError as E, events::*, ix_deal::invest_core, ix_settle::vote, logic::schedule as sch, state::*,
    util::*,
};
use anchor_lang::{
    compat::solana_instructions_sysvar::{self as ixs, load_current_index_checked, load_instruction_at_checked},
    prelude::*,
    solana_program::{program::invoke_signed, program_option::COption, system_instruction},
};
use anchor_spl::{
    associated_token,
    compat::spl_token_metadata_interface::state::TokenMetadata,
    token_2022::{
        self,
        spl_token_2022::{
            extension::{
                default_account_state::DefaultAccountState,
                metadata_pointer::MetadataPointer,
                transfer_hook::{TransferHook, TransferHookAccount},
                BaseStateWithExtensions, ExtensionType, StateWithExtensions,
            },
            state::{Account as T22Account, AccountState, Mint as T22Mint},
        },
        MintTo, Token2022,
    },
    token_interface::{Mint, TokenAccount, TokenInterface},
};

pub const ED25519_ID: Pubkey = pubkey!("Ed25519SigVerify111111111111111111111111111");
/// sha256("spl-transfer-hook-interface:execute")[..8]
pub const EXECUTE_DISCRIMINATOR: [u8; 8] = [105, 37, 101, 197, 75, 251, 102, 26];
/// sRFC 37 metadata key naming the gating program (token-acl client `TOKEN_ACL_METADATA_KEY`).
pub const TOKEN_ACL_METADATA_KEY: &str = "token_acl";
/// Validation account size: TLV header 12 + list length 4 + 3 metas x 35.
pub const HOOK_METAS_LEN: usize = 121;
const MINT_CONFIG_LEN: usize = 100;

// ---------------------------------------------------------------- register_tranche_mints

#[derive(Accounts)]
pub struct RegisterTrancheMints<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,
    #[account(mut, has_one = admin @ E::Unauthorized, has_one = payment_mint)]
    pub deal: Box<Account<'info, Deal>>,
    pub payment_mint: Box<InterfaceAccount<'info, Mint>>,
    /// CHECK: validated by `check_tranche_mint`
    pub senior_mint: UncheckedAccount<'info>,
    /// CHECK: validated by `check_tranche_mint`
    pub junior_mint: UncheckedAccount<'info>,
    /// CHECK: Token ACL MintConfig of `senior_mint`, validated by `check_tranche_mint`
    pub senior_config: UncheckedAccount<'info>,
    /// CHECK: Token ACL MintConfig of `junior_mint`, validated by `check_tranche_mint`
    pub junior_config: UncheckedAccount<'info>,
    /// CHECK: Transfer Hook validation account, created here
    #[account(mut, seeds = [SEED_HOOK_METAS, senior_mint.key().as_ref()], bump)]
    pub senior_metas: UncheckedAccount<'info>,
    /// CHECK: Transfer Hook validation account, created here
    #[account(mut, seeds = [SEED_HOOK_METAS, junior_mint.key().as_ref()], bump)]
    pub junior_metas: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
}

/// Binds one Token-2022 mint per tranche to a deal that has not raised anything yet (P1-1 ①).
pub fn register_tranche_mints_handler(ctx: Context<RegisterTrancheMints>) -> Result<()> {
    let a = &ctx.accounts;
    let d = &a.deal;
    require!(d.status == ST_FUNDRAISING && d.raised == [0, 0] && !d.tokenized(), E::InvalidState);
    let mints = [a.senior_mint.key(), a.junior_mint.key()];
    require!(mints[0] != mints[1], E::InvalidTrancheMint);
    let deal_key = d.key();
    check_tranche_mint(&a.senior_mint, &a.senior_config, &deal_key, a.payment_mint.decimals)?;
    check_tranche_mint(&a.junior_mint, &a.junior_config, &deal_key, a.payment_mint.decimals)?;
    for (t, (metas, bump)) in
        [(&a.senior_metas, ctx.bumps.senior_metas), (&a.junior_metas, ctx.bumps.junior_metas)].into_iter().enumerate()
    {
        let seeds: &[&[u8]] = &[SEED_HOOK_METAS, mints[t].as_ref(), &[bump]];
        create_pda(&a.admin, metas, &a.system_program, HOOK_METAS_LEN, seeds)?;
        metas.try_borrow_mut_data()?.copy_from_slice(&hook_metas(&deal_key, t as u8));
    }
    ctx.accounts.deal.tranche_mints = mints;
    emit!(TrancheMintsRegistered { deal: deal_key, senior: mints[0], junior: mints[1] });
    Ok(())
}

/// Mint rules: authority = Deal PDA, supply 0, payment decimals, freeze authority = its Token ACL
/// MintConfig (permissionless thaw on), exactly the extensions DefaultAccountState(Frozen),
/// TransferHook(this program, no authority), MetadataPointer(self) and TokenMetadata with
/// `token_acl` = the MintConfig gating program.
fn check_tranche_mint(mint: &UncheckedAccount, cfg: &UncheckedAccount, deal: &Pubkey, decimals: u8) -> Result<()> {
    require!(mint.owner == &token_2022::ID && cfg.owner == &TOKEN_ACL_ID, E::InvalidTrancheMint);
    let c = cfg.try_borrow_data()?;
    require!(
        c.len() == MINT_CONFIG_LEN && c[0] == 1 && c[2] == 1 && c[4..36] == mint.key().to_bytes()[..],
        E::InvalidTrancheMint
    );
    let cfg_pda = Pubkey::create_program_address(&[SEED_TACL_MINT_CONFIG, mint.key.as_ref(), &[c[1]]], &TOKEN_ACL_ID)
        .map_err(|_| E::InvalidTrancheMint)?;
    require!(cfg_pda == cfg.key(), E::InvalidTrancheMint);
    let gate = Pubkey::new_from_array(c[68..100].try_into().unwrap());

    let data = mint.try_borrow_data()?;
    let m = StateWithExtensions::<T22Mint>::unpack(&data).map_err(|_| E::InvalidTrancheMint)?;
    let b = &m.base;
    require!(
        b.is_initialized
            && b.supply == 0
            && b.decimals == decimals
            && b.mint_authority == COption::Some(*deal)
            && b.freeze_authority == COption::Some(cfg_pda),
        E::InvalidTrancheMint
    );
    let types = m.get_extension_types().map_err(|_| E::InvalidTrancheMint)?;
    let want = [
        ExtensionType::DefaultAccountState,
        ExtensionType::TransferHook,
        ExtensionType::MetadataPointer,
        ExtensionType::TokenMetadata,
    ];
    require!(types.len() == want.len() && want.iter().all(|e| types.contains(e)), E::InvalidTrancheMint);
    let das = m.get_extension::<DefaultAccountState>().map_err(|_| E::InvalidTrancheMint)?;
    let th = m.get_extension::<TransferHook>().map_err(|_| E::InvalidTrancheMint)?;
    let mp = m.get_extension::<MetadataPointer>().map_err(|_| E::InvalidTrancheMint)?;
    require!(
        das.state == AccountState::Frozen as u8
            && Option::<Pubkey>::from(th.program_id) == Some(crate::ID)
            && Option::<Pubkey>::from(th.authority).is_none()
            && Option::<Pubkey>::from(mp.metadata_address) == Some(mint.key()),
        E::InvalidTrancheMint
    );
    let md = m.get_variable_len_extension::<TokenMetadata>().map_err(|_| E::InvalidTrancheMint)?;
    let gate_s = gate.to_string();
    require!(
        md.mint == mint.key()
            && md.additional_metadata.iter().any(|(k, v)| k == TOKEN_ACL_METADATA_KEY && *v == gate_s),
        E::InvalidTrancheMint
    );
    Ok(())
}

/// Extra accounts for `Execute`: deal (fixed), then the sender's and receiver's positions,
/// `["pos", deal, tranche, owner]` with owner = bytes 32..64 of the source (0) / destination (2) account.
pub fn hook_metas(deal: &Pubkey, tranche: u8) -> [u8; HOOK_METAS_LEN] {
    let mut d = [0u8; HOOK_METAS_LEN];
    d[..8].copy_from_slice(&EXECUTE_DISCRIMINATOR);
    d[8..12].copy_from_slice(&109u32.to_le_bytes());
    d[12..16].copy_from_slice(&3u32.to_le_bytes());
    // meta 0: fixed pubkey, read-only
    d[17..49].copy_from_slice(deal.as_ref());
    for (i, token_index) in [(1usize, 0u8), (2, 2)] {
        let o = 16 + 35 * i;
        d[o] = 1; // PDA of this program
        let seeds = [1, 3, b'p', b'o', b's', 3, 5, 1, 1, tranche, 4, token_index, 32, 32];
        d[o + 1..o + 1 + seeds.len()].copy_from_slice(&seeds);
        d[o + 34] = 1; // writable
    }
    d
}

/// Creates a program-owned PDA; tolerates lamports sent to the address beforehand.
fn create_pda<'info>(
    payer: &Signer<'info>,
    target: &UncheckedAccount<'info>,
    system: &Program<'info, System>,
    space: usize,
    seeds: &[&[u8]],
) -> Result<()> {
    let need = Rent::get()?.minimum_balance(space);
    let have = target.lamports();
    let infos = [payer.to_account_info(), target.to_account_info(), system.to_account_info()];
    if have == 0 {
        let ix = system_instruction::create_account(payer.key, target.key, need, space as u64, &crate::ID);
        return Ok(invoke_signed(&ix, &infos, &[seeds])?);
    }
    require!(target.data_is_empty() && target.owner == &system.key(), E::InvalidState);
    if have < need {
        invoke_signed(&system_instruction::transfer(payer.key, target.key, need - have), &infos, &[])?;
    }
    invoke_signed(&system_instruction::allocate(target.key, space as u64), &infos, &[seeds])?;
    invoke_signed(&system_instruction::assign(target.key, &crate::ID), &infos, &[seeds])?;
    Ok(())
}

// ---------------------------------------------------------------- open_position

#[derive(Accounts)]
#[instruction(tranche: u8, owner: Pubkey)]
pub struct OpenPosition<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,
    pub deal: Box<Account<'info, Deal>>,
    #[account(init, payer = payer, space = 8 + Position::INIT_SPACE,
        seeds = [SEED_POS, deal.key().as_ref(), &[tranche], owner.as_ref()], bump)]
    pub position: Box<Account<'info, Position>>,
    pub system_program: Program<'info, System>,
}

/// Empty position so a new holder can receive tranche tokens (the hook needs both positions).
pub fn open_position_handler(ctx: Context<OpenPosition>, tranche: u8, owner: Pubkey) -> Result<()> {
    require!(ctx.accounts.deal.tokenized(), E::TrancheTokenMode);
    require!(tranche <= 1, E::InvalidParams);
    let acc = ctx.accounts.deal.acc[tranche as usize];
    let deal = ctx.accounts.deal.key();
    let ata = ata_of(&owner, &ctx.accounts.deal.tranche_mint(tranche));
    let p = &mut ctx.accounts.position;
    p.token_account = ata;
    p.deal = deal;
    p.owner = owner;
    p.tranche = tranche;
    p.acc_snapshot = acc;
    p.bump = ctx.bumps.position;
    Ok(())
}

// ---------------------------------------------------------------- invest_tokenized

#[derive(Accounts)]
#[instruction(tranche: u8)]
pub struct InvestTokenized<'info> {
    #[account(mut)]
    pub investor: Signer<'info>,
    #[account(mut, has_one = payment_mint)]
    pub deal: Box<Account<'info, Deal>>,
    /// CHECK: address fixed by seeds; read in `invest_core` so a missing record maps to `KycRequired`.
    #[account(mut, seeds = [SEED_KYC, deal.key().as_ref(), investor.key().as_ref()], bump)]
    pub kyc: UncheckedAccount<'info>,
    #[account(init_if_needed, payer = investor, space = 8 + Position::INIT_SPACE,
        seeds = [SEED_POS, deal.key().as_ref(), &[tranche], investor.key().as_ref()], bump)]
    pub position: Box<Account<'info, Position>>,
    pub payment_mint: Box<InterfaceAccount<'info, Mint>>,
    #[account(mut, token::mint = payment_mint, token::authority = investor)]
    pub investor_token: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(mut, address = deal.vault(V_RAISE))]
    pub raise_vault: Box<InterfaceAccount<'info, TokenAccount>>,
    pub token_program: Interface<'info, TokenInterface>,
    #[account(mut, address = deal.tranche_mint(tranche) @ E::InvalidTrancheMint)]
    pub tranche_mint: Box<InterfaceAccount<'info, Mint>>,
    #[account(mut, token::mint = tranche_mint, token::authority = investor, token::token_program = tranche_token_program)]
    pub investor_tranche_token: Box<InterfaceAccount<'info, TokenAccount>>,
    pub tranche_token_program: Program<'info, Token2022>,
    pub system_program: Program<'info, System>,
}

/// `invest` plus minting the same number of tranche tokens (P1-1 ④: units = token balance).
/// Minting into a frozen account fails, so only wallets Token ACL let thaw can invest (P1-1 ③).
pub fn invest_tokenized_handler(mut ctx: Context<InvestTokenized>, tranche: u8, amount: u64) -> Result<()> {
    require!(ctx.accounts.deal.tokenized(), E::TrancheTokenMode);
    let a = &mut ctx.accounts;
    if a.position.token_account == Pubkey::default() {
        a.position.token_account = ata_of(&a.investor.key(), &a.tranche_mint.key());
    }
    require!(a.investor_tranche_token.key() == a.position.token_account, E::NotAssociatedTokenAccount);
    a.position.units = a.position.units.min(a.investor_tranche_token.amount);
    invest_core(
        &mut a.deal,
        &a.kyc,
        &mut a.position,
        ctx.bumps.position,
        &a.investor,
        &a.payment_mint,
        &a.investor_token,
        &a.raise_vault,
        &a.token_program,
        tranche,
        amount,
    )?;
    let ds = DealSeeds::of(&a.deal);
    let s = ds.seeds();
    let accounts = MintTo {
        mint: a.tranche_mint.to_account_info(),
        to: a.investor_tranche_token.to_account_info(),
        authority: a.deal.to_account_info(),
    };
    token_2022::mint_to(CpiContext::new_with_signer(a.tranche_token_program.key(), accounts, &[&s]), amount)
}

// ---------------------------------------------------------------- transfer hook

#[derive(Accounts)]
pub struct TransferHookExecute<'info> {
    /// CHECK: Token-2022 source account; mint and `transferring` checked in the handler
    pub source_token: UncheckedAccount<'info>,
    /// CHECK: matched against `deal.tranche_mints`
    pub mint: UncheckedAccount<'info>,
    /// CHECK: Token-2022 destination account; mint checked in the handler
    pub destination_token: UncheckedAccount<'info>,
    /// CHECK: transfer authority (owner or delegate), unused
    pub authority: UncheckedAccount<'info>,
    /// CHECK: validation account; Token-2022 resolved the accounts below from it
    pub extra_metas: UncheckedAccount<'info>,
    pub deal: Box<Account<'info, Deal>>,
    #[account(mut, dup)]
    pub source_position: Box<Account<'info, Position>>,
    #[account(mut, dup)]
    pub destination_position: Box<Account<'info, Position>>,
}

/// Token-2022 `Execute`: settles both positions at the current accumulator before the units move
/// with the tokens (P1-2 ①). Only runs inside a transfer (source `transferring` flag). Units are
/// set to the post-transfer balances; a holder's own burn (no hook) stops accruing from here.
pub fn transfer_hook_handler(ctx: Context<TransferHookExecute>, amount: u64) -> Result<()> {
    let a = &ctx.accounts;
    let mint = a.mint.key();
    let t = a.deal.tranche_mints.iter().position(|m| *m == mint).ok_or(E::InvalidTrancheMint)?;
    let (from, src_bal) = holder(&a.source_token, &mint, true)?;
    let (to, dst_bal) = holder(&a.destination_token, &mint, false)?;
    let deal = a.deal.key();
    for (p, o, k) in
        [(&a.source_position, from, a.source_token.key()), (&a.destination_position, to, a.destination_token.key())]
    {
        require!(p.deal == deal && p.tranche as usize == t && p.owner == o, E::Unauthorized);
        require!(p.token_account == k, E::NotAssociatedTokenAccount);
    }
    let acc = a.deal.acc[t];
    // balances before this transfer (one associated account per holder; same holder = same account)
    let (pre_s, pre_d) =
        if from == to { (src_bal, dst_bal) } else { (src_bal.saturating_add(amount), dst_bal.saturating_sub(amount)) };
    let (sp, dp) = (&mut ctx.accounts.source_position, &mut ctx.accounts.destination_position);
    for (p, pre, post) in [(sp, pre_s, src_bal), (dp, pre_d, dst_bal)] {
        p.units = p.units.min(pre);
        p.settle(acc).map_err(E::from)?;
        p.units = post;
    }
    emit!(TrancheTransferred { deal, tranche: t as u8, from, to, amount });
    Ok(())
}

/// Owner and balance of a holder's Token-2022 account of `mint`; for the source the transfer flag
/// must be set. The caller matches the account against the position's associated account.
fn holder(acc: &UncheckedAccount, mint: &Pubkey, source: bool) -> Result<(Pubkey, u64)> {
    require!(acc.owner == &token_2022::ID, E::NotTransferring);
    let data = acc.try_borrow_data()?;
    let s = StateWithExtensions::<T22Account>::unpack(&data).map_err(|_| E::NotTransferring)?;
    require!(s.base.mint == *mint, E::InvalidTrancheMint);
    if source {
        let th = s.get_extension::<TransferHookAccount>().map_err(|_| E::NotTransferring)?;
        require!(bool::from(th.transferring), E::NotTransferring);
    }
    Ok((s.base.owner, s.base.amount))
}

pub fn ata_of(owner: &Pubkey, mint: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[owner.as_ref(), token_2022::ID.as_ref(), mint.as_ref()], &associated_token::ID).0
}

/// Claim on a tokenized deal: the owner's associated account (first remaining account) caps the
/// units, so tokens burned without the hook stop earning (P1-1 ④).
pub(crate) fn tokenized_units(pos: &Position, rem: &[AccountInfo]) -> Result<u64> {
    let acc = rem.first().ok_or(E::NotAssociatedTokenAccount)?;
    require!(
        acc.owner == &token_2022::ID && acc.key() == pos.token_account && pos.token_account != Pubkey::default(),
        E::NotAssociatedTokenAccount
    );
    let data = acc.try_borrow_data()?;
    let s = StateWithExtensions::<T22Account>::unpack(&data).map_err(|_| E::NotAssociatedTokenAccount)?;
    Ok(pos.units.min(s.base.amount))
}

// ---------------------------------------------------------------- approve_attestation_signed

#[derive(Accounts)]
#[instruction(payload: AttestationPayload)]
pub struct ApproveAttestationSigned<'info> {
    #[account(mut)]
    pub submitter: Signer<'info>,
    #[account(mut)]
    pub deal: Box<Account<'info, Deal>>,
    #[account(seeds = [SEED_CAMPAIGN, deal.key().as_ref(), &[0u8]], bump = campaign.bump)]
    pub campaign: Box<Account<'info, Campaign>>,
    #[account(init_if_needed, payer = submitter, space = 8 + PeriodState::INIT_SPACE,
        seeds = [SEED_PERIOD, deal.key().as_ref(), &payload.period.to_le_bytes()], bump)]
    pub period_state: Box<Account<'info, PeriodState>>,
    /// CHECK: instructions sysvar
    #[account(address = ixs::ID)]
    pub instructions: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
}

/// Anyone submits a vote that a verifier signed off-chain: the instruction right before this one
/// must be an Ed25519 precompile check of one signature over `sha256(borsh(payload))` (P1-3).
pub fn approve_attestation_signed_handler(
    mut ctx: Context<ApproveAttestationSigned>,
    payload: AttestationPayload,
) -> Result<()> {
    let msg = sch::h(&[&borsh::to_vec(&payload).map_err(|_| E::InvalidParams)?]);
    let vkey = ed25519_signer(&ctx.accounts.instructions.to_account_info(), &msg)?;
    let a = &mut ctx.accounts;
    vote(vkey, &mut a.deal, &a.campaign, &mut a.period_state, ctx.bumps.period_state, payload)
}

/// Public key of the single signature verified by the preceding Ed25519 instruction, if its
/// message is `msg` and all offsets point into that instruction.
fn ed25519_signer(sysvar: &AccountInfo, msg: &[u8; 32]) -> Result<Pubkey> {
    let cur = load_current_index_checked(sysvar)? as usize;
    require!(cur > 0, E::InvalidSignature);
    let ix = load_instruction_at_checked(cur - 1, sysvar)?;
    let d = &ix.data;
    require!(ix.program_id == ED25519_ID && d.len() >= 16 && d[0] == 1, E::InvalidSignature);
    let u = |i: usize| u16::from_le_bytes([d[i], d[i + 1]]) as usize;
    let here = u16::MAX as usize;
    require!(u(4) == here && u(8) == here && u(14) == here, E::InvalidSignature);
    let (pk, m, len) = (u(6), u(10), u(12));
    require!(len == 32 && d.len() >= pk + 32 && d.len() >= m + 32, E::InvalidSignature);
    require!(d[m..m + 32] == msg[..], E::InvalidSignature);
    Ok(Pubkey::new_from_array(d[pk..pk + 32].try_into().unwrap()))
}
