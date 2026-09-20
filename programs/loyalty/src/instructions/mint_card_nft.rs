use anchor_lang::prelude::*;
use anchor_lang::system_program::{
    allocate, assign, create_account, transfer, Allocate, Assign, CreateAccount, Transfer,
};
use anchor_spl::associated_token::{create as create_associated_token, AssociatedToken, Create};
use anchor_spl::token_interface::{
    initialize_mint2, metadata_pointer_initialize, mint_close_authority_initialize,
    mint_to_checked, non_transferable_mint_initialize, permanent_delegate_initialize,
    set_authority,
    spl_pod::optional_keys::OptionalNonZeroPubkey,
    spl_token_2022::{extension::ExtensionType, instruction::AuthorityType, state::Mint as MintState},
    spl_token_metadata_interface::state::TokenMetadata,
    token_metadata_initialize, InitializeMint2, MetadataPointerInitialize,
    MintCloseAuthorityInitialize, MintToChecked, NonTransferableMintInitialize,
    PermanentDelegateInitialize, SetAuthority, Token2022, TokenMetadataInitialize,
};
use crate::state::{Business, LoyaltyCard};
use crate::error::ErrorCode;
use crate::constants::{CARD_SYMBOL, MAX_VOUCHER_URI_LEN};

/// Gives a stamp card its NFT: a token that sits in the customer's wallet,
/// can't be moved to another wallet, and is burned when the stamps are spent
/// (see `mint_voucher`). The live stamp count stays in the card account.
///
/// A card has at most one NFT at a time. The NFT's address depends on
/// `card.nft_cycle`, which goes up each time the NFT is burned, so asking
/// again for the same cycle fails because the mint already exists.
pub fn mint_card_nft_handler(ctx: Context<MintCardNft>, uri: String) -> Result<()> {
    require!(uri.len() <= MAX_VOUCHER_URI_LEN, ErrorCode::UriTooLong);

    let business = &ctx.accounts.business;
    let card = &ctx.accounts.card;

    // The name is built here, not taken from the client, so a customer can't
    // pass off a card as another business's.
    let name = format!("{} stamp card", business.name);
    let metadata = TokenMetadata {
        update_authority: OptionalNonZeroPubkey::try_from(Some(card.key()))?,
        mint: ctx.accounts.mint.key(),
        name: name.clone(),
        symbol: CARD_SYMBOL.to_string(),
        uri: uri.clone(),
        additional_metadata: vec![],
    };

    let card_key = card.key();
    let cycle_bytes = card.nft_cycle.to_le_bytes();
    let mint_bump = [ctx.bumps.mint];
    let mint_seeds: &[&[u8]] = &[b"card_mint", card_key.as_ref(), &cycle_bytes, &mint_bump];
    let mint_signer = &[mint_seeds];
    let card_bump = [card.bump];
    let card_seeds: &[&[u8]] =
        &[b"card", card.business.as_ref(), card.customer.as_ref(), &card_bump];
    let card_signer = &[card_seeds];

    let token_program = ctx.accounts.token_program.key();
    let system_program = ctx.accounts.system_program.key();
    let mint_info = ctx.accounts.mint.to_account_info();
    let card_info = ctx.accounts.card.to_account_info();
    let relayer_info = ctx.accounts.relayer.to_account_info();

    // The mint carries four extensions, and the metadata is added afterwards.
    // The token program grows the account for it but doesn't pay, so the rent
    // for the final size has to be in the account already.
    let space = ExtensionType::try_calculate_account_len::<MintState>(&[
        ExtensionType::MetadataPointer,
        ExtensionType::PermanentDelegate,
        ExtensionType::NonTransferable,
        ExtensionType::MintCloseAuthority,
    ])?;
    let rent_needed = Rent::get()?.minimum_balance(space + metadata.tlv_size_of()?);

    // Anyone can send lamports to this address in advance, and `create_account`
    // refuses an address that already holds any. Do it in steps instead, so
    // nobody can block a card's NFT that way.
    if mint_info.lamports() == 0 {
        create_account(
            CpiContext::new_with_signer(
                system_program,
                CreateAccount { from: relayer_info.clone(), to: mint_info.clone() },
                mint_signer,
            ),
            rent_needed,
            space as u64,
            &token_program,
        )?;
    } else {
        let rent_missing = rent_needed.saturating_sub(mint_info.lamports());
        if rent_missing > 0 {
            transfer(
                CpiContext::new(
                    system_program,
                    Transfer { from: relayer_info.clone(), to: mint_info.clone() },
                ),
                rent_missing,
            )?;
        }
        allocate(
            CpiContext::new_with_signer(
                system_program,
                Allocate { account_to_allocate: mint_info.clone() },
                mint_signer,
            ),
            space as u64,
        )?;
        assign(
            CpiContext::new_with_signer(
                system_program,
                Assign { account_to_assign: mint_info.clone() },
                mint_signer,
            ),
            &token_program,
        )?;
    }

    // The extensions must be set up before the mint itself.
    metadata_pointer_initialize(
        CpiContext::new(
            token_program,
            MetadataPointerInitialize {
                token_program_id: ctx.accounts.token_program.to_account_info(),
                mint: mint_info.clone(),
            },
        ),
        None,
        Some(mint_info.key()),
    )?;
    permanent_delegate_initialize(
        CpiContext::new(
            token_program,
            PermanentDelegateInitialize {
                token_program_id: ctx.accounts.token_program.to_account_info(),
                mint: mint_info.clone(),
            },
        ),
        &card_key,
    )?;
    non_transferable_mint_initialize(CpiContext::new(
        token_program,
        NonTransferableMintInitialize {
            token_program_id: ctx.accounts.token_program.to_account_info(),
            mint: mint_info.clone(),
        },
    ))?;
    // So the empty mint can be closed and its rent handed back once the card
    // is burned.
    mint_close_authority_initialize(
        CpiContext::new(
            token_program,
            MintCloseAuthorityInitialize {
                token_program_id: ctx.accounts.token_program.to_account_info(),
                mint: mint_info.clone(),
            },
        ),
        Some(&card_key),
    )?;
    initialize_mint2(
        CpiContext::new(token_program, InitializeMint2 { mint: mint_info.clone() }),
        0,
        &card_key,
        None,
    )?;

    token_metadata_initialize(
        CpiContext::new_with_signer(
            token_program,
            TokenMetadataInitialize {
                program_id: ctx.accounts.token_program.to_account_info(),
                metadata: mint_info.clone(),
                update_authority: card_info.clone(),
                mint_authority: card_info.clone(),
                mint: mint_info.clone(),
            },
            card_signer,
        ),
        name,
        CARD_SYMBOL.to_string(),
        uri,
    )?;

    create_associated_token(CpiContext::new(
        ctx.accounts.associated_token_program.key(),
        Create {
            payer: relayer_info,
            associated_token: ctx.accounts.customer_token.to_account_info(),
            authority: ctx.accounts.customer.to_account_info(),
            mint: mint_info.clone(),
            system_program: ctx.accounts.system_program.to_account_info(),
            token_program: ctx.accounts.token_program.to_account_info(),
        },
    ))?;

    mint_to_checked(
        CpiContext::new_with_signer(
            token_program,
            MintToChecked {
                mint: mint_info.clone(),
                to: ctx.accounts.customer_token.to_account_info(),
                authority: card_info.clone(),
            },
            card_signer,
        ),
        1,
        0,
    )?;

    // Give up the right to mint, so only one of these tokens can ever exist.
    set_authority(
        CpiContext::new_with_signer(
            token_program,
            SetAuthority { current_authority: card_info, account_or_mint: mint_info },
            card_signer,
        ),
        AuthorityType::MintTokens,
        None,
    )?;

    Ok(())
}

#[derive(Accounts)]
pub struct MintCardNft<'info> {
    pub business: Account<'info, Business>,

    #[account(
        seeds = [b"card", business.key().as_ref(), customer.key().as_ref()],
        bump = card.bump,
        has_one = business,
        has_one = customer,
    )]
    pub card: Account<'info, LoyaltyCard>,

    /// CHECK: The card's NFT, created here. The address is fixed by the seeds
    /// (the card and its current cycle), and the account is set up by hand
    /// because Anchor can't declare the "can't be moved" extension.
    #[account(
        mut,
        seeds = [b"card_mint", card.key().as_ref(), &card.nft_cycle.to_le_bytes()],
        bump,
    )]
    pub mint: UncheckedAccount<'info>,

    /// CHECK: The customer's token account for the NFT, created here by the
    /// associated token program, which checks that this is the right address.
    #[account(mut)]
    pub customer_token: UncheckedAccount<'info>,

    /// The customer receiving the card NFT. Signs to authorize it, but pays
    /// nothing.
    pub customer: Signer<'info>,

    /// The relayer, covering the rent for the mint and the token account.
    #[account(mut)]
    pub relayer: Signer<'info>,

    pub token_program: Program<'info, Token2022>,

    pub associated_token_program: Program<'info, AssociatedToken>,

    pub system_program: Program<'info, System>,
}
