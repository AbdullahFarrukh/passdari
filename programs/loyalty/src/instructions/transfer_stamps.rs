use anchor_lang::prelude::*;
use crate::state::{Business, LoyaltyCard};
use crate::error::ErrorCode;

/// Passing stamps to someone else's card at the same shop.
///
/// Two people three stamps short of a free coffee each are two people who never get a coffee. Letting one
/// of them hand their stamps to the other is the smallest social thing this app can do, and it is the one
/// reason a customer has to tell a friend about it.
///
/// Three rules make it safe to do on-chain:
///
/// 1. **Same shop only.** A stamp is a debt the shop that issued it owes. Carrying stamps from one shop to
///    another would make a second shop owe a reward for a purchase it never sold, so `has_one = business`
///    on both cards stops it at the program level, not just in the app.
///
/// 2. **The stamps leave the sender.** Nothing is created here. The sender's card goes down by exactly what
///    the receiver's goes up by, so the total a shop owes never grows by a single stamp.
///
/// 3. **The receiving card must already exist.** There is deliberately no `init_if_needed`: if a transfer
///    could open a card, one person could send one stamp to a thousand fresh addresses and leave the
///    relayer paying rent on a thousand accounts forever. You can only pass stamps to someone who already
///    shops there.
///
/// `lifetime_stamps` moves with the stamps on purpose. The merchant's leaderboard works out how many
/// rewards someone has finished from `lifetime_stamps - stamps`, which assumes stamps only ever leave a
/// card by being spent on a voucher. Moving both together keeps that difference unchanged on both cards,
/// so a transfer can't be used to climb the leaderboard. The cost is that `lifetime_stamps` now counts
/// stamps *credited to this card* rather than stamps this person earned at the counter.
///
/// `last_stamp_ts` is left alone. It means "when this customer last earned a stamp by buying something",
/// and receiving stamps from a friend isn't that.
pub fn transfer_stamps_handler(ctx: Context<TransferStamps>, amount: u8) -> Result<()> {
    require!(amount > 0, ErrorCode::InvalidStampAmount);

    // Sending a card's stamps to itself needs no check here: both cards are `Account<..>` and both are
    // `mut`, so Anchor rejects the same address passed twice with ConstraintDuplicateMutableAccount
    // before this handler is ever reached.
    let from = &mut ctx.accounts.from_card;
    let to = &mut ctx.accounts.to_card;

    require!(from.stamps >= amount, ErrorCode::NotEnoughStamps);
    // A card holds its stamp count in a single byte, so a receiver near the top can't take any more.
    let to_stamps = to.stamps.checked_add(amount).ok_or(ErrorCode::TooManyStamps)?;

    let moved = amount as u32;
    from.stamps -= amount;
    to.stamps = to_stamps;
    // `lifetime_stamps` is always at least `stamps`, and no more than `stamps` is ever moved, so the
    // subtraction cannot go below zero. Checked anyway: this is the figure the leaderboard trusts.
    from.lifetime_stamps = from.lifetime_stamps.checked_sub(moved).ok_or(ErrorCode::NotEnoughStamps)?;
    to.lifetime_stamps = to.lifetime_stamps.checked_add(moved).ok_or(ErrorCode::TooManyStamps)?;

    emit!(StampsTransferred {
        business: ctx.accounts.business.key(),
        from: from.customer,
        to: to.customer,
        amount,
        timestamp: Clock::get()?.unix_timestamp,
    });

    Ok(())
}

#[derive(Accounts)]
pub struct TransferStamps<'info> {
    /// Both cards must belong to this shop. Nothing about the shop changes: no stamps are issued here,
    /// only moved, so its running totals stay exactly as they were.
    pub business: Account<'info, Business>,

    /// The sender's card. `has_one = customer` is what proves the signer owns it.
    #[account(mut, has_one = business, has_one = customer)]
    pub from_card: Account<'info, LoyaltyCard>,

    /// The receiving card. It has to exist already — see rule 3 above.
    #[account(mut, has_one = business)]
    pub to_card: Account<'info, LoyaltyCard>,

    /// The person giving their stamps away. Signs to prove it's them, and pays nothing.
    pub customer: Signer<'info>,

    /// The relayer, covering the transaction fee as it does everywhere else. How often one wallet may do
    /// this is decided by the relayer before it co-signs, which is the right place for it: the limit
    /// exists to protect the relayer's own balance, and the relayer is free to simply refuse.
    #[account(mut)]
    pub relayer: Signer<'info>,
}

#[event]
pub struct StampsTransferred {
    pub business: Pubkey,
    pub from: Pubkey,
    pub to: Pubkey,
    pub amount: u8,
    pub timestamp: i64,
}
