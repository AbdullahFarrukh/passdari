use anchor_lang::prelude::*;
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token_interface::{transfer_checked, Mint, Token2022, TokenAccount, TransferChecked};
use crate::state::Voucher;
use crate::error::ErrorCode;

#[derive(Accounts)]
pub struct TransferVoucher<'info> {
    #[account(mut, has_one = mint)]
    pub voucher: Account<'info, Voucher>,

    pub mint: Box<InterfaceAccount<'info, Mint>>,

    /// A frozen account (a voucher presented to a merchant) can't be
    /// transferred. The token program refuses it as well, this just gives a
    /// clearer error.
    #[account(
        mut,
        token::mint = mint,
        token::authority = owner,
        token::token_program = token_program,
        constraint = from_token.amount == 1 @ ErrorCode::NotVoucherHolder,
        constraint = !from_token.is_frozen() @ ErrorCode::VoucherPending,
    )]
    pub from_token: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(
        init_if_needed,
        payer = relayer,
        associated_token::mint = mint,
        associated_token::authority = new_owner,
        associated_token::token_program = token_program,
    )]
    pub to_token: Box<InterfaceAccount<'info, TokenAccount>>,

    /// CHECK: only used as the address that will own the receiving token
    /// account. It never has to sign.
    pub new_owner: UncheckedAccount<'info>,

    /// The current holder, signing to send the voucher away.
    pub owner: Signer<'info>,

    /// The relayer, covering the rent for the receiving token account if it
    /// doesn't exist yet.
    #[account(mut)]
    pub relayer: Signer<'info>,

    pub token_program: Program<'info, Token2022>,

    pub associated_token_program: Program<'info, AssociatedToken>,

    pub system_program: Program<'info, System>,
}

pub fn transfer_voucher_handler(ctx: Context<TransferVoucher>) -> Result<()> {
    transfer_checked(
        CpiContext::new(
            ctx.accounts.token_program.key(),
            TransferChecked {
                from: ctx.accounts.from_token.to_account_info(),
                mint: ctx.accounts.mint.to_account_info(),
                to: ctx.accounts.to_token.to_account_info(),
                authority: ctx.accounts.owner.to_account_info(),
            },
        ),
        1,
        ctx.accounts.mint.decimals,
    )?;

    ctx.accounts.voucher.owner = ctx.accounts.new_owner.key();
    msg!("Voucher {} transferred to {:?}", ctx.accounts.voucher.voucher_id, ctx.accounts.new_owner.key());
    Ok(())
}
