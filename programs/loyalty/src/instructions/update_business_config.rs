use anchor_lang::prelude::*;
use crate::state::Business;
use crate::instructions::business_rules::check_settings;

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

/// Everything a merchant filled in when they registered, editable afterwards — the name and category
/// included, which they were not before.
///
/// What this deliberately does NOT change: the stamp cards customers already hold. Each card keeps the
/// `stamps_required_snapshot` it was opened under, so a shop that raises its price from 8 stamps to 10
/// cannot move the goalposts on someone already seven stamps in. The new number applies to cards opened
/// from now on. The same goes for NFTs already minted: a voucher or card NFT carries the name it was
/// given at the time, because that text lives inside the token and is not rewritten later.
pub fn update_business_config_handler(
    ctx: Context<UpdateBusinessConfig>,
    name: String,
    category: String,
    reward_label: String,
    stamps_required: u8,
    min_purchase_amount: u64,
    receipt_ttl_seconds: u32,
) -> Result<()> {
    check_settings(&name, &category, &reward_label, stamps_required, receipt_ttl_seconds)?;

    let business = &mut ctx.accounts.business;
    business.name = name;
    business.category = category;
    business.reward_label = reward_label;
    business.stamps_required = stamps_required;
    business.min_purchase_amount = min_purchase_amount;
    business.receipt_ttl_seconds = receipt_ttl_seconds;

    msg!("Business settings updated: {:?}", business.authority);
    Ok(())
}