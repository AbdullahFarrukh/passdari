use anchor_lang::prelude::*;
use anchor_lang::system_program::{transfer, Transfer};
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token_interface::{
    mint_to_checked, set_authority, spl_pod::optional_keys::OptionalNonZeroPubkey,
    spl_token_2022::instruction::AuthorityType,
    spl_token_metadata_interface::state::TokenMetadata, token_metadata_initialize, Mint,
    MintToChecked, SetAuthority, Token2022, TokenAccount, TokenMetadataInitialize,
};
use crate::state::{Business, LoyaltyCard, Voucher};
use crate::error::ErrorCode;
use crate::constants::{MAX_VOUCHER_URI_LEN, VOUCHER_SYMBOL};

pub fn mint_voucher_handler(ctx: Context<MintVoucher>, voucher_id: u64, uri: String) -> Result<()> {
    require!(uri.len() <= MAX_VOUCHER_URI_LEN, ErrorCode::UriTooLong);

    let business = &mut ctx.accounts.business;
    let card = &mut ctx.accounts.card;
    let voucher = &mut ctx.accounts.voucher;
    let clock = Clock::get()?;

    require_eq!(voucher_id, business.total_vouchers_issued, ErrorCode::InvalidVoucherId);
    require!(card.stamps >= card.stamps_required_snapshot, ErrorCode::NotEnoughStamps);

    card.stamps -= card.stamps_required_snapshot;

    voucher.business = business.key();
    voucher.owner = ctx.accounts.customer.key();
    voucher.mint = ctx.accounts.mint.key();
    voucher.voucher_id = voucher_id;
    voucher.minted_at = clock.unix_timestamp;
    voucher.bump = ctx.bumps.voucher;

    business.total_vouchers_issued += 1;

    // The name is built here, not taken from the client, so a customer can't
    // pass off a voucher as another business's reward.
    let name = format!("{} - {}", business.name, business.reward_label);
    let metadata = TokenMetadata {
        update_authority: OptionalNonZeroPubkey::try_from(Some(voucher.key()))?,
        mint: ctx.accounts.mint.key(),
        name: name.clone(),
        symbol: VOUCHER_SYMBOL.to_string(),
        uri: uri.clone(),
        additional_metadata: vec![],
    };

    let business_key = business.key();
    let id_bytes = voucher_id.to_le_bytes();
    let bump = [voucher.bump];
    let seeds: &[&[u8]] = &[b"voucher", business_key.as_ref(), &id_bytes, &bump];
    let signer_seeds = &[seeds];

    let token_program = ctx.accounts.token_program.key();
    let mint_info = ctx.accounts.mint.to_account_info();
    let voucher_info = ctx.accounts.voucher.to_account_info();

    // `init` sized the mint for its two extensions only. Adding the metadata
    // makes the token program grow the account, so the extra rent has to be
    // there first.
    let rent_needed =
        Rent::get()?.minimum_balance(mint_info.data_len() + metadata.tlv_size_of()?);
    let rent_missing = rent_needed.saturating_sub(mint_info.lamports());
    if rent_missing > 0 {
        transfer(
            CpiContext::new(
                ctx.accounts.system_program.key(),
                Transfer {
                    from: ctx.accounts.relayer.to_account_info(),
                    to: mint_info.clone(),
                },
            ),
            rent_missing,
        )?;
    }

    token_metadata_initialize(
        CpiContext::new_with_signer(
            token_program,
            TokenMetadataInitialize {
                program_id: ctx.accounts.token_program.to_account_info(),
                metadata: mint_info.clone(),
                update_authority: voucher_info.clone(),
                mint_authority: voucher_info.clone(),
                mint: mint_info.clone(),
            },
            signer_seeds,
        ),
        name,
        VOUCHER_SYMBOL.to_string(),
        uri,
    )?;

    mint_to_checked(
        CpiContext::new_with_signer(
            token_program,
            MintToChecked {
                mint: mint_info.clone(),
                to: ctx.accounts.customer_token.to_account_info(),
                authority: voucher_info.clone(),
            },
            signer_seeds,
        ),
        1,
        0,
    )?;

    // Give up the right to mint, so only one of these tokens can ever exist.
    set_authority(
        CpiContext::new_with_signer(
            token_program,
            SetAuthority {
                current_authority: voucher_info,
                account_or_mint: mint_info,
            },
            signer_seeds,
        ),
        AuthorityType::MintTokens,
        None,
    )?;

    Ok(())
}

#[derive(Accounts)]
#[instruction(voucher_id: u64)]
pub struct MintVoucher<'info> {
    #[account(mut)]
    pub business: Account<'info, Business>,

    #[account(
        mut,
        seeds = [b"card", business.key().as_ref(), customer.key().as_ref()],
        bump = card.bump,
    )]
    pub card: Account<'info, LoyaltyCard>,

    #[account(
        init,
        payer = relayer,
        space = 8 + Voucher::INIT_SPACE,
        seeds = [b"voucher", business.key().as_ref(), &voucher_id.to_le_bytes()],
        bump,
    )]
    pub voucher: Account<'info, Voucher>,

    /// The voucher's NFT. The voucher account is its mint authority, freeze
    /// authority and permanent delegate, so only this program can freeze,
    /// thaw or burn it. The metadata lives inside the mint account itself.
    #[account(
        init,
        payer = relayer,
        seeds = [b"voucher_mint", business.key().as_ref(), &voucher_id.to_le_bytes()],
        bump,
        mint::decimals = 0,
        mint::authority = voucher,
        mint::freeze_authority = voucher,
        mint::token_program = token_program,
        extensions::metadata_pointer::metadata_address = mint,
        extensions::permanent_delegate::delegate = voucher,
    )]
    pub mint: Box<InterfaceAccount<'info, Mint>>,

    #[account(
        init,
        payer = relayer,
        associated_token::mint = mint,
        associated_token::authority = customer,
        associated_token::token_program = token_program,
    )]
    pub customer_token: Box<InterfaceAccount<'info, TokenAccount>>,

    /// The customer converting their stamps into a voucher. Signs to
    /// authorize it, but pays nothing.
    pub customer: Signer<'info>,

    /// The relayer, covering the rent for the voucher, its mint and the
    /// customer's token account on the customer's behalf.
    #[account(mut)]
    pub relayer: Signer<'info>,

    pub token_program: Program<'info, Token2022>,

    pub associated_token_program: Program<'info, AssociatedToken>,

    pub system_program: Program<'info, System>,
}
