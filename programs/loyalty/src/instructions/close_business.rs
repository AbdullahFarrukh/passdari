use anchor_lang::prelude::*;
use crate::state::Business;
use crate::error::ErrorCode;

/// Closes a shop that has finished trading, and sends its rent back to whoever paid it.
///
/// A business account was the last thing in this program whose rent could never come back. It is also
/// the most dangerous thing to close, because every stamp card at the shop points at it — so this
/// refuses unless the shop genuinely owes nobody anything:
///
/// 1. **Its offer has ended.** A shop cannot walk away in the middle of the reward it committed to.
///    While `terms_locked_until` is in the future the offer is still running, and the shop stays.
///
/// 2. **No stamp cards are open.** `open_cards` counts live cards: up on the first stamp of a new one,
///    down when a dead one is closed. While anyone still holds stamps here, closing would destroy them.
///    In practice a shop that stops trading waits for its customers' cards to go a year untouched and
///    be swept, and only then can it close. That is slow on purpose — it is somebody's unfinished
///    coffee.
///
/// 3. **No vouchers are owed.** Every reward handed out has been redeemed or has expired and been
///    cleaned up, so nobody is left holding a voucher for a shop that no longer exists.
///
/// Permissionless like every other closing instruction, because the rent can only go to the wallet the
/// shop itself names.
pub fn close_business_handler(ctx: Context<CloseBusiness>) -> Result<()> {
    let business = &ctx.accounts.business;

    require!(
        Clock::get()?.unix_timestamp >= business.terms_locked_until,
        ErrorCode::ShopOfferStillRunning
    );
    require!(business.open_cards == 0, ErrorCode::ShopStillHasOpenCards);
    // open_vouchers, not total_issued == total_redeemed: a voucher that expired unclaimed is cleaned
    // up without ever counting as a redemption, so that comparison would never come true again once a
    // single reward went unclaimed.
    require!(business.open_vouchers == 0, ErrorCode::ShopStillOwesVouchers);

    msg!("Closing shop {}", business.name);
    Ok(())
}

#[derive(Accounts)]
pub struct CloseBusiness<'info> {
    #[account(
        mut,
        close = rent_payer,
        has_one = rent_payer,
        seeds = [b"business", business.authority.as_ref()],
        bump = business.bump,
    )]
    pub business: Account<'info, Business>,

    /// The wallet that paid for this account, recorded on it. Receives the rent back.
    #[account(mut)]
    pub rent_payer: SystemAccount<'info>,
}
