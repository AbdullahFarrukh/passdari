use anchor_lang::prelude::*;
use crate::state::{Business, LoyaltyCard, Voucher};
use crate::error::ErrorCode;

#[derive(Accounts)]
pub struct RedeemVoucher<'info> {
    #[account(
        mut,
        seeds = [b"business", authority.key().as_ref()],
        bump = business.bump,
        has_one = authority
    )]
    pub business: Account<'info, Business>,

    #[account(
        mut,
        close = authority,
        has_one = business,
        constraint = voucher.pending_redemption @ ErrorCode::VoucherNotPresented,
    )]
    pub voucher: Account<'info, Voucher>,

    #[account(
        mut,
        seeds = [b"card", business.key().as_ref(), voucher.owner.as_ref()],
        bump = card.bump,
    )]
    pub card: Account<'info, LoyaltyCard>,

        #[account(mut)]
    pub authority: Signer<'info>,

    /// Present purely as a required signer, matching every other
    /// relayer-backed instruction's structure — not used inside the
    /// handler itself, since redeeming doesn't create any account or need
    /// a payer.
    pub relayer: Signer<'info>,
}

pub fn redeem_voucher_handler(ctx: Context<RedeemVoucher>) -> Result<()> {
    ctx.accounts.business.total_redemptions += 1;
    ctx.accounts.card.redemptions += 1;
    msg!("Voucher {} redeemed for business {:?}", ctx.accounts.voucher.voucher_id, ctx.accounts.business.key());
    Ok(())
}