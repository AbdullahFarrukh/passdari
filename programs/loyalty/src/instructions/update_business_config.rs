use anchor_lang::prelude::*;
use crate::state::Business;

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

pub fn update_business_config_handler(
    ctx: Context<UpdateBusinessConfig>,
    reward_label: String,
    stamps_required: u8,
    min_purchase_amount: u64,
    receipt_ttl_seconds: u32,
) -> Result<()> {
    let business = &mut ctx.accounts.business;
    business.reward_label = reward_label;
    business.stamps_required = stamps_required;
    business.min_purchase_amount = min_purchase_amount;
    business.receipt_ttl_seconds = receipt_ttl_seconds;

    msg!("Business config updated: {:?}", business.authority);
    Ok(())
}