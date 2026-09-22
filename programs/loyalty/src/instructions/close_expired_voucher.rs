use anchor_lang::prelude::*;
use anchor_spl::token_interface::{
    burn_checked, close_account,
    spl_token_2022::{
        extension::{mint_close_authority::MintCloseAuthority, BaseStateWithExtensions, StateWithExtensions},
        state::{Account as TokenState, AccountState, Mint as MintState},
    },
    thaw_account, BurnChecked, CloseAccount, Mint, ThawAccount, Token2022,
};
use crate::state::Voucher;
use crate::error::ErrorCode;

/// Closes a voucher that was never used within its 90 days: burns the NFT,
/// closes the mint, the holder's token account and the voucher record, and
/// sends all their rent back to whoever paid for them. Anyone may call this —
/// the merchant's clean-up button or the app's daily clean-up job — because
/// the rent can only go back to the wallet the voucher names.
#[derive(Accounts)]
pub struct CloseExpiredVoucher<'info> {
    #[account(mut, close = rent_payer, has_one = mint, has_one = rent_payer)]
    pub voucher: Account<'info, Voucher>,

    #[account(mut)]
    pub mint: Box<InterfaceAccount<'info, Mint>>,

    /// CHECK: The token account holding the voucher, if there still is one
    /// (the holder may have burned it themselves). Checked in the handler: it
    /// must belong to this voucher's mint, and once this runs no token may be
    /// left anywhere.
    #[account(mut)]
    pub holder_token: UncheckedAccount<'info>,

    /// The wallet that paid for the voucher, recorded in it. Receives the rent.
    #[account(mut)]
    pub rent_payer: SystemAccount<'info>,

    pub token_program: Program<'info, Token2022>,
}

pub fn close_expired_voucher_handler(ctx: Context<CloseExpiredVoucher>) -> Result<()> {
    let voucher = &ctx.accounts.voucher;
    require!(Clock::get()?.unix_timestamp > voucher.expires_at, ErrorCode::VoucherNotExpired);

    let id_bytes = voucher.voucher_id.to_le_bytes();
    let bump = [voucher.bump];
    let seeds: &[&[u8]] = &[b"voucher", voucher.business.as_ref(), &id_bytes, &bump];
    let signer_seeds = &[seeds];

    let token_program = ctx.accounts.token_program.key();
    let voucher_info = voucher.to_account_info();
    let mint = ctx.accounts.mint.to_account_info();
    let holder_token = ctx.accounts.holder_token.to_account_info();
    let rent_payer = ctx.accounts.rent_payer.to_account_info();

    if *holder_token.owner == token_program && !holder_token.data_is_empty() {
        let (token_mint, amount, frozen, closer) = {
            let data = holder_token.try_borrow_data()?;
            let state = StateWithExtensions::<TokenState>::unpack(&data)?.base;
            (state.mint, state.amount, state.state == AccountState::Frozen, Option::<Pubkey>::from(state.close_authority))
        };
        require_keys_eq!(token_mint, mint.key(), ErrorCode::NotVoucherHolder);

        if amount > 0 {
            // A voucher left presented is frozen, and a frozen account can't
            // be burned from, so thaw it first.
            if frozen {
                thaw_account(CpiContext::new_with_signer(
                    token_program,
                    ThawAccount { account: holder_token.clone(), mint: mint.clone(), authority: voucher_info.clone() },
                    signer_seeds,
                ))?;
            }
            // Burned through the voucher's permanent delegate power: the
            // holder isn't here.
            burn_checked(
                CpiContext::new_with_signer(
                    token_program,
                    BurnChecked { mint: mint.clone(), from: holder_token.clone(), authority: voucher_info.clone() },
                    signer_seeds,
                ),
                amount,
                ctx.accounts.mint.decimals,
            )?;
        }

        // The first holder granted this right when the voucher was minted, and
        // any later holder when they presented it. A gifted voucher that was
        // never presented leaves its (now empty) account with its holder.
        if closer == Some(voucher_info.key()) {
            close_account(CpiContext::new_with_signer(
                token_program,
                CloseAccount { account: holder_token, destination: rent_payer.clone(), authority: voucher_info.clone() },
                signer_seeds,
            ))?;
        }
    }

    let (supply, mint_closer) = {
        let data = mint.try_borrow_data()?;
        let state = StateWithExtensions::<MintState>::unpack(&data)?;
        let closer = state
            .get_extension::<MintCloseAuthority>()
            .ok()
            .and_then(|ext| Option::<Pubkey>::from(ext.close_authority));
        (state.base.supply, closer)
    };
    // Nothing may be closed while the token still exists somewhere: the
    // account passed in has to be the one that held it.
    require!(supply == 0, ErrorCode::NotVoucherHolder);

    if mint_closer == Some(voucher_info.key()) {
        close_account(CpiContext::new_with_signer(
            token_program,
            CloseAccount { account: mint, destination: rent_payer, authority: voucher_info },
            signer_seeds,
        ))?;
    }

    msg!("Expired voucher {} closed", ctx.accounts.voucher.voucher_id);
    Ok(())
}
