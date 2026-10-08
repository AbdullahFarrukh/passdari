use anchor_lang::prelude::*;

#[error_code]
pub enum ErrorCode {
    #[msg("Custom error message")]
    CustomError,
    #[msg("The purchase amount must meet the business's minimum threshold")]
    InvalidAmountBand,
    #[msg("This receipt has expired")]
    ReceiptExpired,
    #[msg("The provided secret does not match this receipt")]
    InvalidSecret,
    #[msg("This voucher ID does not match the business's next expected ID")]
    InvalidVoucherId,
    #[msg("Not enough stamps to mint a voucher")]
    NotEnoughStamps,
    #[msg("This voucher is currently presented for redemption")]
    VoucherPending,
    #[msg("This voucher has not been presented for redemption")]
    VoucherNotPresented,
    #[msg("This receipt has not yet expired")]
    ReceiptNotYetExpired,
    #[msg("Please wait before claiming another stamp on this card")]
    StampCooldownActive,
    #[msg("This business has issued too many receipts in the last hour")]
    ReceiptRateLimitExceeded,
    #[msg("This card has claimed too many stamps today")]
    ClaimRateLimitExceeded,
    #[msg("The voucher metadata URI is too long")]
    UriTooLong,
    #[msg("This token account does not hold the voucher")]
    NotVoucherHolder,
    #[msg("This voucher has expired")]
    VoucherExpired,
    #[msg("This voucher has not expired yet")]
    VoucherNotExpired,
    #[msg("This card has had a stamp in the last 90 days")]
    CardNftNotIdle,
    #[msg("The rent must go back to the wallet that paid it")]
    WrongRentPayer,
    #[msg("The number of stamps to transfer must be at least one")]
    InvalidStampAmount,
    #[msg("That card cannot hold any more stamps")]
    TooManyStamps,
    #[msg("This card still has stamps on it")]
    CardNotEmpty,
    #[msg("This card has had a stamp in the last year")]
    CardNotDead,
    #[msg("This card still has an NFT; recycle that first")]
    CardNftStillAlive,
    #[msg("This account is not a loyalty card")]
    NotACard,
    #[msg("This card has already been migrated")]
    CardAlreadyMigrated,
}