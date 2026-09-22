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
    /// Whoever paid this receipt's rent (the relayer, when the app issues
    /// it). The rent goes back to exactly this wallet when the receipt is
    /// claimed or reclaimed, so nobody else can collect it.
    pub rent_payer: Pubkey,
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
    /// Which card NFT is the current one. Part of the NFT's mint address, so a
    /// fresh NFT gets a fresh address each time the last one is burned. This
    /// used to be `redemptions` (unused since vouchers became NFTs); the size
    /// is the same, so cards that already exist keep working.
    pub nft_cycle: u32,
    pub stamps_required_snapshot: u8,
    pub bump: u8,
}

#[account]
#[derive(InitSpace)]
pub struct Voucher {
    pub business: Pubkey,
    /// The wallet that last received this voucher through `transfer_voucher`.
    /// Only a hint so the app can list a customer's vouchers. A wallet can
    /// move the NFT without telling us, so this can go stale. Never use it
    /// for permission checks: the token account is the source of truth.
    pub owner: Pubkey,
    pub mint: Pubkey,
    pub voucher_id: u64,
    pub minted_at: i64,
    pub bump: u8,
    /// Whoever paid the rent for this voucher, its NFT and the first holder's
    /// token account (the relayer, when the app mints it). All of that rent
    /// goes back to exactly this wallet when the voucher is redeemed.
    pub rent_payer: Pubkey,
    /// When the voucher stops being usable (90 days after minting). After
    /// this it can't be presented, gifted or redeemed, and anyone can close
    /// it so its rent goes back to `rent_payer`.
    pub expires_at: i64,
}

/// Who paid for a card NFT (its mint, the customer's token account and this
/// record), so that rent goes back to exactly that wallet when the NFT is
/// burned: at cash-in, or once the card has gone 90 days without a stamp.
/// Lives at `["card_nft", mint]` and is closed together with the NFT.
#[account]
#[derive(InitSpace)]
pub struct CardNft {
    pub rent_payer: Pubkey,
    pub bump: u8,
}