use anchor_lang::prelude::*;
use anchor_spl::token_interface::{thaw_account, Mint, ThawAccount, Token2022, TokenAccount};
use crate::state::Voucher;
use crate::error::ErrorCode;

#[derive(Accounts)]
pub struct CancelPresentation<'info> {
    #[account(has_one = mint)]
    pub voucher: Account<'info, Voucher>,

    pub mint: Box<InterfaceAccount<'info, Mint>>,

    #[account(
        mut,
        token::mint = mint,
        token::authority = owner,
        token::token_program = token_program,
        constraint = holder_token.amount == 1 @ ErrorCode::NotVoucherHolder,
        constraint = holder_token.is_frozen() @ ErrorCode::VoucherNotPresented,
    )]
    pub holder_token: Box<InterfaceAccount<'info, TokenAccount>>,

    pub owner: Signer<'info>,

    pub token_program: Program<'info, Token2022>,
}

pub fn cancel_presentation_handler(ctx: Context<CancelPresentation>) -> Result<()> {
    let voucher = &ctx.accounts.voucher;
    let id_bytes = voucher.voucher_id.to_le_bytes();
    let bump = [voucher.bump];
    let seeds: &[&[u8]] = &[b"voucher", voucher.business.as_ref(), &id_bytes, &bump];

    thaw_account(CpiContext::new_with_signer(
        ctx.accounts.token_program.key(),
        ThawAccount {
            account: ctx.accounts.holder_token.to_account_info(),
            mint: ctx.accounts.mint.to_account_info(),
            authority: voucher.to_account_info(),
        },
        &[seeds],
    ))
}
