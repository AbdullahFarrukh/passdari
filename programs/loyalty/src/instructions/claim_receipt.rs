use anchor_lang::prelude::*;
use solana_keccak_hasher::hash;
use crate::state::{Business, LoyaltyCard, Receipt};
use crate::error::ErrorCode;
use crate::constants::{STAMP_COOLDOWN_SECONDS, MAX_CLAIMS_PER_CARD_PER_DAY};

pub fn claim_receipt_handler(ctx: Context<ClaimReceipt>, secret: [u8; 32]) -> Result<()> {
    let receipt = &ctx.accounts.receipt;
    let business = &mut ctx.accounts.business;
    let card = &mut ctx.accounts.card;
    let clock = Clock::get()?;

    let computed_hash = hash(secret.as_ref()).to_bytes();
    let (expected_receipt_pda, _bump) = Pubkey::find_program_address(
        &[b"receipt", business.key().as_ref(), computed_hash.as_ref()],
        ctx.program_id,
    );
    require_keys_eq!(expected_receipt_pda, receipt.key(), ErrorCode::InvalidSecret);

    require!(clock.unix_timestamp <= receipt.expires_at, ErrorCode::ReceiptExpired);

         if card.business == Pubkey::default() {
        card.business = business.key();
        card.customer = ctx.accounts.customer.key();
        card.bump = ctx.bumps.card;
        card.stamps_required_snapshot = business.stamps_required;
        business.total_cards += 1;
    }

    if card.last_stamp_ts != 0 {
        require!(
            clock.unix_timestamp - card.last_stamp_ts >= STAMP_COOLDOWN_SECONDS,
            ErrorCode::StampCooldownActive
        );
    }

    if clock.unix_timestamp - card.claims_window_start >= 86400 {
        card.claims_window_start = clock.unix_timestamp;
        card.claims_this_window = 0;
    }
    require!(
        card.claims_this_window < MAX_CLAIMS_PER_CARD_PER_DAY,
        ErrorCode::ClaimRateLimitExceeded
    );
    card.claims_this_window += 1;

    card.stamps += 1;
    card.last_stamp_ts = clock.unix_timestamp;
    card.lifetime_stamps += 1;

    business.total_stamps_issued += 1;

    emit!(StampClaimed {
        business: business.key(),
        customer: card.customer,
        stamps: card.stamps,
        timestamp: clock.unix_timestamp,
    });

    Ok(())
}

#[derive(Accounts)]
#[instruction(secret: [u8; 32])]
pub struct ClaimReceipt<'info> {
    #[account(mut)]
    pub business: Account<'info, Business>,

    /// Closed here. Its rent goes back to whoever paid for it, never to the
    /// business: the merchant didn't pay for it.
    #[account(mut, close = rent_payer, has_one = rent_payer)]
    pub receipt: Account<'info, Receipt>,

    #[account(
        init_if_needed,
        payer = relayer,
        space = 8 + LoyaltyCard::INIT_SPACE,
        seeds = [b"card", business.key().as_ref(), customer.key().as_ref()],
        bump,
    )]
    pub card: Account<'info, LoyaltyCard>,

    /// The customer authorizing this claim. Signs to prove it's really them,
    /// but pays nothing — the relayer covers rent and fees instead.
    pub customer: Signer<'info>,

    /// The relayer, paying rent and fees on the customer's behalf so the
    /// customer never needs to hold SOL.
    #[account(mut)]
    pub relayer: Signer<'info>,

    /// The wallet that paid the receipt's rent, recorded in the receipt.
    /// Receives it back. Normally this is the relayer itself.
    #[account(mut)]
    pub rent_payer: SystemAccount<'info>,

    pub system_program: Program<'info, System>,
}

#[event]
pub struct StampClaimed {
    pub business: Pubkey,
    pub customer: Pubkey,
    pub stamps: u8,
    pub timestamp: i64,
}