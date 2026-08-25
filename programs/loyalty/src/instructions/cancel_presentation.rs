use anchor_lang::prelude::*;
use crate::state::Voucher;

#[derive(Accounts)]
pub struct CancelPresentation<'info> {
    #[account(mut, has_one = owner)]
    pub voucher: Account<'info, Voucher>,
    pub owner: Signer<'info>,
}

pub fn cancel_presentation_handler(ctx: Context<CancelPresentation>) -> Result<()> {
    ctx.accounts.voucher.pending_redemption = false;
    Ok(())
}