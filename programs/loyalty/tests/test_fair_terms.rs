//! Counting rewards properly, committing to an offer for a period, and closing a shop that has
//! finished trading.

mod common;
use common::assert_fails_with;

use {
    anchor_lang::{
        prelude::Pubkey,
        solana_program::{instruction::Instruction, system_program},
        AccountDeserialize, InstructionData, Space, ToAccountMetas,
    },
    litesvm::{types::TransactionResult, LiteSVM},
    solana_clock::Clock,
    solana_keccak_hasher as keccak,
    solana_keypair::Keypair,
    solana_message::{Message, VersionedMessage},
    solana_signer::Signer,
    solana_transaction::versioned::VersionedTransaction,
};

const DAY: i64 = 24 * 60 * 60;
const TOKEN_2022: Pubkey = anchor_spl::token_interface::spl_token_2022::ID;

fn now_of(svm: &LiteSVM) -> i64 { svm.get_sysvar::<Clock>().unix_timestamp }
fn warp(svm: &mut LiteSVM, seconds: i64) {
    let mut c = svm.get_sysvar::<Clock>();
    c.unix_timestamp += seconds;
    svm.set_sysvar::<Clock>(&c);
}
fn pda(seeds: &[&[u8]], id: &Pubkey) -> Pubkey { Pubkey::find_program_address(seeds, id).0 }
fn ata(owner: Pubkey, mint: Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[owner.as_ref(), TOKEN_2022.as_ref(), mint.as_ref()],
        &anchor_spl::associated_token::ID).0
}
fn send(svm: &mut LiteSVM, ixs: &[Instruction], payer: &Keypair, signers: &[&Keypair]) -> TransactionResult {
    svm.expire_blockhash();
    let bh = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(ixs, Some(&payer.pubkey()), &bh);
    svm.send_transaction(VersionedTransaction::try_new(VersionedMessage::Legacy(msg), signers).unwrap())
}

struct World { owner: Keypair, customer: Keypair, relayer: Keypair, business: Pubkey, card: Pubkey }

fn setup(svm: &mut LiteSVM, id: Pubkey, stamps: u8, locked_until: i64) -> World {
    let (owner, customer, relayer) = (Keypair::new(), Keypair::new(), Keypair::new());
    svm.add_program(id, include_bytes!("../../../target/deploy/loyalty.so")).unwrap();
    for w in [&owner, &customer, &relayer] { svm.airdrop(&w.pubkey(), 10_000_000_000).unwrap(); }
    let business = pda(&[b"business", owner.pubkey().as_ref()], &id);
    let ix = Instruction::new_with_bytes(id,
        &loyalty::instruction::RegisterBusiness {
            name: "Coffee Corner".into(), category: "cafe".into(), reward_label: "Free coffee".into(),
            stamps_required: stamps, min_purchase_amount: 100_000, currency: "PKR".into(),
            receipt_ttl_seconds: 300, terms_locked_until: locked_until,
        }.data(),
        loyalty::accounts::RegisterBusiness { business, authority: owner.pubkey(),
            relayer: relayer.pubkey(), system_program: system_program::ID }.to_account_metas(None));
    assert!(send(svm, &[ix], &relayer, &[&owner, &relayer]).is_ok(), "setup register");
    let card = pda(&[b"card", business.as_ref(), customer.pubkey().as_ref()], &id);
    World { owner, customer, relayer, business, card }
}

fn stamp(svm: &mut LiteSVM, id: Pubkey, w: &World, b: u8) -> TransactionResult {
    let secret = [b; 32];
    let hash = keccak::hash(secret.as_ref()).to_bytes();
    let receipt = pda(&[b"receipt", w.business.as_ref(), hash.as_ref()], &id);
    let issue = Instruction::new_with_bytes(id,
        &loyalty::instruction::IssueReceipt { secret_hash: hash, amount_band: 1 }.data(),
        loyalty::accounts::IssueReceipt { business: w.business, receipt, authority: w.owner.pubkey(),
            relayer: w.relayer.pubkey(), system_program: system_program::ID }.to_account_metas(None));
    let claim = Instruction::new_with_bytes(id,
        &loyalty::instruction::ClaimReceipt { secret }.data(),
        loyalty::accounts::ClaimReceipt { business: w.business, receipt, card: w.card,
            customer: w.customer.pubkey(), relayer: w.relayer.pubkey(),
            rent_payer: w.relayer.pubkey(), system_program: system_program::ID }.to_account_metas(None));
    let r = send(svm, &[issue, claim], &w.relayer, &[&w.owner, &w.customer, &w.relayer]);
    warp(svm, 61);
    r
}

fn cash_in(svm: &mut LiteSVM, id: Pubkey, w: &World, voucher_id: u64) -> TransactionResult {
    let bytes = voucher_id.to_le_bytes();
    let voucher = pda(&[b"voucher", w.business.as_ref(), &bytes], &id);
    let mint = pda(&[b"voucher_mint", w.business.as_ref(), &bytes], &id);
    let card = loyalty::LoyaltyCard::try_deserialize(
        &mut svm.get_account(&w.card).unwrap().data.as_slice()).unwrap();
    let card_mint = pda(&[b"card_mint", w.card.as_ref(), &card.nft_cycle.to_le_bytes()], &id);
    let ix = Instruction::new_with_bytes(id,
        &loyalty::instruction::MintVoucher { voucher_id, uri: String::new() }.data(),
        loyalty::accounts::MintVoucher { business: w.business, card: w.card, voucher, mint,
            customer_token: ata(w.customer.pubkey(), mint), card_mint,
            card_token: ata(w.customer.pubkey(), card_mint), card_rent_payer: w.relayer.pubkey(),
            customer: w.customer.pubkey(), relayer: w.relayer.pubkey(), token_program: TOKEN_2022,
            associated_token_program: anchor_spl::associated_token::ID,
            system_program: system_program::ID }.to_account_metas(None));
    send(svm, &[ix], &w.relayer, &[&w.customer, &w.relayer])
}

fn edit(svm: &mut LiteSVM, id: Pubkey, w: &World, stamps: u8, reward: &str, locked_until: i64) -> TransactionResult {
    let ix = Instruction::new_with_bytes(id,
        &loyalty::instruction::UpdateBusinessConfig {
            reward_label: reward.into(), stamps_required: stamps,
            min_purchase_amount: 100_000, receipt_ttl_seconds: 300, terms_locked_until: locked_until,
        }.data(),
        loyalty::accounts::UpdateBusinessConfig { business: w.business, authority: w.owner.pubkey() }
            .to_account_metas(None));
    send(svm, &[ix], &w.relayer, &[&w.owner, &w.relayer])
}

fn card_of(svm: &LiteSVM, card: Pubkey) -> loyalty::LoyaltyCard {
    loyalty::LoyaltyCard::try_deserialize(&mut svm.get_account(&card).unwrap().data.as_slice()).unwrap()
}
fn shop_of(svm: &LiteSVM, business: Pubkey) -> loyalty::Business {
    loyalty::Business::try_deserialize(&mut svm.get_account(&business).unwrap().data.as_slice()).unwrap()
}

// --- rewards counted, and terms refreshed at cash-in ---

#[test]
fn test_a_finished_card_takes_on_the_shops_current_terms_for_its_next_round() {
    let id = loyalty::id();
    let mut svm = LiteSVM::new();
    let w = setup(&mut svm, id, 2, 0);

    assert!(stamp(&mut svm, id, &w, 1).is_ok());
    assert!(stamp(&mut svm, id, &w, 2).is_ok());
    assert_eq!(card_of(&svm, w.card).stamps_required_snapshot, 2);
    assert_eq!(card_of(&svm, w.card).rewards_earned, 0);

    // The shop puts its price up while this card is full but not yet cashed in.
    assert!(edit(&mut svm, id, &w, 5, "Free coffee", 0).is_ok());
    assert_eq!(card_of(&svm, w.card).stamps_required_snapshot, 2,
        "the card in hand still finishes at the price it was started at");

    assert!(cash_in(&mut svm, id, &w, 0).is_ok(), "and it does finish");
    let after = card_of(&svm, w.card);
    assert_eq!(after.rewards_earned, 1, "the reward is counted on the card itself");
    assert_eq!(after.stamps_required_snapshot, 5,
        "the NEXT round uses the shop's current price — this is what never used to happen");
    assert_eq!(after.stamps, 0);
}

#[test]
fn test_rewards_earned_counts_up_and_survives_a_change_of_terms() {
    let id = loyalty::id();
    let mut svm = LiteSVM::new();
    let w = setup(&mut svm, id, 1, 0);

    assert!(stamp(&mut svm, id, &w, 1).is_ok());
    assert!(cash_in(&mut svm, id, &w, 0).is_ok());
    assert_eq!(card_of(&svm, w.card).rewards_earned, 1);

    assert!(edit(&mut svm, id, &w, 3, "Free coffee", 0).is_ok());
    for b in 2..5u8 { assert!(stamp(&mut svm, id, &w, b).is_ok()); }
    assert!(cash_in(&mut svm, id, &w, 1).is_ok());

    let c = card_of(&svm, w.card);
    assert_eq!(c.rewards_earned, 2, "two rewards, earned under two different prices");
    // The old derivation would now be wrong: (lifetime 4 - stamps 0) / snapshot 3 = 1, not 2.
    let derived = (c.lifetime_stamps - c.stamps as u32) / c.stamps_required_snapshot as u32;
    assert_ne!(derived, c.rewards_earned,
        "this is exactly the case the old (lifetime - stamps) / snapshot sum got wrong");
}

// --- committing to an offer ---

#[test]
fn test_a_shop_cannot_change_its_reward_while_it_is_committed() {
    let id = loyalty::id();
    let mut svm = LiteSVM::new();
    let w = setup(&mut svm, id, 5, 0);
    let until = now_of(&svm) + 120 * DAY;

    assert!(edit(&mut svm, id, &w, 5, "Free pizza", until).is_ok(), "the shop commits to free pizza");
    assert_eq!(shop_of(&svm, w.business).terms_locked_until, until);

    assert_fails_with(edit(&mut svm, id, &w, 5, "Free salad", until), "RewardTermsLocked");
    assert_fails_with(edit(&mut svm, id, &w, 9, "Free pizza", until), "RewardTermsLocked");
    assert_eq!(shop_of(&svm, w.business).reward_label, "Free pizza", "nothing moved");

    warp(&mut svm, 120 * DAY + 1);
    assert!(edit(&mut svm, id, &w, 9, "Free salad", 0).is_ok(), "once the offer is over it can change");
    assert_eq!(shop_of(&svm, w.business).reward_label, "Free salad");
}

#[test]
fn test_an_offer_cannot_be_promised_for_more_than_a_year_or_into_the_past() {
    let id = loyalty::id();
    let mut svm = LiteSVM::new();
    let w = setup(&mut svm, id, 5, 0);
    // Move off zero first: zero is the "no commitment at all" value, not a date in the past.
    warp(&mut svm, 30 * DAY);
    let now = now_of(&svm);

    assert_fails_with(edit(&mut svm, id, &w, 5, "Free pizza", now + 400 * DAY), "TermsLockTooLong");
    warp(&mut svm, 10 * DAY);
    assert_fails_with(edit(&mut svm, id, &w, 5, "Free pizza", now), "TermsLockInThePast");
    let later = now_of(&svm) + 300 * DAY;
    assert!(edit(&mut svm, id, &w, 5, "Free pizza", later).is_ok());
}

#[test]
fn test_an_offer_can_be_wound_down_early_but_only_with_two_weeks_notice() {
    let id = loyalty::id();
    let mut svm = LiteSVM::new();
    let w = setup(&mut svm, id, 5, 0);
    let long = now_of(&svm) + 300 * DAY;
    assert!(edit(&mut svm, id, &w, 5, "Free pizza", long).is_ok());

    // While committed, the terms themselves are frozen, so winding down happens after it lapses...
    warp(&mut svm, 300 * DAY + 1);
    let now = now_of(&svm);
    assert!(edit(&mut svm, id, &w, 5, "Free pizza", now + 200 * DAY).is_ok(), "re-committed for 200 days");

    // ...and a shop that has lapsed can still not yank a fresh promise forward to tomorrow.
    warp(&mut svm, 200 * DAY + 1);
    let now = now_of(&svm);
    assert!(edit(&mut svm, id, &w, 5, "Free pizza", now + 100 * DAY).is_ok());
    warp(&mut svm, 100 * DAY + 1);

    // A shop with no promise on the books may set any valid date, including a near one.
    let now = now_of(&svm);
    assert!(edit(&mut svm, id, &w, 5, "Free pizza", now + DAY).is_ok(),
        "with nothing promised, a short offer is the shop's own business");
}

// --- closing a shop ---

#[test]
fn test_a_shop_that_has_finished_trading_closes_and_its_rent_comes_back() {
    let id = loyalty::id();
    let mut svm = LiteSVM::new();
    let w = setup(&mut svm, id, 1, 0);

    // One customer collects, cashes in and redeems; then their card is closed as dead.
    assert!(stamp(&mut svm, id, &w, 1).is_ok());
    assert!(cash_in(&mut svm, id, &w, 0).is_ok());
    assert_eq!(shop_of(&svm, w.business).open_cards, 1);
    assert_eq!(shop_of(&svm, w.business).open_vouchers, 1);

    // The voucher expires and is cleaned up, so nothing is owed.
    let bytes = 0u64.to_le_bytes();
    let voucher = pda(&[b"voucher", w.business.as_ref(), &bytes], &id);
    let vmint = pda(&[b"voucher_mint", w.business.as_ref(), &bytes], &id);
    warp(&mut svm, 91 * DAY);
    let close_v = Instruction::new_with_bytes(id,
        &loyalty::instruction::CloseExpiredVoucher {}.data(),
        loyalty::accounts::CloseExpiredVoucher { voucher, business: w.business, mint: vmint,
            holder_token: ata(w.customer.pubkey(), vmint), rent_payer: w.relayer.pubkey(),
            token_program: TOKEN_2022 }.to_account_metas(None));
    assert!(send(&mut svm, &[close_v], &w.relayer, &[&w.relayer]).is_ok(), "expired voucher cleaned up");
    assert_eq!(shop_of(&svm, w.business).open_vouchers, 0, "nothing owed any more");

    // Still a card open, so the shop stays.
    assert_fails_with(close_shop(&mut svm, id, &w), "ShopStillHasOpenCards");

    // A year passes with no stamp and the dead card is closed.
    warp(&mut svm, 366 * DAY);
    let card_mint = pda(&[b"card_mint", w.card.as_ref(), &card_of(&svm, w.card).nft_cycle.to_le_bytes()], &id);
    let close_c = Instruction::new_with_bytes(id,
        &loyalty::instruction::CloseDeadCard {}.data(),
        loyalty::accounts::CloseDeadCard { card: w.card, business: w.business, card_mint,
            rent_payer: w.relayer.pubkey() }.to_account_metas(None));
    assert!(send(&mut svm, &[close_c], &w.relayer, &[&w.relayer]).is_ok(), "dead card closed");
    assert_eq!(shop_of(&svm, w.business).open_cards, 0, "the shop's live card count came down");

    let held = svm.get_balance(&w.business).unwrap();
    let before = svm.get_balance(&w.relayer.pubkey()).unwrap();
    assert!(close_shop(&mut svm, id, &w).is_ok(), "now the shop itself can close");
    assert!(svm.get_account(&w.business).is_none_or(|a| a.data.is_empty()), "the shop is gone");
    assert_eq!(svm.get_balance(&w.relayer.pubkey()).unwrap() - before, held,
        "every lamport of the shop's rent went back to the relayer");
}

fn close_shop(svm: &mut LiteSVM, id: Pubkey, w: &World) -> TransactionResult {
    let ix = Instruction::new_with_bytes(id,
        &loyalty::instruction::CloseBusiness {}.data(),
        loyalty::accounts::CloseBusiness { business: w.business, rent_payer: w.relayer.pubkey() }
            .to_account_metas(None));
    let stranger = Keypair::new();
    svm.airdrop(&stranger.pubkey(), 1_000_000_000).unwrap();
    send(svm, &[ix], &stranger, &[&stranger])
}

#[test]
fn test_a_shop_cannot_close_in_the_middle_of_its_own_offer() {
    let id = loyalty::id();
    let mut svm = LiteSVM::new();
    let w = setup(&mut svm, id, 1, 0);
    let until = now_of(&svm) + 100 * DAY;
    assert!(edit(&mut svm, id, &w, 1, "Free pizza", until).is_ok());

    assert_fails_with(close_shop(&mut svm, id, &w), "ShopOfferStillRunning");
    warp(&mut svm, 100 * DAY + 1);
    assert!(close_shop(&mut svm, id, &w).is_ok(), "once the offer is over, and nothing is owed, it closes");
}

#[test]
fn test_a_shop_cannot_close_while_a_reward_is_still_owed() {
    let id = loyalty::id();
    let mut svm = LiteSVM::new();
    let w = setup(&mut svm, id, 1, 0);
    assert!(stamp(&mut svm, id, &w, 1).is_ok());
    assert!(cash_in(&mut svm, id, &w, 0).is_ok());

    // Close the card out of the way, leaving only the unredeemed voucher.
    warp(&mut svm, 366 * DAY);
    let card_mint = pda(&[b"card_mint", w.card.as_ref(), &card_of(&svm, w.card).nft_cycle.to_le_bytes()], &id);
    let close_c = Instruction::new_with_bytes(id,
        &loyalty::instruction::CloseDeadCard {}.data(),
        loyalty::accounts::CloseDeadCard { card: w.card, business: w.business, card_mint,
            rent_payer: w.relayer.pubkey() }.to_account_metas(None));
    assert!(send(&mut svm, &[close_c], &w.relayer, &[&w.relayer]).is_ok());
    assert_eq!(shop_of(&svm, w.business).open_cards, 0);
    assert_eq!(shop_of(&svm, w.business).open_vouchers, 1, "a customer still holds a reward");

    assert_fails_with(close_shop(&mut svm, id, &w), "ShopStillOwesVouchers");
}

#[test]
fn test_a_shops_rent_cannot_be_sent_to_someone_else() {
    let id = loyalty::id();
    let mut svm = LiteSVM::new();
    let w = setup(&mut svm, id, 1, 0);
    let ix = Instruction::new_with_bytes(id,
        &loyalty::instruction::CloseBusiness {}.data(),
        loyalty::accounts::CloseBusiness { business: w.business, rent_payer: w.owner.pubkey() }
            .to_account_metas(None));
    assert_fails_with(send(&mut svm, &[ix], &w.owner, &[&w.owner]), "ConstraintHasOne");
    assert!(svm.get_account(&w.business).is_some(), "the shop was left alone");
}

#[test]
fn test_a_new_card_is_counted_against_the_shop() {
    let id = loyalty::id();
    let mut svm = LiteSVM::new();
    let w = setup(&mut svm, id, 5, 0);
    assert_eq!(shop_of(&svm, w.business).open_cards, 0);
    assert!(stamp(&mut svm, id, &w, 1).is_ok());
    assert_eq!(shop_of(&svm, w.business).open_cards, 1, "the first stamp opens a card");
    assert!(stamp(&mut svm, id, &w, 2).is_ok());
    assert_eq!(shop_of(&svm, w.business).open_cards, 1, "a second stamp on the same card opens nothing");
    assert_eq!(shop_of(&svm, w.business).total_cards, 1);
}

#[test]
fn test_the_new_fields_fit_the_space_reserved_for_them() {
    // Cheap guard: if a field is ever added without the account growing, this catches it before a
    // migration script does.
    assert_eq!(8 + loyalty::LoyaltyCard::INIT_SPACE, 139);
    assert_eq!(8 + loyalty::Business::INIT_SPACE, 241);
}

/// `card.stamps` is a u8 and claim_receipt increments it without a guard. What happens at 255?
#[test]
fn test_what_happens_to_a_card_at_the_stamp_ceiling() {
    let id = loyalty::id();
    let mut svm = LiteSVM::new();
    let w = setup(&mut svm, id, 100, 0);
    assert!(stamp(&mut svm, id, &w, 1).is_ok());

    // Jump the card to the top of the byte rather than claiming 255 times.
    let mut account = svm.get_account(&w.card).unwrap();
    account.data[72] = 255;
    svm.set_account(w.card, account).unwrap();

    let res = stamp(&mut svm, id, &w, 2);
    println!("stamp #256: {:?}", res.as_ref().map(|_| "OK".to_string()).map_err(|e| format!("{:?}", e.err)));
    assert!(res.is_err(), "it should fail rather than wrap around to zero");
    assert_eq!(card_of(&svm, w.card).stamps, 255, "and the card is unchanged");
}
