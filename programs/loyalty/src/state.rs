use anchor_lang::prelude::*;

#[account]
#[derive(InitSpace)]
pub struct Counter {
    pub owner: Pubkey,
    pub count: u64,
    pub bump: u8,
}

#[account]
#[derive(InitSpace)]
pub struct Business {
    pub authority: Pubkey,
    #[max_len(32)]
    pub name: String,
    #[max_len(20)]
    pub category: String,
    #[max_len(32)]
    pub reward_label: String,
    pub stamps_required: u8,
    pub min_purchase_amount: u64,
    #[max_len(3)]
    pub currency: String,
    pub receipt_ttl_seconds: u32,
    pub receipts_window_start: i64,
    pub receipts_this_window: u32,
    pub total_cards: u32,
    pub total_stamps_issued: u64,
    pub total_vouchers_issued: u64,
    pub total_redemptions: u32,
    pub bump: u8,
}

#[account]
#[derive(InitSpace)]
pub struct Receipt {
    pub business: Pubkey,
    pub amount_band: u8,
    pub issued_at: i64,
    pub expires_at: i64,
    pub bump: u8,
}

#[account]
#[derive(InitSpace)]
pub struct LoyaltyCard {
    pub business: Pubkey,
    pub customer: Pubkey,
    pub stamps: u8,
    pub last_stamp_ts: i64,
    pub claims_window_start: i64,
    pub claims_this_window: u32,
    pub lifetime_stamps: u32,
    pub redemptions: u32,
    pub bump: u8,
}

#[account]
#[derive(InitSpace)]
pub struct Voucher {
    pub business: Pubkey,
    pub owner: Pubkey,
    pub voucher_id: u64,
    pub minted_at: i64,
    pub pending_redemption: bool,
    pub bump: u8,
}