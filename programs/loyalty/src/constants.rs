use anchor_lang::prelude::*;

#[constant]
pub const SEED: &str = "anchor";

pub const STAMP_COOLDOWN_SECONDS: i64 = 60;
pub const MAX_RECEIPTS_PER_HOUR: u32 = 100;
pub const MAX_CLAIMS_PER_CARD_PER_DAY: u32 = 20;
pub const MAX_VOUCHER_URI_LEN: usize = 100;
pub const VOUCHER_SYMBOL: &str = "PSDR";
pub const CARD_SYMBOL: &str = "PSDC";
