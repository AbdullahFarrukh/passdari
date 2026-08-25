use anchor_lang::prelude::*;
use crate::state::{Business, Voucher};
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

    #[account(mut)]
    pub authority: Signer<'info>,
}

pub fn redeem_voucher_handler(ctx: Context<RedeemVoucher>) -> Result<()> {
    ctx.accounts.business.total_redemptions += 1;
    msg!("Voucher {} redeemed for business {:?}", ctx.accounts.voucher.voucher_id, ctx.accounts.business.key());
    Ok(())
}