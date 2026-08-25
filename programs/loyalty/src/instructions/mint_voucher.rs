use anchor_lang::prelude::*;
use crate::state::{Business, LoyaltyCard, Voucher};
use crate::error::ErrorCode;

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
        payer = customer,
        space = 8 + Voucher::INIT_SPACE,
        seeds = [b"voucher", business.key().as_ref(), &voucher_id.to_le_bytes()],
        bump
    )]
    pub voucher: Account<'info, Voucher>,

    #[account(mut)]
    pub customer: Signer<'info>,

    pub system_program: Program<'info, System>,
}

pub fn mint_voucher_handler(ctx: Context<MintVoucher>, voucher_id: u64) -> Result<()> {
    let business = &mut ctx.accounts.business;
    require_eq!(voucher_id, business.total_vouchers_issued, ErrorCode::InvalidVoucherId);

    let card = &mut ctx.accounts.card;
    require!(card.stamps >= business.stamps_required, ErrorCode::NotEnoughStamps);
    card.stamps -= business.stamps_required;

    let business_key = business.key();
    let owner_key = ctx.accounts.customer.key();

    let voucher = &mut ctx.accounts.voucher;
    voucher.business = business_key;
    voucher.owner = owner_key;
    voucher.voucher_id = voucher_id;
    voucher.minted_at = Clock::get()?.unix_timestamp;
    voucher.pending_redemption = false;
    voucher.bump = ctx.bumps.voucher;

    business.total_vouchers_issued += 1;

    msg!("Voucher {} minted for business {:?}, owner {:?}", voucher_id, business_key, owner_key);
    Ok(())
}