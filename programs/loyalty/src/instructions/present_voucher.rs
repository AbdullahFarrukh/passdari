use anchor_lang::prelude::*;
use anchor_spl::token_interface::{
    freeze_account, set_authority, spl_token_2022::instruction::AuthorityType, FreezeAccount, Mint,
    SetAuthority, Token2022, TokenAccount,
};
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
    // A voucher is only usable for 90 days after it was minted.
    require!(Clock::get()?.unix_timestamp <= ctx.accounts.voucher.expires_at, ErrorCode::VoucherExpired);
    let voucher = &ctx.accounts.voucher;
    let id_bytes = voucher.voucher_id.to_le_bytes();
    let bump = [voucher.bump];
    let seeds: &[&[u8]] = &[b"voucher", voucher.business.as_ref(), &id_bytes, &bump];

    // Redeeming burns the token and closes this account, sending its rent
    // back to whoever paid for it. Only an account's close authority can
    // close it and the holder isn't there at redeem, so the holder hands
    // that right to the voucher now. This has to happen before freezing: a
    // frozen account's authorities can't be changed. An account that already
    // names another close authority is left alone; it simply isn't closed.
    let close_authority: Option<Pubkey> = ctx.accounts.holder_token.close_authority.into();
    if close_authority.is_none() {
        set_authority(
            CpiContext::new(
                ctx.accounts.token_program.key(),
                SetAuthority {
                    current_authority: ctx.accounts.owner.to_account_info(),
                    account_or_mint: ctx.accounts.holder_token.to_account_info(),
                },
            ),
            AuthorityType::CloseAccount,
            Some(voucher.key()),
        )?;
    }

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
