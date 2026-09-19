use anchor_lang::prelude::*;
use anchor_spl::token_interface::{
    burn_checked, thaw_account, BurnChecked, Mint, ThawAccount, Token2022, TokenAccount,
};
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
        has_one = mint,
    )]
    pub voucher: Account<'info, Voucher>,

    /// Writable because burning lowers its supply.
    #[account(mut)]
    pub mint: Box<InterfaceAccount<'info, Mint>>,

    /// Whoever holds the voucher right now. The merchant doesn't need to
    /// know the holder's address, only that this account is the one holding
    /// the voucher token and that it was presented (frozen).
    #[account(
        mut,
        token::mint = mint,
        token::token_program = token_program,
        constraint = holder_token.amount == 1 @ ErrorCode::NotVoucherHolder,
        constraint = holder_token.is_frozen() @ ErrorCode::VoucherNotPresented,
    )]
    pub holder_token: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(mut)]
    pub authority: Signer<'info>,

    /// Present purely as a required signer, matching every other
    /// relayer-backed instruction's structure — not used inside the
    /// handler itself, since redeeming doesn't create any account or need
    /// a payer.
    pub relayer: Signer<'info>,

    pub token_program: Program<'info, Token2022>,
}

pub fn redeem_voucher_handler(ctx: Context<RedeemVoucher>) -> Result<()> {
    let voucher = &ctx.accounts.voucher;
    let id_bytes = voucher.voucher_id.to_le_bytes();
    let bump = [voucher.bump];
    let seeds: &[&[u8]] = &[b"voucher", voucher.business.as_ref(), &id_bytes, &bump];
    let signer_seeds = &[seeds];

    let token_program = ctx.accounts.token_program.key();
    let holder_token = ctx.accounts.holder_token.to_account_info();
    let mint = ctx.accounts.mint.to_account_info();

    // A frozen account can't be burned from, so thaw it first.
    thaw_account(CpiContext::new_with_signer(
        token_program,
        ThawAccount {
            account: holder_token.clone(),
            mint: mint.clone(),
            authority: voucher.to_account_info(),
        },
        signer_seeds,
    ))?;

    // The merchant redeems while the customer isn't signing, so the burn
    // is done through the voucher's permanent delegate power.
    burn_checked(
        CpiContext::new_with_signer(
            token_program,
            BurnChecked {
                mint,
                from: holder_token,
                authority: voucher.to_account_info(),
            },
            signer_seeds,
        ),
        1,
        ctx.accounts.mint.decimals,
    )?;

    ctx.accounts.business.total_redemptions += 1;
    msg!("Voucher {} redeemed for business {:?}", ctx.accounts.voucher.voucher_id, ctx.accounts.business.key());
    Ok(())
}
