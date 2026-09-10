use anchor_lang::prelude::*;
use crate::state::{Business, LoyaltyCard, Voucher};
use crate::error::ErrorCode;

pub fn mint_voucher_handler(ctx: Context<MintVoucher>, voucher_id: u64) -> Result<()> {
    let business = &mut ctx.accounts.business;
    let card = &mut ctx.accounts.card;
    let voucher = &mut ctx.accounts.voucher;
    let clock = Clock::get()?;

        require_eq!(voucher_id, business.total_vouchers_issued, ErrorCode::InvalidVoucherId);
    require!(card.stamps >= card.stamps_required_snapshot, ErrorCode::NotEnoughStamps);

    card.stamps -= card.stamps_required_snapshot;

    voucher.business = business.key();
    voucher.owner = ctx.accounts.customer.key();
    voucher.voucher_id = voucher_id;
    voucher.minted_at = clock.unix_timestamp;
    voucher.pending_redemption = false;
    voucher.bump = ctx.bumps.voucher;

    business.total_vouchers_issued += 1;

    Ok(())
}

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
        payer = relayer,
        space = 8 + Voucher::INIT_SPACE,
        seeds = [b"voucher", business.key().as_ref(), &voucher_id.to_le_bytes()],
        bump,
    )]
    pub voucher: Account<'info, Voucher>,

    /// The customer converting their stamps into a voucher. Signs to
    /// authorize it, but pays nothing.
    pub customer: Signer<'info>,

    /// The relayer, covering the voucher's rent on the customer's behalf.
    #[account(mut)]
    pub relayer: Signer<'info>,

    pub system_program: Program<'info, System>,
}