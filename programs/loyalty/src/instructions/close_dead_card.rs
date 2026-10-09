use anchor_lang::prelude::*;
use crate::state::{Business, LoyaltyCard};
use crate::error::ErrorCode;
use crate::constants::CARD_DEAD_SECONDS;

/// Closes a stamp card nobody is coming back to, and sends its rent home.
///
/// Until now a card was the one thing in this program that could never be closed. Receipts, vouchers and
/// card NFTs all hand their rent back; a card held 1,173,480 lamports of the relayer's money for ever,
/// for every customer who ever collected a single stamp anywhere and never returned. At any real size
/// that is the largest permanent cost in the whole system.
///
/// Three conditions, all of them necessary:
///
/// 1. **No stamps left.** Closing a card destroys its stamps, so a card with any on it is somebody's
///    unfinished reward and is never touched. A card at zero has nothing to lose.
///
/// 2. **A year without a stamp.** Far longer than the 90 days that recycles an idle card's NFT, because
///    that one is reversible — the next stamp brings a fresh NFT — and this one is not.
///
/// 3. **Its NFT must already be gone.** This is the subtle one. Closing the card resets `nft_cycle` to
///    zero, so if the customer ever comes back, the first NFT minted for their new card lands on exactly
///    the address the very first one used. Were that mint still alive the mint would fail and the card
///    would be stuck unable to make an NFT. Requiring the current NFT to be gone keeps that address free.
///    In practice the 90-day sweep has long since taken it.
///
/// Permissionless, like every other closing instruction here: the rent can only ever go to the wallet
/// the card names, so there is nothing to gain by calling it on someone else's card.
pub fn close_dead_card_handler(ctx: Context<CloseDeadCard>) -> Result<()> {
    let card = &ctx.accounts.card;

    require!(card.stamps == 0, ErrorCode::CardNotEmpty);
    require!(
        Clock::get()?.unix_timestamp - card.last_stamp_ts >= CARD_DEAD_SECONDS,
        ErrorCode::CardNotDead
    );

    // A live mint account here means the card still has an NFT, so closing would strand its address.
    let mint = ctx.accounts.card_mint.to_account_info();
    require!(mint.data_is_empty(), ErrorCode::CardNftStillAlive);

    let business_key = card.business;
    // One fewer card open at this shop, which is what will eventually let the shop itself close.
    ctx.accounts.business.open_cards = ctx.accounts.business.open_cards.saturating_sub(1);
    msg!("Closing a dead card at {}", business_key);
    Ok(())
}

#[derive(Accounts)]
pub struct CloseDeadCard<'info> {
    #[account(
        mut,
        close = rent_payer,
        has_one = rent_payer,
        seeds = [b"card", card.business.as_ref(), card.customer.as_ref()],
        bump = card.bump,
    )]
    pub card: Account<'info, LoyaltyCard>,

    /// The shop this card belongs to, so its count of open cards comes down.
    #[account(mut, address = card.business)]
    pub business: Account<'info, Business>,

    /// CHECK: The card's current NFT mint. Only its address matters, fixed by the card and its cycle;
    /// the handler requires that nothing lives there.
    #[account(
        seeds = [b"card_mint", card.key().as_ref(), &card.nft_cycle.to_le_bytes()],
        bump,
    )]
    pub card_mint: UncheckedAccount<'info>,

    /// The wallet that paid for this card, recorded on it. Receives the rent back.
    #[account(mut)]
    pub rent_payer: SystemAccount<'info>,
}
