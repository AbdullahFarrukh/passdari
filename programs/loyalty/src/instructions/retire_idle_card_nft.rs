use anchor_lang::prelude::*;
use anchor_spl::associated_token::get_associated_token_address_with_program_id;
use anchor_spl::token_interface::Token2022;
use crate::state::{CardNft, LoyaltyCard};
use crate::error::ErrorCode;
use crate::constants::CARD_NFT_IDLE_SECONDS;
use crate::instructions::mint_card_nft::burn_and_close_card_nft;

/// Recycles the NFT of a card that has gone 90 days without a stamp: burns
/// it, closes its accounts and sends their rent back to whoever paid for it.
/// The stamps stay on the card, and the next stamp brings a new NFT. Anyone may
/// call this — the merchant's clean-up button or the app's daily clean-up job
/// — because the rent can only go back to the wallet the NFT's record names.
#[derive(Accounts)]
pub struct RetireIdleCardNft<'info> {
    #[account(mut, has_one = customer)]
    pub card: Account<'info, LoyaltyCard>,

    /// CHECK: The card's customer. Only used to find their token account; it
    /// never signs, and must be the customer the card names.
    pub customer: UncheckedAccount<'info>,

    /// CHECK: The card's current NFT. Its address is fixed by the card and its
    /// cycle, and its record (below) only exists if the NFT does.
    #[account(
        mut,
        seeds = [b"card_mint", card.key().as_ref(), &card.nft_cycle.to_le_bytes()],
        bump,
    )]
    pub card_mint: UncheckedAccount<'info>,

    /// CHECK: The customer's token account for that NFT, checked by address.
    #[account(
        mut,
        address = get_associated_token_address_with_program_id(
            &customer.key(),
            &card_mint.key(),
            &token_program.key(),
        ),
    )]
    pub card_token: UncheckedAccount<'info>,

    #[account(
        mut,
        close = rent_payer,
        has_one = rent_payer,
        seeds = [b"card_nft", card_mint.key().as_ref()],
        bump = record.bump,
    )]
    pub record: Account<'info, CardNft>,

    /// The wallet that paid for the NFT, recorded in its record. Receives the
    /// rent.
    #[account(mut)]
    pub rent_payer: SystemAccount<'info>,

    pub token_program: Program<'info, Token2022>,
}

pub fn retire_idle_card_nft_handler(ctx: Context<RetireIdleCardNft>) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    let card = &ctx.accounts.card;
    require!(now - card.last_stamp_ts >= CARD_NFT_IDLE_SECONDS, ErrorCode::CardNftNotIdle);

    let bump = [card.bump];
    let seeds: &[&[u8]] = &[b"card", card.business.as_ref(), card.customer.as_ref(), &bump];
    burn_and_close_card_nft(
        &card.to_account_info(),
        &[seeds],
        &ctx.accounts.card_mint.to_account_info(),
        &ctx.accounts.card_token.to_account_info(),
        None,
        &ctx.accounts.rent_payer.to_account_info(),
        &ctx.accounts.token_program.to_account_info(),
    )?;

    // The next stamp brings a fresh NFT at a new address, as after cashing in.
    ctx.accounts.card.nft_cycle += 1;
    Ok(())
}
