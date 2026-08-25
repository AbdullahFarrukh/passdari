use anchor_lang::prelude::*;
use crate::state::{Business, Receipt};
use crate::error::ErrorCode;

#[derive(Accounts)]
pub struct ReclaimExpiredReceipt<'info> {
    #[account(mut, close = authority, has_one = business)]
    pub receipt: Account<'info, Receipt>,

    #[account(has_one = authority)]
    pub business: Account<'info, Business>,

    #[account(mut)]
    pub authority: Signer<'info>,
}

pub fn reclaim_expired_receipt_handler(ctx: Context<ReclaimExpiredReceipt>) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    require!(now >= ctx.accounts.receipt.expires_at, ErrorCode::ReceiptNotYetExpired);
    Ok(())
}