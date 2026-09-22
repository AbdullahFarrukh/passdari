use anchor_lang::prelude::*;

#[constant]
pub const SEED: &str = "anchor";

pub const STAMP_COOLDOWN_SECONDS: i64 = 60;
pub const MAX_RECEIPTS_PER_HOUR: u32 = 100;
pub const MAX_CLAIMS_PER_CARD_PER_DAY: u32 = 20;
pub const MAX_VOUCHER_URI_LEN: usize = 100;
pub const VOUCHER_SYMBOL: &str = "PSDR";
pub const CARD_SYMBOL: &str = "PSDC";

/// How long a voucher can be used after it is minted: 90 days. The date is
/// fixed on the voucher when it is minted, so it can never be shortened later.
pub const VOUCHER_VALID_SECONDS: i64 = 90 * 24 * 60 * 60;

/// After this long without a stamp, a card's NFT can be recycled so its rent
/// goes back to whoever paid it: 90 days. The stamps stay on the card, and the
/// next stamp brings a new NFT.
pub const CARD_NFT_IDLE_SECONDS: i64 = 90 * 24 * 60 * 60;
