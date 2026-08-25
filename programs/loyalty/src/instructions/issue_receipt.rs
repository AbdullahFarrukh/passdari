use anchor_lang::prelude::*;
use crate::state::{Business, Receipt};
use crate::error::ErrorCode;

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
        payer = authority,
        space = 8 + Receipt::INIT_SPACE,
        seeds = [b"receipt", business.key().as_ref(), secret_hash.as_ref()],
        bump
    )]
    pub receipt: Account<'info, Receipt>,

    #[account(mut)]
    pub authority: Signer<'info>,

    pub system_program: Program<'info, System>,
}

pub fn issue_receipt_handler(
    ctx: Context<IssueReceipt>,
    _secret_hash: [u8; 32],
    amount_band: u8,
) -> Result<()> {
    require!(amount_band != 0, ErrorCode::InvalidAmountBand);

    let business = &ctx.accounts.business;
    let now = Clock::get()?.unix_timestamp;

    let receipt = &mut ctx.accounts.receipt;
    receipt.business = business.key();
    receipt.amount_band = amount_band;
    receipt.issued_at = now;
    receipt.expires_at = now + (business.receipt_ttl_seconds as i64);
    receipt.bump = ctx.bumps.receipt;

    msg!("Receipt issued for business {:?}, band {}", receipt.business, amount_band);
    Ok(())
}