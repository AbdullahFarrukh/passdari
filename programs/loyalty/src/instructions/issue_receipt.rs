use anchor_lang::prelude::*;
use crate::state::{Business, Receipt};
use crate::error::ErrorCode;
use crate::MAX_RECEIPTS_PER_HOUR;

#[derive(Accounts)]
#[instruction(secret_hash: [u8; 32])]
pub struct IssueReceipt<'info> {
    #[account(
        mut,
        seeds = [b"business", authority.key().as_ref()],
        bump = business.bump,
        has_one = authority
    )]
    pub business: Account<'info, Business>,

    #[account(
        init,
        payer = relayer,
        space = 8 + Receipt::INIT_SPACE,
        seeds = [b"receipt", business.key().as_ref(), secret_hash.as_ref()],
        bump
    )]
    pub receipt: Account<'info, Receipt>,

    /// The merchant issuing this receipt. Signs to prove it's really them
    /// and to satisfy the has_one check above, but pays nothing.
    pub authority: Signer<'info>,

    /// The relayer, covering the receipt account's rent on the merchant's
    /// behalf.
    #[account(mut)]
    pub relayer: Signer<'info>,

    pub system_program: Program<'info, System>,
}

pub fn issue_receipt_handler(
    ctx: Context<IssueReceipt>,
    _secret_hash: [u8; 32],
    amount_band: u8,
) -> Result<()> {
    require!(amount_band != 0, ErrorCode::InvalidAmountBand);

    let now = Clock::get()?.unix_timestamp;

    let business = &mut ctx.accounts.business;
    if now - business.receipts_window_start >= 3600 {
        business.receipts_window_start = now;
        business.receipts_this_window = 0;
    }
    require!(business.receipts_this_window < MAX_RECEIPTS_PER_HOUR, ErrorCode::ReceiptRateLimitExceeded);
    business.receipts_this_window += 1;

    let receipt = &mut ctx.accounts.receipt;
    receipt.business = business.key();
    receipt.amount_band = amount_band;
    receipt.issued_at = now;
    receipt.expires_at = now + (business.receipt_ttl_seconds as i64);
    receipt.bump = ctx.bumps.receipt;
    receipt.rent_payer = ctx.accounts.relayer.key();

    msg!("Receipt issued for business {:?}, band {}", receipt.business, amount_band);
    Ok(())
}