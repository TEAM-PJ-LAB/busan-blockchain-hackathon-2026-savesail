use anchor_lang::prelude::*;

#[error_code]
pub enum SaveSailError {
    #[msg("Signer is not allowed for this action")]
    Unauthorized,
    #[msg("Deal or campaign is in the wrong state")]
    InvalidState,
    #[msg("Invalid parameters")]
    InvalidParams,
    #[msg("Anchor already registered for this role")]
    AlreadyRegistered,
    #[msg("Group is not the current group")]
    WrongGroup,
    #[msg("Role already revealed for this group")]
    AlreadyRevealed,
    #[msg("Revealed value does not match the hash chain")]
    HashChainMismatch,
    #[msg("Hash chain skip exceeds the limit")]
    SkipTooLarge,
    #[msg("Reveal timeout not reached")]
    TimeoutNotReached,
    #[msg("Group already resolved")]
    AlreadyResolved,
    #[msg("Group not resolved")]
    GroupNotResolved,
    #[msg("Data must be committed in group order")]
    WrongDataOrder,
    #[msg("Period data is not complete")]
    PeriodNotReady,
    #[msg("Payload data roots do not match on-chain snapshots")]
    SnapshotMismatch,
    #[msg("Verifier already approved")]
    AlreadyApproved,
    #[msg("Rate bounds are invalid")]
    InvalidRateBounds,
    #[msg("Missing-group fuel is inconsistent with the schedule")]
    MissingKgInconsistent,
    #[msg("Previous period is still open")]
    PreviousPeriodOpen,
    #[msg("Period not finalized")]
    NotFinalized,
    #[msg("Period already settled")]
    AlreadySettled,
    #[msg("Period not settled")]
    NotSettled,
    #[msg("Period already distributed")]
    AlreadyDistributed,
    #[msg("KYC record required")]
    KycRequired,
    #[msg("Investor tier not allowed for this tranche")]
    TierNotAllowed,
    #[msg("Annual investment limit exceeded")]
    LimitExceeded,
    #[msg("Tranche cap exceeded")]
    CapExceeded,
    #[msg("Tranches not fully subscribed")]
    NotFullySubscribed,
    #[msg("Nothing to claim")]
    NothingToClaim,
    #[msg("Math overflow")]
    MathOverflow,
    #[msg("Deal has not reached its last period")]
    DealNotEnded,
}

impl From<crate::logic::settle::MathError> for SaveSailError {
    fn from(_: crate::logic::settle::MathError) -> Self {
        SaveSailError::MathOverflow
    }
}
