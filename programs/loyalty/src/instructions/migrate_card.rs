use anchor_lang::prelude::*;
use anchor_lang::system_program::{transfer, Transfer};
use anchor_lang::Discriminator;
use crate::state::{CardNft, LoyaltyCard};
use crate::error::ErrorCode;
use crate::instructions::mint_card_nft::close_program_account;

/// Brings a card made before `rent_payer` existed up to the current shape, and hands back the rent of
/// the old per-NFT record at the same time.
///
/// Only needed for cards already on chain when this change shipped. A fresh deployment never calls it.
///
/// The card is taken unchecked and read by hand on purpose: an old card is 32 bytes shorter than the
/// current struct, so Anchor cannot deserialize one into `LoyaltyCard` at all — that is the very thing
/// this instruction exists to fix. Everything it relies on sits at a fixed offset in the old layout and
/// is verified below.
///
/// Who the rent goes back to: if the card still has one of the old `["card_nft", mint]` records, that
/// record names the wallet that really paid, and that is what gets written onto the card — nobody can
/// redirect it. If no record survives, nothing on chain remembers who paid, so the signer becomes the
/// recorded payer. The relayer should therefore migrate its own cards promptly: whoever gets there
/// first is who the rent returns to afterwards.

// Offsets inside the old (103-byte) card: 8 discriminator, 32 business, 32 customer, then the counters.
const OLD_CARD_LEN: usize = 103;
const BUSINESS_AT: usize = 8;
const CUSTOMER_AT: usize = 40;
const NFT_CYCLE_AT: usize = 97;
const BUMP_AT: usize = 102;

fn pubkey_at(data: &[u8], at: usize) -> Pubkey {
    Pubkey::new_from_array(data[at..at + 32].try_into().unwrap())
}

pub fn migrate_card_handler(ctx: Context<MigrateCard>) -> Result<()> {
    let card_info = ctx.accounts.card.to_account_info();
    let new_len = 8 + LoyaltyCard::INIT_SPACE;

    require_keys_eq!(*card_info.owner, crate::ID, ErrorCode::NotACard);
    {
        let data = card_info.try_borrow_data()?;
        require!(data.len() >= OLD_CARD_LEN, ErrorCode::NotACard);
        require!(data[..8] == LoyaltyCard::DISCRIMINATOR[..], ErrorCode::NotACard);
        require!(data.len() == OLD_CARD_LEN, ErrorCode::CardAlreadyMigrated);

        // Prove this really is the card its own contents claim to be, which is what the seeds would
        // have checked had Anchor been able to load it.
        let business = pubkey_at(&data, BUSINESS_AT);
        let customer = pubkey_at(&data, CUSTOMER_AT);
        let bump = data[BUMP_AT];
        let expected = Pubkey::create_program_address(
            &[b"card", business.as_ref(), customer.as_ref(), &[bump]],
            &crate::ID,
        )
        .map_err(|_| error!(ErrorCode::NotACard))?;
        require_keys_eq!(expected, card_info.key(), ErrorCode::NotACard);

        // The record's address is fixed by the card's current NFT mint, so check the caller passed the
        // right pair rather than some other card's record.
        let cycle = u32::from_le_bytes(data[NFT_CYCLE_AT..NFT_CYCLE_AT + 4].try_into().unwrap());
        let (mint, _) = Pubkey::find_program_address(
            &[b"card_mint", card_info.key().as_ref(), &cycle.to_le_bytes()],
            &crate::ID,
        );
        require_keys_eq!(ctx.accounts.card_mint.key(), mint, ErrorCode::NotACard);
        let (record, _) = Pubkey::find_program_address(&[b"card_nft", mint.as_ref()], &crate::ID);
        require_keys_eq!(ctx.accounts.record.key(), record, ErrorCode::NotACard);
    }

    // Grow the account and keep it rent exempt.
    let rent = Rent::get()?;
    let extra = rent.minimum_balance(new_len).saturating_sub(card_info.lamports());
    if extra > 0 {
        transfer(
            CpiContext::new(
                ctx.accounts.system_program.key(),
                Transfer { from: ctx.accounts.relayer.to_account_info(), to: card_info.clone() },
            ),
            extra,
        )?;
    }
    // `resize` zeroes the bytes it adds, so the new field starts blank before it is written below.
    card_info.resize(new_len)?;

    // Prefer the truth the old record holds over the word of whoever is calling.
    let record = ctx.accounts.record.to_account_info();
    let payer = if *record.owner == crate::ID && !record.data_is_empty() {
        let recorded = CardNft::try_deserialize(&mut &record.try_borrow_data()?[..])?.rent_payer;
        require_keys_eq!(ctx.accounts.rent_payer.key(), recorded, ErrorCode::WrongRentPayer);
        close_program_account(&record, &ctx.accounts.rent_payer.to_account_info())?;
        recorded
    } else {
        ctx.accounts.relayer.key()
    };

    let mut data = card_info.try_borrow_mut_data()?;
    data[OLD_CARD_LEN..new_len].copy_from_slice(payer.as_ref());
    Ok(())
}

#[derive(Accounts)]
pub struct MigrateCard<'info> {
    /// CHECK: An old, short LoyaltyCard. Anchor cannot type it until this instruction has grown it, so
    /// its discriminator, its seeds and its length are all checked by hand in the handler.
    #[account(mut)]
    pub card: UncheckedAccount<'info>,

    /// CHECK: The old `["card_nft", mint]` record for this card's current NFT, if one is still there.
    /// Checked against the card's own cycle in the handler. May be empty.
    #[account(mut)]
    pub record: UncheckedAccount<'info>,

    /// CHECK: The card's current NFT mint, used only to derive the record's address. Checked in the
    /// handler against the card's own cycle.
    pub card_mint: UncheckedAccount<'info>,

    /// Receives the old record's rent. Must be the wallet that record names.
    #[account(mut)]
    pub rent_payer: SystemAccount<'info>,

    /// Pays for the card's extra bytes, and becomes the card's recorded payer when no record survives
    /// to say otherwise.
    #[account(mut)]
    pub relayer: Signer<'info>,

    pub system_program: Program<'info, System>,
}
