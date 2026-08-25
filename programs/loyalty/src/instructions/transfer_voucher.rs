use anchor_lang::prelude::*;
use crate::state::Voucher;
use crate::error::ErrorCode;

#[derive(Accounts)]
pub struct TransferVoucher<'info> {
    #[account(
        mut,
        has_one = owner,
        constraint = !voucher.pending_redemption @ ErrorCode::VoucherPending,
    )]
    pub voucher: Account<'info, Voucher>,

    pub owner: Signer<'info>,
}

pub fn transfer_voucher_handler(ctx: Context<TransferVoucher>, new_owner: Pubkey) -> Result<()> {
    ctx.accounts.voucher.owner = new_owner;
    msg!("Voucher {} transferred to {:?}", ctx.accounts.voucher.voucher_id, new_owner);
    Ok(())
}