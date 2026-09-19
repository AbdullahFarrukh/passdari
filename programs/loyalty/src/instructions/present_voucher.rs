use anchor_lang::prelude::*;
use anchor_spl::token_interface::{freeze_account, FreezeAccount, Mint, Token2022, TokenAccount};
use crate::state::Voucher;
use crate::error::ErrorCode;

#[derive(Accounts)]
pub struct PresentVoucher<'info> {
    #[account(has_one = mint)]
    pub voucher: Account<'info, Voucher>,

    pub mint: Box<InterfaceAccount<'info, Mint>>,

    /// The token account holding the voucher. The signer must own it, so
    /// only the real holder can present.
    #[account(
        mut,
        token::mint = mint,
        token::authority = owner,
        token::token_program = token_program,
        constraint = holder_token.amount == 1 @ ErrorCode::NotVoucherHolder,
        constraint = !holder_token.is_frozen() @ ErrorCode::VoucherPending,
    )]
    pub holder_token: Box<InterfaceAccount<'info, TokenAccount>>,

    pub owner: Signer<'info>,

    pub token_program: Program<'info, Token2022>,
}

pub fn present_voucher_handler(ctx: Context<PresentVoucher>) -> Result<()> {
    let voucher = &ctx.accounts.voucher;
    let id_bytes = voucher.voucher_id.to_le_bytes();
    let bump = [voucher.bump];
    let seeds: &[&[u8]] = &[b"voucher", voucher.business.as_ref(), &id_bytes, &bump];

    // A frozen token account can't be transferred, so the voucher can't be
    // gifted away while the merchant is looking at it.
    freeze_account(CpiContext::new_with_signer(
        ctx.accounts.token_program.key(),
        FreezeAccount {
            account: ctx.accounts.holder_token.to_account_info(),
            mint: ctx.accounts.mint.to_account_info(),
            authority: voucher.to_account_info(),
        },
        &[seeds],
    ))
}
