use anchor_lang::prelude::*;
use crate::state::Business;
use crate::instructions::business_rules::{check_settings, check_terms_lock};
use crate::error::ErrorCode;

#[derive(Accounts)]
pub struct UpdateBusinessConfig<'info> {
    #[account(
        mut,
        seeds = [b"business", authority.key().as_ref()],
        bump = business.bump,
        has_one = authority
    )]
    pub business: Account<'info, Business>,

    pub authority: Signer<'info>,
}

/// The terms of the shop's reward, changeable by the shop itself.
///
/// The name and the category are deliberately NOT here. They are the shop's identity, not its offer:
/// the name is written into every card and voucher NFT at the moment it is minted and is never
/// rewritten, so letting it change would leave tokens in customers' wallets naming a shop that no
/// longer calls itself that — and would let a shop rename itself as another. The category is what the
/// public directory files it under. Both are fixed at registration.
///
/// What this does NOT change either: the stamp cards customers already hold. Each keeps the
/// `stamps_required_snapshot` it was opened under, so a shop raising its price from 8 stamps to 10
/// cannot move the goalposts on someone already seven stamps in. The new number applies to cards opened
/// from then on.
///
/// The reward's NAME is different, and worth being clear about: it is read from the shop at the moment
/// a voucher is minted, not stored on the card. A customer part-way through a card will receive
/// whatever the reward is called when they cash in. That is deliberate — a shop that stops selling
/// croissants should not owe five hundred cards a croissant — and the customer's own screen shows the
/// current reward on their card, so what they see is always what they will get.
pub fn update_business_config_handler(
    ctx: Context<UpdateBusinessConfig>,
    reward_label: String,
    stamps_required: u8,
    min_purchase_amount: u64,
    receipt_ttl_seconds: u32,
    terms_locked_until: i64,
) -> Result<()> {
    // The name and category are not being changed, but they are passed through the same check so the
    // rules live in exactly one place.
    let (name, category) = {
        let business = &ctx.accounts.business;
        (business.name.clone(), business.category.clone())
    };
    check_settings(&name, &category, &reward_label, stamps_required, receipt_ttl_seconds)?;

    // The heart of it: while a shop's promise stands, it cannot change what it promised. A customer
    // collecting towards "free pizza until 2 February" will still be collecting towards free pizza
    // when they finish.
    let now = Clock::get()?.unix_timestamp;
    let current_lock = ctx.accounts.business.terms_locked_until;
    require!(now >= current_lock, ErrorCode::RewardTermsLocked);
    check_terms_lock(now, current_lock, terms_locked_until)?;

    let business = &mut ctx.accounts.business;
    business.terms_locked_until = terms_locked_until;
    business.reward_label = reward_label;
    business.stamps_required = stamps_required;
    business.min_purchase_amount = min_purchase_amount;
    business.receipt_ttl_seconds = receipt_ttl_seconds;

    msg!("Reward terms updated: {:?}", business.authority);
    Ok(())
}
