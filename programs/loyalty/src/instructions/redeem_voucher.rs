use anchor_lang::prelude::*;
use anchor_spl::token_interface::{
    burn_checked, close_account, spl_token_2022::{
        extension::{mint_close_authority::MintCloseAuthority, BaseStateWithExtensions, StateWithExtensions},
        state::Mint as MintState,
    },
    thaw_account, BurnChecked, CloseAccount, Mint, ThawAccount, Token2022, TokenAccount,
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

    /// Closed here. Its rent goes back to whoever paid for it, never to the
    /// merchant: the merchant didn't pay for it.
    #[account(
        mut,
        close = rent_payer,
        has_one = business,
        has_one = mint,
        has_one = rent_payer,
    )]
    pub voucher: Account<'info, Voucher>,

    /// Writable because burning lowers its supply, and because it is closed
    /// once it is empty.
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

    /// The wallet that paid for the voucher, recorded in it. Receives back
    /// the rent of the voucher account, its NFT and the holder's token
    /// account. Normally this is the relayer itself.
    #[account(mut)]
    pub rent_payer: SystemAccount<'info>,

    pub token_program: Program<'info, Token2022>,
}

pub fn redeem_voucher_handler(ctx: Context<RedeemVoucher>) -> Result<()> {
    // A voucher is only usable for 90 days after it was minted.
    require!(Clock::get()?.unix_timestamp <= ctx.accounts.voucher.expires_at, ErrorCode::VoucherExpired);
    let voucher = &ctx.accounts.voucher;
    let id_bytes = voucher.voucher_id.to_le_bytes();
    let bump = [voucher.bump];
    let seeds: &[&[u8]] = &[b"voucher", voucher.business.as_ref(), &id_bytes, &bump];
    let signer_seeds = &[seeds];

    let token_program = ctx.accounts.token_program.key();
    let holder_token = ctx.accounts.holder_token.to_account_info();
    let mint = ctx.accounts.mint.to_account_info();
    let voucher_info = voucher.to_account_info();
    let rent_payer = ctx.accounts.rent_payer.to_account_info();

    // A frozen account can't be burned from, so thaw it first.
    thaw_account(CpiContext::new_with_signer(
        token_program,
        ThawAccount {
            account: holder_token.clone(),
            mint: mint.clone(),
            authority: voucher_info.clone(),
        },
        signer_seeds,
    ))?;

    // The merchant redeems while the customer isn't signing, so the burn
    // is done through the voucher's permanent delegate power.
    burn_checked(
        CpiContext::new_with_signer(
            token_program,
            BurnChecked {
                mint: mint.clone(),
                from: holder_token.clone(),
                authority: voucher_info.clone(),
            },
            signer_seeds,
        ),
        1,
        ctx.accounts.mint.decimals,
    )?;

    // The burn is the proof the voucher was used; it stays in the chain's
    // history for good. The now-empty accounts are closed so their rent goes
    // back to whoever paid for them. The holder granted the voucher the right
    // to close their token account when they presented it.
    let holder_closer: Option<Pubkey> = ctx.accounts.holder_token.close_authority.into();
    if holder_closer == Some(voucher_info.key()) {
        close_account(CpiContext::new_with_signer(
            token_program,
            CloseAccount {
                account: holder_token,
                destination: rent_payer.clone(),
                authority: voucher_info.clone(),
            },
            signer_seeds,
        ))?;
    }

    let mint_closer = {
        let data = mint.try_borrow_data()?;
        let state = StateWithExtensions::<MintState>::unpack(&data)?;
        state
            .get_extension::<MintCloseAuthority>()
            .ok()
            .and_then(|ext| Option::<Pubkey>::from(ext.close_authority))
    };
    if mint_closer == Some(voucher_info.key()) {
        close_account(CpiContext::new_with_signer(
            token_program,
            CloseAccount {
                account: mint,
                destination: rent_payer,
                authority: voucher_info,
            },
            signer_seeds,
        ))?;
    }

    ctx.accounts.business.total_redemptions += 1;
    msg!("Voucher {} redeemed for business {:?}", ctx.accounts.voucher.voucher_id, ctx.accounts.business.key());
    Ok(())
}
