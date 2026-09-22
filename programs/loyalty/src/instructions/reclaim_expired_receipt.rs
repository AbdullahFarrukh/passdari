use anchor_lang::prelude::*;
use crate::state::Receipt;
use crate::error::ErrorCode;

/// Closes a receipt nobody claimed in time and sends its rent back to whoever
/// paid for it. Anyone may call this — the merchant's clean-up button or the
/// app's daily clean-up job — because the rent can only go back to the wallet
/// the receipt names, and an expired receipt can't be claimed anyway.
#[derive(Accounts)]
pub struct ReclaimExpiredReceipt<'info> {
    #[account(mut, close = rent_payer, has_one = rent_payer)]
    pub receipt: Account<'info, Receipt>,

    /// The wallet that paid the receipt's rent, recorded in the receipt.
    /// Receives it back — not the merchant, who never paid for it.
    #[account(mut)]
    pub rent_payer: SystemAccount<'info>,
}

pub fn reclaim_expired_receipt_handler(ctx: Context<ReclaimExpiredReceipt>) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    require!(now >= ctx.accounts.receipt.expires_at, ErrorCode::ReceiptNotYetExpired);
    Ok(())
}
