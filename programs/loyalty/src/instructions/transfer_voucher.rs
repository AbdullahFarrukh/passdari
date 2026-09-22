use anchor_lang::prelude::*;
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token_interface::{
    close_account, transfer_checked, CloseAccount, Mint, Token2022, TokenAccount, TransferChecked,
};
use crate::state::Voucher;
use crate::error::ErrorCode;

#[derive(Accounts)]
pub struct TransferVoucher<'info> {
    #[account(mut, has_one = mint, has_one = rent_payer)]
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

    /// The wallet that paid for the voucher, recorded in it. Receives back
    /// the rent of the sender's token account, which is empty once the
    /// voucher has moved. Normally this is the relayer itself.
    #[account(mut)]
    pub rent_payer: SystemAccount<'info>,

    pub token_program: Program<'info, Token2022>,

    pub associated_token_program: Program<'info, AssociatedToken>,

    pub system_program: Program<'info, System>,
}

pub fn transfer_voucher_handler(ctx: Context<TransferVoucher>) -> Result<()> {
    // A voucher is only usable for 90 days after it was minted.
    require!(Clock::get()?.unix_timestamp <= ctx.accounts.voucher.expires_at, ErrorCode::VoucherExpired);
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

    // The sender's token account is empty now, so close it and send its rent
    // back to whoever paid for it. Normally the sender can close it; once the
    // voucher has been presented (and cancelled), that right belongs to the
    // voucher. An account that names some other close authority is left.
    let voucher = &ctx.accounts.voucher;
    let voucher_key = voucher.key();
    let id_bytes = voucher.voucher_id.to_le_bytes();
    let bump = [voucher.bump];
    let seeds: &[&[u8]] = &[b"voucher", voucher.business.as_ref(), &id_bytes, &bump];
    let token_program = ctx.accounts.token_program.key();
    let from_closer: Option<Pubkey> = ctx.accounts.from_token.close_authority.into();
    if from_closer.is_none() {
        close_account(CpiContext::new(
            token_program,
            CloseAccount {
                account: ctx.accounts.from_token.to_account_info(),
                destination: ctx.accounts.rent_payer.to_account_info(),
                authority: ctx.accounts.owner.to_account_info(),
            },
        ))?;
    } else if from_closer == Some(voucher_key) {
        close_account(CpiContext::new_with_signer(
            token_program,
            CloseAccount {
                account: ctx.accounts.from_token.to_account_info(),
                destination: ctx.accounts.rent_payer.to_account_info(),
                authority: ctx.accounts.voucher.to_account_info(),
            },
            &[seeds],
        ))?;
    }

    ctx.accounts.voucher.owner = ctx.accounts.new_owner.key();
    msg!("Voucher {} transferred to {:?}", ctx.accounts.voucher.voucher_id, ctx.accounts.new_owner.key());
    Ok(())
}
