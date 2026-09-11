use anchor_lang::prelude::*;
use crate::state::{Business, Receipt};
use crate::error::ErrorCode;

#[derive(Accounts)]
pub struct ReclaimExpiredReceipt<'info> {
    #[account(mut, close = relayer, has_one = business)]
    pub receipt: Account<'info, Receipt>,

    #[account(has_one = authority)]
    pub business: Account<'info, Business>,

    pub authority: Signer<'info>,

    /// The relayer, receiving back the rent it originally paid to create
    /// this receipt — not the merchant, who never paid for it.
    #[account(mut)]
    pub relayer: Signer<'info>,
}

pub fn reclaim_expired_receipt_handler(ctx: Context<ReclaimExpiredReceipt>) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    require!(now >= ctx.accounts.receipt.expires_at, ErrorCode::ReceiptNotYetExpired);
    Ok(())
}