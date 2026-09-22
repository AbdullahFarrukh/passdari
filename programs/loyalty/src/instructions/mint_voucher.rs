use anchor_lang::prelude::*;
use anchor_lang::system_program::{transfer, Transfer};
use anchor_spl::associated_token::{get_associated_token_address_with_program_id, AssociatedToken};
use anchor_spl::token_interface::{
    mint_to_checked, set_authority, spl_pod::optional_keys::OptionalNonZeroPubkey,
    spl_token_2022::instruction::AuthorityType,
    spl_token_metadata_interface::state::TokenMetadata, token_metadata_initialize, Mint,
    MintToChecked, SetAuthority, Token2022, TokenAccount, TokenMetadataInitialize,
};
use crate::state::{Business, CardNft, LoyaltyCard, Voucher};
use crate::error::ErrorCode;
use crate::constants::{MAX_VOUCHER_URI_LEN, VOUCHER_SYMBOL, VOUCHER_VALID_SECONDS};
use crate::instructions::mint_card_nft::{burn_and_close_card_nft, close_program_account};

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
    voucher.rent_payer = ctx.accounts.relayer.key();
    voucher.expires_at = clock.unix_timestamp + VOUCHER_VALID_SECONDS;
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

    // The customer hands the voucher the right to close their token account,
    // so it can be closed (and its rent returned) when the voucher is redeemed
    // or expires, even if they never come back. They sign this transaction,
    // so they can grant it now.
    set_authority(
        CpiContext::new(
            token_program,
            SetAuthority {
                current_authority: ctx.accounts.customer.to_account_info(),
                account_or_mint: ctx.accounts.customer_token.to_account_info(),
            },
        ),
        AuthorityType::CloseAccount,
        Some(voucher_info.key()),
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

    // The stamps are spent, so the card's NFT is spent too. The card keeps
    // any leftover stamps and gets a fresh NFT with its next stamp.
    if retire_card_nft(&ctx.accounts)? {
        ctx.accounts.card.nft_cycle += 1;
    }

    Ok(())
}

/// Burns the customer's card NFT and closes its accounts, sending the rent
/// back to whoever paid for it. Returns false when the card has no NFT, which
/// is the case for cards made before card NFTs existed.
///
/// It copes with a customer who burned the token themselves: nothing is left
/// to burn then, but the accounts are still cleaned up, so cashing in can
/// never get stuck on the card NFT.
fn retire_card_nft(accounts: &MintVoucher) -> Result<bool> {
    let card_mint = accounts.card_mint.to_account_info();
    if *card_mint.owner != accounts.token_program.key() || card_mint.data_is_empty() {
        return Ok(false);
    }

    // The NFT's record says who paid for it. NFTs made before these records
    // existed have none; their rent goes to the relayer signing this cash-in.
    let record = accounts.card_nft_record.to_account_info();
    let has_record = *record.owner == crate::ID && !record.data_is_empty();
    let destination = if has_record {
        let payer = CardNft::try_deserialize(&mut &record.try_borrow_data()?[..])?.rent_payer;
        require_keys_eq!(accounts.card_rent_payer.key(), payer, ErrorCode::WrongRentPayer);
        accounts.card_rent_payer.to_account_info()
    } else {
        accounts.relayer.to_account_info()
    };

    let card = &accounts.card;
    let bump = [card.bump];
    let seeds: &[&[u8]] = &[b"card", card.business.as_ref(), card.customer.as_ref(), &bump];
    burn_and_close_card_nft(
        &card.to_account_info(),
        &[seeds],
        &card_mint,
        &accounts.card_token.to_account_info(),
        Some(&accounts.customer.to_account_info()),
        &destination,
        &accounts.token_program.to_account_info(),
    )?;
    if has_record {
        close_program_account(&record, &destination)?;
    }
    Ok(true)
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
    /// authority, permanent delegate and close authority, so only this
    /// program can freeze, thaw, burn or close it. The metadata lives inside
    /// the mint account itself.
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
        extensions::close_authority::authority = voucher,
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

    /// CHECK: The mint of the card's current NFT. Only its address is fixed
    /// here (by the card and its cycle); if nothing exists there the card has
    /// no NFT yet and this step is skipped.
    #[account(
        mut,
        seeds = [b"card_mint", card.key().as_ref(), &card.nft_cycle.to_le_bytes()],
        bump,
    )]
    pub card_mint: UncheckedAccount<'info>,

    /// CHECK: The customer's token account for the card NFT. Checked to be
    /// the customer's associated token account for that mint; it may be
    /// missing if there is no NFT or the customer already closed it.
    #[account(
        mut,
        address = get_associated_token_address_with_program_id(
            &customer.key(),
            &card_mint.key(),
            &token_program.key(),
        ),
    )]
    pub card_token: UncheckedAccount<'info>,

    /// CHECK: The card NFT's record of who paid for it. Its address is fixed
    /// by the NFT's mint; it may not exist (no NFT, or one made before records
    /// existed), which the handler checks.
    #[account(
        mut,
        seeds = [b"card_nft", card_mint.key().as_ref()],
        bump,
    )]
    pub card_nft_record: UncheckedAccount<'info>,

    /// CHECK: Receives the card NFT's rent. When the record exists this must
    /// be the wallet it names, which the handler checks.
    #[account(mut)]
    pub card_rent_payer: UncheckedAccount<'info>,

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
