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
    #[msg("This voucher is currently presented for redemption and cannot be transferred")]
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
}