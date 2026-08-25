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
}
