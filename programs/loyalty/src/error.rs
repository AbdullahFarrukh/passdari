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
    #[msg("The business needs a name")]
    BusinessNameRequired,
    #[msg("That business name is too long")]
    BusinessNameTooLong,
    #[msg("That category is too long")]
    CategoryTooLong,
    #[msg("The reward needs a name")]
    RewardLabelRequired,
    #[msg("That reward name is too long")]
    RewardLabelTooLong,
    #[msg("That currency code is too long")]
    CurrencyTooLong,
    #[msg("A card must need at least one stamp")]
    StampsRequiredTooLow,
    #[msg("That is too many stamps to ask for")]
    StampsRequiredTooHigh,
    #[msg("A receipt must last between a minute and a day")]
    ReceiptTtlOutOfRange,
    #[msg("That date has already passed")]
    TermsLockInThePast,
    #[msg("A reward can be committed to for at most a year")]
    TermsLockTooLong,
    #[msg("Ending an offer early needs at least two weeks' notice")]
    TermsNoticeTooShort,
    #[msg("This reward is committed until its end date and cannot be changed yet")]
    RewardTermsLocked,
    #[msg("This shop still has open stamp cards")]
    ShopStillHasOpenCards,
    #[msg("This shop's reward is still running")]
    ShopOfferStillRunning,
    #[msg("This shop still owes customers unredeemed rewards")]
    ShopStillOwesVouchers,
    #[msg("This account is not a business")]
    NotABusiness,
    #[msg("This business has already been migrated")]
    BusinessAlreadyMigrated,
}