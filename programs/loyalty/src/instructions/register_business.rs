use anchor_lang::prelude::*;
use crate::state::Business;

#[derive(Accounts)]
pub struct RegisterBusiness<'info> {
    #[account(
        init,
        payer = relayer,
        space = 8 + Business::INIT_SPACE,
        seeds = [b"business", authority.key().as_ref()],
        bump
    )]
    pub business: Account<'info, Business>,

    /// The merchant registering this business. Signs to prove it's really
    /// them — their identity is baked directly into the business's own
    /// address — but pays nothing.
    pub authority: Signer<'info>,

    /// The relayer, covering the business account's rent on the merchant's
    /// behalf.
    #[account(mut)]
    pub relayer: Signer<'info>,

    pub system_program: Program<'info, System>,
}

pub fn register_business_handler(
    ctx: Context<RegisterBusiness>,
    name: String,
    category: String,
    reward_label: String,
    stamps_required: u8,
    min_purchase_amount: u64,
    currency: String,
    receipt_ttl_seconds: u32,
) -> Result<()> {
    let business = &mut ctx.accounts.business;
    business.authority = ctx.accounts.authority.key();
    business.name = name;
    business.category = category;
    business.reward_label = reward_label;
    business.stamps_required = stamps_required;
    business.min_purchase_amount = min_purchase_amount;
    business.currency = currency;
    business.receipt_ttl_seconds = receipt_ttl_seconds;
    business.total_cards = 0;
    business.total_stamps_issued = 0;
    business.total_vouchers_issued = 0;
    business.total_redemptions = 0;
    business.bump = ctx.bumps.business;

    msg!("Business registered: {:?}", business.authority);
    Ok(())
}