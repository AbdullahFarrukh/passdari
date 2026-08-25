use anchor_lang::prelude::*;
use crate::state::Voucher;

#[derive(Accounts)]
pub struct PresentVoucher<'info> {
    #[account(mut, has_one = owner)]
    pub voucher: Account<'info, Voucher>,
    pub owner: Signer<'info>,
}

pub fn present_voucher_handler(ctx: Context<PresentVoucher>) -> Result<()> {
    ctx.accounts.voucher.pending_redemption = true;
    Ok(())
}