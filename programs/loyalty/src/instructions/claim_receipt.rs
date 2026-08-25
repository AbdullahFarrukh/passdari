use anchor_lang::prelude::*;
use solana_keccak_hasher as keccak;
use crate::state::{Business, Receipt, LoyaltyCard};
use crate::error::ErrorCode;

#[derive(Accounts)]
pub struct ClaimReceipt<'info> {
    #[account(mut)]
    pub business: Account<'info, Business>,

    #[account(mut, close = business, has_one = business)]
    pub receipt: Account<'info, Receipt>,

    #[account(
        init_if_needed,
        payer = customer,
        space = 8 + LoyaltyCard::INIT_SPACE,
        seeds = [b"card", business.key().as_ref(), customer.key().as_ref()],
        bump
    )]
    pub card: Account<'info, LoyaltyCard>,

    #[account(mut)]
    pub customer: Signer<'info>,

    pub system_program: Program<'info, System>,
}

#[event]
pub struct StampClaimed {
    pub business: Pubkey,
    pub customer: Pubkey,
    pub stamps: u8,
    pub timestamp: i64,
}

pub fn claim_receipt_handler(ctx: Context<ClaimReceipt>, secret: [u8; 32]) -> Result<()> {
    // Recompute the hash from the raw secret, and check it derives the exact
    // address of the receipt account the caller supplied. This is the proof
    // of possession — the same guarantee the seeds constraint would have
    // given us, just checked explicitly instead of declaratively.
    let computed_hash = keccak::hash(secret.as_ref());
    let (expected_receipt, _bump) = Pubkey::find_program_address(
        &[
            b"receipt",
            ctx.accounts.business.key().as_ref(),
            computed_hash.to_bytes().as_ref(),
        ],
        ctx.program_id,
    );
    require_keys_eq!(ctx.accounts.receipt.key(), expected_receipt, ErrorCode::InvalidSecret);

    let now = Clock::get()?.unix_timestamp;
    require!(now < ctx.accounts.receipt.expires_at, ErrorCode::ReceiptExpired);

    let business_key = ctx.accounts.business.key();
    let customer_key = ctx.accounts.customer.key();

    let card = &mut ctx.accounts.card;
    let is_new_card = card.business == Pubkey::default();
    if is_new_card {
        card.business = business_key;
        card.customer = customer_key;
        card.stamps = 0;
        card.lifetime_stamps = 0;
        card.redemptions = 0;
        card.bump = ctx.bumps.card;
    }
    card.stamps += 1;
    card.lifetime_stamps += 1;
    card.last_stamp_ts = now;
    let new_stamp_count = card.stamps;

    let business = &mut ctx.accounts.business;
    business.total_stamps_issued += 1;
    if is_new_card {
        business.total_cards += 1;
    }

    msg!(
        "Stamp claimed: business {:?}, customer {:?}, stamps now {}",
        business_key, customer_key, new_stamp_count
    );

    emit!(StampClaimed {
        business: business_key,
        customer: customer_key,
        stamps: new_stamp_count,
        timestamp: now,
    });

    Ok(())
}