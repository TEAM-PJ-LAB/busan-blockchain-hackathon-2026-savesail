use crate::state::{Deal, SEED_DEAL};
use anchor_lang::prelude::*;
use anchor_spl::token_interface::{self, Mint, TokenInterface, TransferChecked};

/// Owned copy of the Deal PDA signer seeds.
pub struct DealSeeds {
    admin: Pubkey,
    id: [u8; 8],
    bump: [u8; 1],
}

impl DealSeeds {
    pub fn of(d: &Deal) -> Self {
        Self { admin: d.admin, id: d.deal_id.to_le_bytes(), bump: [d.bump] }
    }
    pub fn seeds(&self) -> [&[u8]; 4] {
        [SEED_DEAL, self.admin.as_ref(), &self.id, &self.bump]
    }
}

/// `transfer_checked`; no-op for zero. `signer` = Deal PDA seeds when the vault pays.
pub fn xfer<'info>(
    token_program: &Interface<'info, TokenInterface>,
    mint: &InterfaceAccount<'info, Mint>,
    from: AccountInfo<'info>,
    to: AccountInfo<'info>,
    authority: AccountInfo<'info>,
    amount: u64,
    signer: Option<&[&[&[u8]]]>,
) -> Result<()> {
    if amount == 0 {
        return Ok(());
    }
    let accounts = TransferChecked { from, mint: mint.to_account_info(), to, authority };
    let ctx = match signer {
        Some(s) => CpiContext::new_with_signer(token_program.key(), accounts, s),
        None => CpiContext::new(token_program.key(), accounts),
    };
    token_interface::transfer_checked(ctx, amount, mint.decimals)
}
