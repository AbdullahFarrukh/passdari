pub mod constants;
pub mod error;
pub mod instructions;
pub mod state;

use anchor_lang::prelude::*;

pub use constants::*;
pub use instructions::*;
pub use state::*;

declare_id!("HWvvvwSEounpNXcbD4JUNmniB5YxTcFNYoAestzJJCuL");

#[program]
pub mod loyalty {
    use super::*;

        pub fn initialize(ctx: Context<Initialize>) -> Result<()> {
        crate::instructions::initialize::initialize_handler(ctx)
    }

    pub fn register_business(
        ctx: Context<RegisterBusiness>,
        name: String,
        category: String,
        reward_label: String,
        stamps_required: u8,
        min_purchase_amount: u64,
        currency: String,
        receipt_ttl_seconds: u32,
    ) -> Result<()> {
        crate::instructions::register_business::register_business_handler(
            ctx,
            name,
            category,
            reward_label,
            stamps_required,
            min_purchase_amount,
            currency,
            receipt_ttl_seconds,
        )
    }

        pub fn update_business_config(
        ctx: Context<UpdateBusinessConfig>,
        reward_label: String,
        stamps_required: u8,
        min_purchase_amount: u64,
        receipt_ttl_seconds: u32,
    ) -> Result<()> {
        crate::instructions::update_business_config::update_business_config_handler(
            ctx,
            reward_label,
            stamps_required,
            min_purchase_amount,
            receipt_ttl_seconds,
        )
    }

        pub fn issue_receipt(
        ctx: Context<IssueReceipt>,
        secret_hash: [u8; 32],
        amount_band: u8,
    ) -> Result<()> {
        crate::instructions::issue_receipt::issue_receipt_handler(ctx, secret_hash, amount_band)
    }

        pub fn claim_receipt(ctx: Context<ClaimReceipt>, secret: [u8; 32]) -> Result<()> {
        crate::instructions::claim_receipt::claim_receipt_handler(ctx, secret)
    }

        pub fn mint_voucher(ctx: Context<MintVoucher>, voucher_id: u64) -> Result<()> {
        crate::instructions::mint_voucher::mint_voucher_handler(ctx, voucher_id)
    }

    pub fn transfer_voucher(ctx: Context<TransferVoucher>, new_owner: Pubkey) -> Result<()> {
        crate::instructions::transfer_voucher::transfer_voucher_handler(ctx, new_owner)
    }

    pub fn present_voucher(ctx: Context<PresentVoucher>) -> Result<()> {
        crate::instructions::present_voucher::present_voucher_handler(ctx)
    }

    pub fn cancel_presentation(ctx: Context<CancelPresentation>) -> Result<()> {
        crate::instructions::cancel_presentation::cancel_presentation_handler(ctx)
    }

    pub fn redeem_voucher(ctx: Context<RedeemVoucher>) -> Result<()> {
        crate::instructions::redeem_voucher::redeem_voucher_handler(ctx)
    }

    pub fn reclaim_expired_receipt(ctx: Context<ReclaimExpiredReceipt>) -> Result<()> {
        crate::instructions::reclaim_expired_receipt::reclaim_expired_receipt_handler(ctx)
    }
    
}
