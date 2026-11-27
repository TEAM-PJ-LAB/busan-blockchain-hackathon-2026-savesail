use anchor_lang::prelude::*;

#[event]
pub struct DealInitialized {
    pub deal: Pubkey,
    pub campaign: Pubkey,
}
#[event]
pub struct VaultsInitialized {
    pub deal: Pubkey,
}
#[event]
pub struct KycSet {
    pub deal: Pubkey,
    pub investor: Pubkey,
    pub tier: u8,
    pub annual_limit: u64,
}
#[event]
pub struct Invested {
    pub deal: Pubkey,
    pub investor: Pubkey,
    pub tranche: u8,
    pub amount: u64,
}
#[event]
pub struct FundraisingClosed {
    pub deal: Pubkey,
    pub install_cost: u64,
}
#[event]
pub struct AnchorRegistered {
    pub deal: Pubkey,
    pub role: u8,
    pub anchor: [u8; 32],
}
#[event]
pub struct Revealed {
    pub deal: Pubkey,
    pub role: u8,
    pub group: u16,
    pub value: [u8; 32],
}
#[event]
pub struct GroupResolved {
    pub deal: Pubkey,
    pub group: u16,
    pub off_index: u8,
    pub missing_mask: u8,
}
#[event]
pub struct BondSlashed {
    pub deal: Pubkey,
    pub role: u8,
    pub amount: u64,
}
#[event]
pub struct DataCommitted {
    pub deal: Pubkey,
    pub kind: u8,
    pub group: u16,
    pub root: [u8; 32],
}
#[event]
pub struct AttestationApproved {
    pub deal: Pubkey,
    pub period: u16,
    pub verifier: Pubkey,
}
#[event]
pub struct AttestationFinalized {
    pub deal: Pubkey,
    pub period: u16,
    pub due: u64,
    pub entitlement: u64,
    pub outcome: u8,
}
#[event]
pub struct Settled {
    pub deal: Pubkey,
    pub period: u16,
    pub amount: u64,
}
#[event]
pub struct Distributed {
    pub deal: Pubkey,
    pub period: u16,
    pub opex: u64,
    pub senior: u64,
    pub reserve_draw: u64,
    pub reserve_in: u64,
    pub junior: u64,
}
#[event]
pub struct Claimed {
    pub deal: Pubkey,
    pub tranche: u8,
    pub owner: Pubkey,
    pub amount: u64,
}
#[event]
pub struct BondsReleased {
    pub deal: Pubkey,
}
#[event]
pub struct DealClosed {
    pub deal: Pubkey,
    pub refund: u64,
}
#[event]
pub struct TrancheMintsRegistered {
    pub deal: Pubkey,
    pub senior: Pubkey,
    pub junior: Pubkey,
}
#[event]
pub struct TrancheTransferred {
    pub deal: Pubkey,
    pub tranche: u8,
    pub from: Pubkey,
    pub to: Pubkey,
    pub amount: u64,
}
