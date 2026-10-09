use anchor_lang::prelude::*;
use anchor_lang::system_program::{transfer, Transfer};
use anchor_lang::Discriminator;
use crate::state::Business;
use crate::error::ErrorCode;

/// Brings a shop registered before it carried a commitment date, live counts and its own rent payer up
/// to the current shape.
///
/// Unlike a card, a shop cannot be read at fixed offsets. Its name, category and reward label are Borsh
/// strings — length-prefixed, holding only the characters actually used — so every field after them
/// sits at a different place in every account. `#[max_len]` sizes the account, not the data. So the old
/// shape is declared here and deserialized properly, and the new one written back whole.
///
/// The counts are rebuilt as closely as the account can tell, erring in the safe direction:
/// `open_cards` starts at every card ever started, and `open_vouchers` at those issued minus those
/// redeemed. Both can only overstate what is outstanding, which can delay a shop closing but can never
/// let one close while a customer still holds stamps or an unredeemed reward. They settle to the truth
/// as cards and vouchers are closed from here on.
///
/// The commitment date is set to now: a shop that never made a promise is not held to one.
#[derive(AnchorDeserialize)]
struct BusinessV1 {
    authority: Pubkey,
    name: String,
    category: String,
    reward_label: String,
    stamps_required: u8,
    min_purchase_amount: u64,
    currency: String,
    receipt_ttl_seconds: u32,
    receipts_window_start: i64,
    receipts_this_window: u32,
    total_cards: u32,
    total_stamps_issued: u64,
    total_vouchers_issued: u64,
    total_redemptions: u32,
    bump: u8,
}

pub fn migrate_business_handler(ctx: Context<MigrateBusiness>) -> Result<()> {
    let info = ctx.accounts.business.to_account_info();
    let new_len = 8 + Business::INIT_SPACE;

    require_keys_eq!(*info.owner, crate::ID, ErrorCode::NotABusiness);

    let old: BusinessV1 = {
        let data = info.try_borrow_data()?;
        require!(data.len() >= 8, ErrorCode::NotABusiness);
        require!(data[..8] == Business::DISCRIMINATOR[..], ErrorCode::NotABusiness);
        require!(data.len() < new_len, ErrorCode::BusinessAlreadyMigrated);
        BusinessV1::deserialize(&mut &data[8..]).map_err(|_| error!(ErrorCode::NotABusiness))?
    };

    let expected = Pubkey::create_program_address(
        &[b"business", old.authority.as_ref(), &[old.bump]],
        &crate::ID,
    )
    .map_err(|_| error!(ErrorCode::NotABusiness))?;
    require_keys_eq!(expected, info.key(), ErrorCode::NotABusiness);

    let rent = Rent::get()?;
    let extra = rent.minimum_balance(new_len).saturating_sub(info.lamports());
    if extra > 0 {
        transfer(
            CpiContext::new(
                ctx.accounts.system_program.key(),
                Transfer { from: ctx.accounts.relayer.to_account_info(), to: info.clone() },
            ),
            extra,
        )?;
    }
    info.resize(new_len)?;

    let now = Clock::get()?.unix_timestamp;
    let updated = Business {
        authority: old.authority,
        name: old.name,
        category: old.category,
        reward_label: old.reward_label,
        stamps_required: old.stamps_required,
        min_purchase_amount: old.min_purchase_amount,
        currency: old.currency,
        receipt_ttl_seconds: old.receipt_ttl_seconds,
        receipts_window_start: old.receipts_window_start,
        receipts_this_window: old.receipts_this_window,
        total_cards: old.total_cards,
        total_stamps_issued: old.total_stamps_issued,
        total_vouchers_issued: old.total_vouchers_issued,
        total_redemptions: old.total_redemptions,
        bump: old.bump,
        terms_locked_until: now,
        open_cards: old.total_cards,
        rent_payer: ctx.accounts.relayer.key(),
        open_vouchers: old
            .total_vouchers_issued
            .saturating_sub(old.total_redemptions as u64)
            .min(u32::MAX as u64) as u32,
    };

    let mut data = info.try_borrow_mut_data()?;
    data[..8].copy_from_slice(&Business::DISCRIMINATOR[..]);
    let mut cursor = &mut data[8..];
    updated.serialize(&mut cursor).map_err(|_| error!(ErrorCode::NotABusiness))?;
    Ok(())
}

#[derive(Accounts)]
pub struct MigrateBusiness<'info> {
    /// CHECK: An old, short Business. Anchor cannot type it until this instruction has grown it, so its
    /// discriminator, its seeds and its length are all checked by hand in the handler.
    #[account(mut)]
    pub business: UncheckedAccount<'info>,

    /// Pays for the extra bytes and becomes the shop's recorded rent payer. Nothing on chain remembers
    /// who originally paid, so whoever migrates takes that on — which is why the relayer must run this
    /// over its own shops promptly.
    #[account(mut)]
    pub relayer: Signer<'info>,

    pub system_program: Program<'info, System>,
}
