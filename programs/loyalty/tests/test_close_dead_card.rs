//! Closing a stamp card nobody is coming back to, and migrating the cards made before the card carried
//! its own rent payer.

mod common;
use common::assert_fails_with;

use {
    anchor_lang::{
        prelude::Pubkey,
        solana_program::{instruction::Instruction, system_program},
        AccountDeserialize, Discriminator, InstructionData, Space, ToAccountMetas,
    },
    litesvm::{types::TransactionResult, LiteSVM},
    solana_clock::Clock,
    solana_keccak_hasher as keccak,
    solana_keypair::Keypair,
    solana_message::{Message, VersionedMessage},
    solana_signer::Signer,
    solana_transaction::versioned::VersionedTransaction,
};

const ONE_YEAR: i64 = 365 * 24 * 60 * 60;
const NINETY_DAYS: i64 = 90 * 24 * 60 * 60;
const TOKEN_2022: Pubkey = anchor_spl::token_interface::spl_token_2022::ID;

fn warp(svm: &mut LiteSVM, seconds: i64) {
    let mut clock = svm.get_sysvar::<Clock>();
    clock.unix_timestamp += seconds;
    svm.set_sysvar::<Clock>(&clock);
}

fn send(svm: &mut LiteSVM, ixs: &[Instruction], payer: &Keypair, signers: &[&Keypair]) -> TransactionResult {
    svm.expire_blockhash();
    let bh = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(ixs, Some(&payer.pubkey()), &bh);
    svm.send_transaction(VersionedTransaction::try_new(VersionedMessage::Legacy(msg), signers).unwrap())
}

fn pda(seeds: &[&[u8]], program_id: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(seeds, program_id).0
}

fn ata(owner: Pubkey, mint: Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[owner.as_ref(), TOKEN_2022.as_ref(), mint.as_ref()],
        &anchor_spl::associated_token::ID,
    ).0
}

struct World {
    owner: Keypair,
    customer: Keypair,
    relayer: Keypair,
    business: Pubkey,
    card: Pubkey,
}

/// A shop, a customer, and one stamp — which also brings the card's first NFT.
fn setup(svm: &mut LiteSVM, program_id: Pubkey, stamps_required: u8) -> World {
    let owner = Keypair::new();
    let customer = Keypair::new();
    let relayer = Keypair::new();
    svm.add_program(program_id, include_bytes!("../../../target/deploy/loyalty.so")).unwrap();
    for who in [&owner, &customer, &relayer] {
        svm.airdrop(&who.pubkey(), 10_000_000_000).unwrap();
    }

    let business = pda(&[b"business", owner.pubkey().as_ref()], &program_id);
    let ix = Instruction::new_with_bytes(
        program_id,
        &loyalty::instruction::RegisterBusiness {
            name: "Coffee Corner".into(), category: "cafe".into(), reward_label: "Free coffee".into(),
            stamps_required, min_purchase_amount: 100_000, currency: "PKR".into(), receipt_ttl_seconds: 300,
        }.data(),
        loyalty::accounts::RegisterBusiness {
            business, authority: owner.pubkey(), relayer: relayer.pubkey(), system_program: system_program::ID,
        }.to_account_metas(None),
    );
    assert!(send(svm, &[ix], &relayer, &[&owner, &relayer]).is_ok(), "setup register");

    let card = pda(&[b"card", business.as_ref(), customer.pubkey().as_ref()], &program_id);
    World { owner, customer, relayer, business, card }
}

fn stamp(svm: &mut LiteSVM, program_id: Pubkey, w: &World, secret_byte: u8, cycle: u32) -> TransactionResult {
    let secret = [secret_byte; 32];
    let hash = keccak::hash(secret.as_ref()).to_bytes();
    let receipt = pda(&[b"receipt", w.business.as_ref(), hash.as_ref()], &program_id);
    let issue = Instruction::new_with_bytes(
        program_id,
        &loyalty::instruction::IssueReceipt { secret_hash: hash, amount_band: 1 }.data(),
        loyalty::accounts::IssueReceipt {
            business: w.business, receipt, authority: w.owner.pubkey(),
            relayer: w.relayer.pubkey(), system_program: system_program::ID,
        }.to_account_metas(None),
    );
    let claim = Instruction::new_with_bytes(
        program_id,
        &loyalty::instruction::ClaimReceipt { secret }.data(),
        loyalty::accounts::ClaimReceipt {
            business: w.business, receipt, card: w.card, customer: w.customer.pubkey(),
            relayer: w.relayer.pubkey(), rent_payer: w.relayer.pubkey(), system_program: system_program::ID,
        }.to_account_metas(None),
    );
    let mint = pda(&[b"card_mint", w.card.as_ref(), &cycle.to_le_bytes()], &program_id);
    let nft = Instruction::new_with_bytes(
        program_id,
        &loyalty::instruction::MintCardNft { uri: String::new() }.data(),
        loyalty::accounts::MintCardNft {
            business: w.business, card: w.card, mint,
            customer_token: ata(w.customer.pubkey(), mint),
            customer: w.customer.pubkey(), relayer: w.relayer.pubkey(),
            token_program: TOKEN_2022,
            associated_token_program: anchor_spl::associated_token::ID,
            system_program: system_program::ID,
        }.to_account_metas(None),
    );
    send(svm, &[issue, claim, nft], &w.relayer, &[&w.owner, &w.customer, &w.relayer])
}

fn stamp_only(svm: &mut LiteSVM, program_id: Pubkey, w: &World, secret_byte: u8) -> TransactionResult {
    let secret = [secret_byte; 32];
    let hash = keccak::hash(secret.as_ref()).to_bytes();
    let receipt = pda(&[b"receipt", w.business.as_ref(), hash.as_ref()], &program_id);
    let issue = Instruction::new_with_bytes(
        program_id,
        &loyalty::instruction::IssueReceipt { secret_hash: hash, amount_band: 1 }.data(),
        loyalty::accounts::IssueReceipt {
            business: w.business, receipt, authority: w.owner.pubkey(),
            relayer: w.relayer.pubkey(), system_program: system_program::ID,
        }.to_account_metas(None),
    );
    let claim = Instruction::new_with_bytes(
        program_id,
        &loyalty::instruction::ClaimReceipt { secret }.data(),
        loyalty::accounts::ClaimReceipt {
            business: w.business, receipt, card: w.card, customer: w.customer.pubkey(),
            relayer: w.relayer.pubkey(), rent_payer: w.relayer.pubkey(), system_program: system_program::ID,
        }.to_account_metas(None),
    );
    send(svm, &[issue, claim], &w.relayer, &[&w.owner, &w.customer, &w.relayer])
}

fn retire_nft(svm: &mut LiteSVM, program_id: Pubkey, w: &World, cycle: u32) -> TransactionResult {
    let mint = pda(&[b"card_mint", w.card.as_ref(), &cycle.to_le_bytes()], &program_id);
    let ix = Instruction::new_with_bytes(
        program_id,
        &loyalty::instruction::RetireIdleCardNft {}.data(),
        loyalty::accounts::RetireIdleCardNft {
            card: w.card, customer: w.customer.pubkey(), card_mint: mint,
            card_token: ata(w.customer.pubkey(), mint),
            rent_payer: w.relayer.pubkey(), token_program: TOKEN_2022,
        }.to_account_metas(None),
    );
    send(svm, &[ix], &w.relayer, &[&w.relayer])
}

fn close_dead(svm: &mut LiteSVM, program_id: Pubkey, w: &World, cycle: u32, rent_payer: Pubkey) -> TransactionResult {
    let ix = Instruction::new_with_bytes(
        program_id,
        &loyalty::instruction::CloseDeadCard {}.data(),
        loyalty::accounts::CloseDeadCard {
            card: w.card,
            card_mint: pda(&[b"card_mint", w.card.as_ref(), &cycle.to_le_bytes()], &program_id),
            rent_payer,
        }.to_account_metas(None),
    );
    // Deliberately paid by a stranger: closing is permissionless.
    let stranger = Keypair::new();
    svm.airdrop(&stranger.pubkey(), 1_000_000_000).unwrap();
    send(svm, &[ix], &stranger, &[&stranger])
}

/// Spends the card's one stamp on a voucher, leaving it empty and burning its NFT.
fn cash_in(svm: &mut LiteSVM, program_id: Pubkey, w: &World, voucher_id: u64, cycle: u32) -> TransactionResult {
    let id = voucher_id.to_le_bytes();
    let voucher = pda(&[b"voucher", w.business.as_ref(), &id], &program_id);
    let mint = pda(&[b"voucher_mint", w.business.as_ref(), &id], &program_id);
    let card_mint = pda(&[b"card_mint", w.card.as_ref(), &cycle.to_le_bytes()], &program_id);
    let ix = Instruction::new_with_bytes(
        program_id,
        &loyalty::instruction::MintVoucher { voucher_id, uri: String::new() }.data(),
        loyalty::accounts::MintVoucher {
            business: w.business, card: w.card, voucher, mint,
            customer_token: ata(w.customer.pubkey(), mint),
            card_mint, card_token: ata(w.customer.pubkey(), card_mint),
            card_rent_payer: w.relayer.pubkey(),
            customer: w.customer.pubkey(), relayer: w.relayer.pubkey(),
            token_program: TOKEN_2022,
            associated_token_program: anchor_spl::associated_token::ID,
            system_program: system_program::ID,
        }.to_account_metas(None),
    );
    send(svm, &[ix], &w.relayer, &[&w.customer, &w.relayer])
}

#[test]
fn test_a_dead_card_is_closed_and_all_its_rent_goes_back_to_the_relayer() {
    let program_id = loyalty::id();
    let mut svm = LiteSVM::new();
    let w = setup(&mut svm, program_id, 1);
    assert!(stamp(&mut svm, program_id, &w, 1, 0).is_ok());

    // Spend the stamp, which also burns the NFT: an empty card with no NFT.
    assert!(cash_in(&mut svm, program_id, &w, 0, 0).is_ok());
    warp(&mut svm, ONE_YEAR);

    let held = svm.get_balance(&w.card).unwrap();
    let before = svm.get_balance(&w.relayer.pubkey()).unwrap();
    let res = close_dead(&mut svm, program_id, &w, 1, w.relayer.pubkey());
    assert!(res.is_ok(), "a dead card should close: {:?}", res);

    assert!(svm.get_account(&w.card).is_none_or(|a| a.data.is_empty()), "the card account is gone");
    assert_eq!(
        svm.get_balance(&w.relayer.pubkey()).unwrap() - before, held,
        "every lamport of the card's rent went back to the relayer"
    );
}

#[test]
fn test_a_card_with_stamps_on_it_is_never_closed() {
    let program_id = loyalty::id();
    let mut svm = LiteSVM::new();
    let w = setup(&mut svm, program_id, 10);
    assert!(stamp(&mut svm, program_id, &w, 1, 0).is_ok());
    warp(&mut svm, ONE_YEAR);
    assert!(retire_nft(&mut svm, program_id, &w, 0).is_ok(), "its NFT can still be recycled");

    let res = close_dead(&mut svm, program_id, &w, 1, w.relayer.pubkey());
    assert_fails_with(res, "CardNotEmpty");
    assert!(svm.get_account(&w.card).is_some(), "somebody's unfinished card must survive");
}

#[test]
fn test_a_card_used_within_the_year_is_not_dead() {
    let program_id = loyalty::id();
    let mut svm = LiteSVM::new();
    let w = setup(&mut svm, program_id, 1);
    assert!(stamp(&mut svm, program_id, &w, 1, 0).is_ok());
    assert!(cash_in(&mut svm, program_id, &w, 0, 0).is_ok());

    warp(&mut svm, ONE_YEAR - 60);
    assert_fails_with(close_dead(&mut svm, program_id, &w, 1, w.relayer.pubkey()), "CardNotDead");

    warp(&mut svm, 120);
    assert!(close_dead(&mut svm, program_id, &w, 1, w.relayer.pubkey()).is_ok(), "a minute past a year, it goes");
}

/// The subtle one. Closing resets the card's NFT cycle to zero, so the next NFT it ever mints lands on
/// the address the very first one used. If that mint were still alive the card would be stuck.
#[test]
fn test_a_card_whose_nft_is_still_alive_is_not_closed() {
    let program_id = loyalty::id();
    let mut svm = LiteSVM::new();
    let w = setup(&mut svm, program_id, 10);
    assert!(stamp(&mut svm, program_id, &w, 1, 0).is_ok());

    // Empty the card by hand, so only the live NFT stands in the way.
    let mut account = svm.get_account(&w.card).unwrap();
    account.data[72] = 0; // stamps
    svm.set_account(w.card, account).unwrap();
    warp(&mut svm, ONE_YEAR);

    assert_fails_with(close_dead(&mut svm, program_id, &w, 0, w.relayer.pubkey()), "CardNftStillAlive");

    // Recycle the NFT first and it closes.
    assert!(retire_nft(&mut svm, program_id, &w, 0).is_ok());
    assert!(close_dead(&mut svm, program_id, &w, 1, w.relayer.pubkey()).is_ok(), "now it can go");
}

#[test]
fn test_a_dead_cards_rent_cannot_be_sent_to_someone_else() {
    let program_id = loyalty::id();
    let mut svm = LiteSVM::new();
    let w = setup(&mut svm, program_id, 1);
    assert!(stamp(&mut svm, program_id, &w, 1, 0).is_ok());
    assert!(cash_in(&mut svm, program_id, &w, 0, 0).is_ok());
    warp(&mut svm, ONE_YEAR);

    // The shop owner tries to collect rent the relayer paid.
    let res = close_dead(&mut svm, program_id, &w, 1, w.owner.pubkey());
    assert_fails_with(res, "ConstraintHasOne");
    assert!(svm.get_account(&w.card).is_some(), "nothing was closed");
}

#[test]
fn test_closing_a_dead_card_lets_the_customer_start_over() {
    let program_id = loyalty::id();
    let mut svm = LiteSVM::new();
    let w = setup(&mut svm, program_id, 1);
    assert!(stamp(&mut svm, program_id, &w, 1, 0).is_ok());
    assert!(cash_in(&mut svm, program_id, &w, 0, 0).is_ok());
    warp(&mut svm, ONE_YEAR);
    assert!(close_dead(&mut svm, program_id, &w, 1, w.relayer.pubkey()).is_ok());

    // A returning customer gets a brand new card, back at cycle zero, with a fresh NFT at the address
    // the first one used and gave up.
    let res = stamp(&mut svm, program_id, &w, 2, 0);
    assert!(res.is_ok(), "a customer who comes back can start again: {:?}", res);
    let card = loyalty::LoyaltyCard::try_deserialize(
        &mut svm.get_account(&w.card).unwrap().data.as_slice()).unwrap();
    assert_eq!(card.stamps, 1, "their new card starts at one stamp");
    assert_eq!(card.nft_cycle, 0, "and back at the first NFT cycle");
    assert_eq!(card.rent_payer, w.relayer.pubkey(), "the relayer is recorded as paying for it again");
}

// --- migrating the cards that existed before the card carried its payer ---

/// Shrinks a card back to its old 103-byte shape, as if it had been made by the previous program.
fn make_card_old(svm: &mut LiteSVM, card: Pubkey) {
    let mut account = svm.get_account(&card).unwrap();
    account.data.truncate(103);
    svm.set_account(card, account).unwrap();
}

fn migrate(svm: &mut LiteSVM, program_id: Pubkey, w: &World, cycle: u32, rent_payer: Pubkey, signer: &Keypair) -> TransactionResult {
    let mint = pda(&[b"card_mint", w.card.as_ref(), &cycle.to_le_bytes()], &program_id);
    let ix = Instruction::new_with_bytes(
        program_id,
        &loyalty::instruction::MigrateCard {}.data(),
        loyalty::accounts::MigrateCard {
            card: w.card,
            record: pda(&[b"card_nft", mint.as_ref()], &program_id),
            card_mint: mint,
            rent_payer,
            relayer: signer.pubkey(),
            system_program: system_program::ID,
        }.to_account_metas(None),
    );
    send(svm, &[ix], signer, &[signer])
}

#[test]
fn test_an_old_card_is_grown_and_given_its_payer() {
    let program_id = loyalty::id();
    let mut svm = LiteSVM::new();
    let w = setup(&mut svm, program_id, 10);
    assert!(stamp(&mut svm, program_id, &w, 1, 0).is_ok());
    make_card_old(&mut svm, w.card);
    assert_eq!(svm.get_account(&w.card).unwrap().data.len(), 103, "it is an old card now");

    let res = migrate(&mut svm, program_id, &w, 0, w.relayer.pubkey(), &w.relayer);
    assert!(res.is_ok(), "an old card should migrate: {:?}", res);

    let account = svm.get_account(&w.card).unwrap();
    assert_eq!(account.data.len(), 8 + loyalty::LoyaltyCard::INIT_SPACE, "it is the current shape");
    assert!(account.lamports >= svm.minimum_balance_for_rent_exemption(account.data.len()),
        "and still rent exempt at the bigger size");
    let card = loyalty::LoyaltyCard::try_deserialize(&mut account.data.as_slice()).unwrap();
    assert_eq!(card.rent_payer, w.relayer.pubkey());
    assert_eq!(card.stamps, 1, "nothing else about the card changed");
    assert_eq!(card.business, w.business);
}

#[test]
fn test_a_card_is_only_migrated_once() {
    let program_id = loyalty::id();
    let mut svm = LiteSVM::new();
    let w = setup(&mut svm, program_id, 10);
    assert!(stamp(&mut svm, program_id, &w, 1, 0).is_ok());
    make_card_old(&mut svm, w.card);
    assert!(migrate(&mut svm, program_id, &w, 0, w.relayer.pubkey(), &w.relayer).is_ok());
    assert_fails_with(migrate(&mut svm, program_id, &w, 0, w.relayer.pubkey(), &w.relayer), "CardAlreadyMigrated");
}

#[test]
fn test_migrate_refuses_an_account_that_is_not_a_card() {
    let program_id = loyalty::id();
    let mut svm = LiteSVM::new();
    let w = setup(&mut svm, program_id, 10);
    assert!(stamp(&mut svm, program_id, &w, 1, 0).is_ok());

    // Point it at the business account instead: right owner, wrong discriminator.
    let mint = pda(&[b"card_mint", w.card.as_ref(), &0u32.to_le_bytes()], &program_id);
    let ix = Instruction::new_with_bytes(
        program_id,
        &loyalty::instruction::MigrateCard {}.data(),
        loyalty::accounts::MigrateCard {
            card: w.business,
            record: pda(&[b"card_nft", mint.as_ref()], &program_id),
            card_mint: mint,
            rent_payer: w.relayer.pubkey(),
            relayer: w.relayer.pubkey(),
            system_program: system_program::ID,
        }.to_account_metas(None),
    );
    assert_fails_with(send(&mut svm, &[ix], &w.relayer, &[&w.relayer]), "NotACard");
    assert_eq!(svm.get_account(&w.business).unwrap().data[..8], loyalty::Business::DISCRIMINATOR[..],
        "the business account was left alone");
}

#[test]
fn test_a_migrated_card_can_then_be_closed_when_it_dies() {
    let program_id = loyalty::id();
    let mut svm = LiteSVM::new();
    let w = setup(&mut svm, program_id, 1);
    assert!(stamp(&mut svm, program_id, &w, 1, 0).is_ok());
    assert!(cash_in(&mut svm, program_id, &w, 0, 0).is_ok());
    make_card_old(&mut svm, w.card);

    assert!(migrate(&mut svm, program_id, &w, 1, w.relayer.pubkey(), &w.relayer).is_ok());
    warp(&mut svm, ONE_YEAR);

    let held = svm.get_balance(&w.card).unwrap();
    let before = svm.get_balance(&w.relayer.pubkey()).unwrap();
    assert!(close_dead(&mut svm, program_id, &w, 1, w.relayer.pubkey()).is_ok(), "a migrated card closes too");
    assert_eq!(svm.get_balance(&w.relayer.pubkey()).unwrap() - before, held,
        "including the extra rent the migration put in");
}

#[test]
fn test_an_idle_nft_on_a_card_still_works_after_migration() {
    let program_id = loyalty::id();
    let mut svm = LiteSVM::new();
    let w = setup(&mut svm, program_id, 10);
    assert!(stamp(&mut svm, program_id, &w, 1, 0).is_ok());
    make_card_old(&mut svm, w.card);
    assert!(migrate(&mut svm, program_id, &w, 0, w.relayer.pubkey(), &w.relayer).is_ok());

    warp(&mut svm, NINETY_DAYS);
    let res = retire_nft(&mut svm, program_id, &w, 0);
    assert!(res.is_ok(), "the 90-day sweep still works on a migrated card: {:?}", res);
}

/// Proves the migration is not optional: until a card has been through it, the program cannot read it,
/// so ordinary use of that card fails. This is the whole reason the migration has to run immediately
/// after the upgrade rather than at leisure.
#[test]
fn test_an_unmigrated_card_cannot_be_used_at_all() {
    let program_id = loyalty::id();
    let mut svm = LiteSVM::new();
    let w = setup(&mut svm, program_id, 10);
    assert!(stamp(&mut svm, program_id, &w, 1, 0).is_ok());
    make_card_old(&mut svm, w.card);

    // A second stamp on an old card: the program has to load the card to add to it, and cannot.
    // Anchor answers AccountDidNotDeserialize (3003).
    let res = stamp_only(&mut svm, program_id, &w, 2);
    assert_fails_with(res, "AccountDidNotDeserialize");

    // After migrating, the same stamp works.
    assert!(migrate(&mut svm, program_id, &w, 0, w.relayer.pubkey(), &w.relayer).is_ok());
    warp(&mut svm, 61); // clear the stamp cooldown
    let res = stamp_only(&mut svm, program_id, &w, 3);
    assert!(res.is_ok(), "once migrated the card works again: {:?}", res);
}

/// A shop asking for zero stamps used to be allowed, and it was not harmless: mint_voucher's
/// `card.stamps >= card.stamps_required_snapshot` was satisfied by an empty card and took nothing away,
/// so one stamp bought reward after reward, each a real NFT the relayer paid rent for. Refused now, on
/// the way in and on the way through an edit.
#[test]
fn test_a_shop_cannot_ask_for_zero_stamps() {
    let program_id = loyalty::id();
    let mut svm = LiteSVM::new();
    let owner = Keypair::new();
    let relayer = Keypair::new();
    svm.add_program(program_id, include_bytes!("../../../target/deploy/loyalty.so")).unwrap();
    for who in [&owner, &relayer] { svm.airdrop(&who.pubkey(), 10_000_000_000).unwrap(); }
    let business = pda(&[b"business", owner.pubkey().as_ref()], &program_id);

    let register = |stamps: u8| Instruction::new_with_bytes(
        program_id,
        &loyalty::instruction::RegisterBusiness {
            name: "Coffee Corner".into(), category: "cafe".into(), reward_label: "Free coffee".into(),
            stamps_required: stamps, min_purchase_amount: 100_000, currency: "PKR".into(),
            receipt_ttl_seconds: 300,
        }.data(),
        loyalty::accounts::RegisterBusiness {
            business, authority: owner.pubkey(), relayer: relayer.pubkey(), system_program: system_program::ID,
        }.to_account_metas(None),
    );

    assert_fails_with(send(&mut svm, &[register(0)], &relayer, &[&owner, &relayer]), "StampsRequiredTooLow");
    assert!(svm.get_account(&business).is_none_or(|a| a.data.is_empty()), "nothing was created");

    // One stamp is the least a card may ask for, and that is allowed.
    assert!(send(&mut svm, &[register(1)], &relayer, &[&owner, &relayer]).is_ok(), "one stamp is fine");

    // And a shop cannot sneak to zero by editing afterwards.
    let edit = |stamps: u8| Instruction::new_with_bytes(
        program_id,
        &loyalty::instruction::UpdateBusinessConfig {
            name: "Coffee Corner".into(), category: "cafe".into(), reward_label: "Free coffee".into(),
            stamps_required: stamps, min_purchase_amount: 100_000, receipt_ttl_seconds: 300,
        }.data(),
        loyalty::accounts::UpdateBusinessConfig { business, authority: owner.pubkey() }.to_account_metas(None),
    );
    assert_fails_with(send(&mut svm, &[edit(0)], &relayer, &[&owner, &relayer]), "StampsRequiredTooLow");
    assert_fails_with(send(&mut svm, &[edit(101)], &relayer, &[&owner, &relayer]), "StampsRequiredTooHigh");
    assert!(send(&mut svm, &[edit(12)], &relayer, &[&owner, &relayer]).is_ok(), "a sensible change goes through");
}

/// A merchant editing their shop: everything the registration page asks for, including the name and
/// category, which could not be changed before.
#[test]
fn test_a_merchant_can_edit_every_setting_without_moving_the_goalposts() {
    let program_id = loyalty::id();
    let mut svm = LiteSVM::new();
    let w = setup(&mut svm, program_id, 8);

    // A customer is already seven stamps into an eight-stamp card.
    assert!(stamp(&mut svm, program_id, &w, 1, 0).is_ok());
    let mut account = svm.get_account(&w.card).unwrap();
    account.data[72] = 7;
    svm.set_account(w.card, account).unwrap();

    let edit = Instruction::new_with_bytes(
        program_id,
        &loyalty::instruction::UpdateBusinessConfig {
            name: "Blue Door Cafe".into(), category: "Restaurant".into(),
            reward_label: "Free karahi".into(), stamps_required: 20,
            min_purchase_amount: 999_000, receipt_ttl_seconds: 3600,
        }.data(),
        loyalty::accounts::UpdateBusinessConfig { business: w.business, authority: w.owner.pubkey() }
            .to_account_metas(None),
    );
    assert!(send(&mut svm, &[edit], &w.relayer, &[&w.owner, &w.relayer]).is_ok(), "the edit goes through");

    let b = loyalty::Business::try_deserialize(
        &mut svm.get_account(&w.business).unwrap().data.as_slice()).unwrap();
    assert_eq!(b.name, "Blue Door Cafe", "the name can be changed now");
    assert_eq!(b.category, "Restaurant", "and the category");
    assert_eq!(b.reward_label, "Free karahi");
    assert_eq!(b.stamps_required, 20);
    assert_eq!(b.min_purchase_amount, 999_000);
    assert_eq!(b.receipt_ttl_seconds, 3600);

    // The customer already collecting is untouched: their card keeps the eight it was opened under.
    let card = loyalty::LoyaltyCard::try_deserialize(
        &mut svm.get_account(&w.card).unwrap().data.as_slice()).unwrap();
    assert_eq!(card.stamps_required_snapshot, 8, "a shop cannot move the goalposts on a card in progress");
    assert_eq!(card.stamps, 7);
}

/// Only the shop's own wallet may change its settings.
#[test]
fn test_a_stranger_cannot_edit_someone_elses_shop() {
    let program_id = loyalty::id();
    let mut svm = LiteSVM::new();
    let w = setup(&mut svm, program_id, 8);
    let stranger = Keypair::new();
    svm.airdrop(&stranger.pubkey(), 1_000_000_000).unwrap();

    let ix = Instruction::new_with_bytes(
        program_id,
        &loyalty::instruction::UpdateBusinessConfig {
            name: "Stolen".into(), category: "cafe".into(), reward_label: "Free everything".into(),
            stamps_required: 1, min_purchase_amount: 0, receipt_ttl_seconds: 300,
        }.data(),
        loyalty::accounts::UpdateBusinessConfig { business: w.business, authority: stranger.pubkey() }
            .to_account_metas(None),
    );
    let res = send(&mut svm, &[ix], &stranger, &[&stranger]);
    assert!(res.is_err(), "a stranger must not be able to rewrite a shop's terms");
    let b = loyalty::Business::try_deserialize(
        &mut svm.get_account(&w.business).unwrap().data.as_slice()).unwrap();
    assert_eq!(b.name, "Coffee Corner", "the shop was left alone");
}
