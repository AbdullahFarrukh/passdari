use anchor_lang::prelude::*;
use crate::error::ErrorCode;

/// The limits a shop's own settings have to obey, checked the same way whether the shop is being
/// registered or edited later.
///
/// These did not exist before, and their absence was not harmless. A shop could set "stamps needed" to
/// zero, and then `mint_voucher`'s `card.stamps >= card.stamps_required_snapshot` was satisfied by an
/// empty card and took nothing away — one stamp bought reward after reward, each one a real NFT the
/// relayer paid rent for. There is a test for exactly that.
///
/// The string lengths match the space reserved on the `Business` account. Writing past them failed
/// anyway, but only at the end of the instruction with `AccountDidNotSerialize`, which tells a merchant
/// nothing about which field was too long.

pub const MAX_NAME_LEN: usize = 32;
pub const MAX_CATEGORY_LEN: usize = 20;
pub const MAX_REWARD_LABEL_LEN: usize = 32;
pub const MAX_CURRENCY_LEN: usize = 3;

/// Nobody runs a card that needs more than this, and it keeps a shop from parking a customer's stamps
/// behind a wall they can never finish.
pub const MAX_STAMPS_REQUIRED: u8 = 100;

/// A receipt has to live long enough for the customer to scan it, and not so long that unclaimed ones
/// pile up holding the relayer's rent: between a minute and a day.
pub const MIN_RECEIPT_TTL_SECONDS: u32 = 60;
pub const MAX_RECEIPT_TTL_SECONDS: u32 = 24 * 60 * 60;

/// A shop may commit to its reward for at most a year. Someone will type a date in 2099 and lock
/// themselves out for decades otherwise.
pub const MAX_TERMS_LOCK_SECONDS: i64 = 365 * 24 * 60 * 60;

/// How much notice a shop must give before its terms unlock. It may always commit for longer, and it
/// may wind an offer down early — but never to less than this, so a customer part-way through a card
/// always has time to finish.
pub const MIN_TERMS_NOTICE_SECONDS: i64 = 14 * 24 * 60 * 60;

/// Checks a proposed "committed until" date against the clock and against the shop's current promise.
///
/// `current_lock` is what the shop has already promised. A shop may always extend it. It may bring it
/// forward — winding an offer down — but not to less than two weeks away, because people are collecting
/// against it right now.
pub fn check_terms_lock(now: i64, current_lock: i64, proposed: i64) -> Result<()> {
    // Zero means "no commitment". Committing is optional — it is a promise a shop chooses to make, not
    // one the program imposes — so a shop that offers no end date simply stays editable.
    if proposed == 0 {
        return Ok(());
    }
    require!(proposed >= now, ErrorCode::TermsLockInThePast);
    require!(proposed - now <= MAX_TERMS_LOCK_SECONDS, ErrorCode::TermsLockTooLong);
    // Bringing the promise forward is allowed, but only with notice.
    if proposed < current_lock {
        require!(proposed - now >= MIN_TERMS_NOTICE_SECONDS, ErrorCode::TermsNoticeTooShort);
    }
    Ok(())
}

pub fn check_settings(
    name: &str,
    category: &str,
    reward_label: &str,
    stamps_required: u8,
    receipt_ttl_seconds: u32,
) -> Result<()> {
    require!(!name.trim().is_empty(), ErrorCode::BusinessNameRequired);
    require!(name.len() <= MAX_NAME_LEN, ErrorCode::BusinessNameTooLong);
    require!(category.len() <= MAX_CATEGORY_LEN, ErrorCode::CategoryTooLong);
    require!(!reward_label.trim().is_empty(), ErrorCode::RewardLabelRequired);
    require!(reward_label.len() <= MAX_REWARD_LABEL_LEN, ErrorCode::RewardLabelTooLong);
    require!(stamps_required >= 1, ErrorCode::StampsRequiredTooLow);
    require!(stamps_required <= MAX_STAMPS_REQUIRED, ErrorCode::StampsRequiredTooHigh);
    require!(
        (MIN_RECEIPT_TTL_SECONDS..=MAX_RECEIPT_TTL_SECONDS).contains(&receipt_ttl_seconds),
        ErrorCode::ReceiptTtlOutOfRange
    );
    Ok(())
}
