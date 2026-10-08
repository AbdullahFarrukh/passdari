//! Passing stamps to a friend's card at the same shop.

mod common;
use common::assert_fails_with;

use {
    anchor_lang::{
        prelude::Pubkey,
        solana_program::{instruction::Instruction, system_program},
        AccountDeserialize, InstructionData, ToAccountMetas,
    },
    litesvm::{types::TransactionResult, LiteSVM},
    solana_clock::Clock,
    solana_keccak_hasher as keccak,
    solana_keypair::Keypair,
    solana_message::{Message, VersionedMessage},
    solana_signer::Signer,
    solana_transaction::versioned::VersionedTransaction,
};

const STAMPS_REQUIRED: u8 = 10;

fn register(
    svm: &mut LiteSVM,
    program_id: Pubkey,
    owner: &Keypair,
    relayer: &Keypair,
    business_pda: Pubkey,
    name: &str,
) {
    let ix = Instruction::new_with_bytes(
        program_id,
        &loyalty::instruction::RegisterBusiness {
            name: name.to_string(),
            category: "cafe".to_string(),
            reward_label: "Free coffee".to_string(),
            stamps_required: STAMPS_REQUIRED,
            min_purchase_amount: 100_000,
            currency: "PKR".to_string(),
            receipt_ttl_seconds: 300,
        }
        .data(),
        loyalty::accounts::RegisterBusiness {
            business: business_pda,
            authority: owner.pubkey(),
            relayer: relayer.pubkey(),
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    let bh = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(&[ix], Some(&relayer.pubkey()), &bh);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[owner, relayer]).unwrap();
    assert!(svm.send_transaction(tx).is_ok(), "setup registration should succeed");
}

fn warp(svm: &mut LiteSVM, seconds: i64) {
    let mut clock = svm.get_sysvar::<Clock>();
    clock.unix_timestamp += seconds;
    svm.set_sysvar::<Clock>(&clock);
}

fn card_pda(program_id: Pubkey, business_pda: Pubkey, customer: &Keypair) -> Pubkey {
    Pubkey::find_program_address(
        &[b"card", business_pda.as_ref(), customer.pubkey().as_ref()],
        &program_id,
    )
    .0
}

fn read_card(svm: &LiteSVM, pda: Pubkey) -> loyalty::LoyaltyCard {
    let account = svm.get_account(&pda).expect("card should exist");
    loyalty::LoyaltyCard::try_deserialize(&mut account.data.as_slice()).unwrap()
}

/// Gives one card one stamp, the long way round: the shop issues a receipt and the customer claims it.
/// Stamps can't be conjured in a test any more than in real life, which is rather the point.
fn add_stamp(
    svm: &mut LiteSVM,
    program_id: Pubkey,
    owner: &Keypair,
    relayer: &Keypair,
    business_pda: Pubkey,
    customer: &Keypair,
    secret_byte: u8,
) {
    let secret = [secret_byte; 32];
    let secret_hash = keccak::hash(secret.as_ref()).to_bytes();
    let (receipt_pda, _) =
        Pubkey::find_program_address(&[b"receipt", business_pda.as_ref(), secret_hash.as_ref()], &program_id);

    let issue = Instruction::new_with_bytes(
        program_id,
        &loyalty::instruction::IssueReceipt { secret_hash, amount_band: 1 }.data(),
        loyalty::accounts::IssueReceipt {
            business: business_pda,
            receipt: receipt_pda,
            authority: owner.pubkey(),
            relayer: relayer.pubkey(),
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    svm.expire_blockhash();
    let bh = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(&[issue], Some(&relayer.pubkey()), &bh);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[owner, relayer]).unwrap();
    assert!(svm.send_transaction(tx).is_ok(), "setup issue should succeed");

    let claim = Instruction::new_with_bytes(
        program_id,
        &loyalty::instruction::ClaimReceipt { secret }.data(),
        loyalty::accounts::ClaimReceipt {
            business: business_pda,
            receipt: receipt_pda,
            card: card_pda(program_id, business_pda, customer),
            customer: customer.pubkey(),
            relayer: relayer.pubkey(),
            rent_payer: relayer.pubkey(),
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    svm.expire_blockhash();
    let bh = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(&[claim], Some(&relayer.pubkey()), &bh);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[customer, relayer]).unwrap();
    assert!(svm.send_transaction(tx).is_ok(), "setup claim should succeed");

    warp(svm, 61); // clear the one-minute stamp cooldown for the next one
}

#[allow(clippy::too_many_arguments)]
fn transfer(
    svm: &mut LiteSVM,
    program_id: Pubkey,
    business_pda: Pubkey,
    from_card: Pubkey,
    to_card: Pubkey,
    sender: &Keypair,
    relayer: &Keypair,
    amount: u8,
) -> TransactionResult {
    let ix = Instruction::new_with_bytes(
        program_id,
        &loyalty::instruction::TransferStamps { amount }.data(),
        loyalty::accounts::TransferStamps {
            business: business_pda,
            from_card,
            to_card,
            customer: sender.pubkey(),
            relayer: relayer.pubkey(),
        }
        .to_account_metas(None),
    );
    svm.expire_blockhash();
    let bh = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(&[ix], Some(&relayer.pubkey()), &bh);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[sender, relayer]).unwrap();
    svm.send_transaction(tx)
}

struct Shop {
    owner: Keypair,
    business: Pubkey,
}

/// One shop, two customers with cards: Sara has `sara_stamps`, Hamza has `hamza_stamps`.
fn setup(svm: &mut LiteSVM, program_id: Pubkey, sara_stamps: u8, hamza_stamps: u8)
    -> (Shop, Keypair, Keypair, Keypair, Pubkey, Pubkey)
{
    let owner = Keypair::new();
    let sara = Keypair::new();
    let hamza = Keypair::new();
    let relayer = Keypair::new();
    svm.add_program(program_id, include_bytes!("../../../target/deploy/loyalty.so")).unwrap();
    for who in [&owner, &sara, &hamza, &relayer] {
        svm.airdrop(&who.pubkey(), 10_000_000_000).unwrap();
    }

    let (business, _) = Pubkey::find_program_address(&[b"business", owner.pubkey().as_ref()], &program_id);
    register(svm, program_id, &owner, &relayer, business, "Blue Door Cafe");

    let mut secret_byte = 1u8;
    for _ in 0..sara_stamps {
        add_stamp(svm, program_id, &owner, &relayer, business, &sara, secret_byte);
        secret_byte += 1;
    }
    for _ in 0..hamza_stamps {
        add_stamp(svm, program_id, &owner, &relayer, business, &hamza, secret_byte);
        secret_byte += 1;
    }

    let sara_card = card_pda(program_id, business, &sara);
    let hamza_card = card_pda(program_id, business, &hamza);
    (Shop { owner, business }, sara, hamza, relayer, sara_card, hamza_card)
}

#[test]
fn test_transfer_moves_stamps_between_cards_at_the_same_shop() {
    let program_id = loyalty::id();
    let mut svm = LiteSVM::new();
    let (shop, sara, _hamza, relayer, sara_card, hamza_card) = setup(&mut svm, program_id, 3, 7);

    let res = transfer(&mut svm, program_id, shop.business, sara_card, hamza_card, &sara, &relayer, 3);
    assert!(res.is_ok(), "a transfer between two cards at the same shop should succeed");

    let sara_after = read_card(&svm, sara_card);
    let hamza_after = read_card(&svm, hamza_card);
    assert_eq!(sara_after.stamps, 0, "the sender's stamps should be gone");
    assert_eq!(hamza_after.stamps, 10, "the receiver should now have a full card");
}

#[test]
fn test_transfer_creates_no_new_stamps() {
    let program_id = loyalty::id();
    let mut svm = LiteSVM::new();
    let (shop, sara, _hamza, relayer, sara_card, hamza_card) = setup(&mut svm, program_id, 4, 2);

    let before = read_card(&svm, sara_card).stamps + read_card(&svm, hamza_card).stamps;
    let business_before = {
        let acc = svm.get_account(&shop.business).unwrap();
        loyalty::Business::try_deserialize(&mut acc.data.as_slice()).unwrap()
    };

    assert!(transfer(&mut svm, program_id, shop.business, sara_card, hamza_card, &sara, &relayer, 3).is_ok());

    let after = read_card(&svm, sara_card).stamps + read_card(&svm, hamza_card).stamps;
    assert_eq!(before, after, "the two cards together should hold exactly as many stamps as before");

    let business_after = {
        let acc = svm.get_account(&shop.business).unwrap();
        loyalty::Business::try_deserialize(&mut acc.data.as_slice()).unwrap()
    };
    assert_eq!(
        business_before.total_stamps_issued, business_after.total_stamps_issued,
        "moving stamps is not issuing stamps: the shop's total must not move"
    );
}

/// The merchant's leaderboard reads `lifetime_stamps - stamps` as "rewards finished". If a transfer moved
/// only `stamps`, sending them away would look exactly like spending them on a reward, and anyone could
/// top the leaderboard by passing stamps around. Both figures have to move together.
#[test]
fn test_transfer_cannot_be_used_to_fake_finished_rewards() {
    let program_id = loyalty::id();
    let mut svm = LiteSVM::new();
    let (shop, sara, _hamza, relayer, sara_card, hamza_card) = setup(&mut svm, program_id, 10, 1);

    let sara_before = read_card(&svm, sara_card);
    let hamza_before = read_card(&svm, hamza_card);
    let rewards = |c: &loyalty::LoyaltyCard| (c.lifetime_stamps - c.stamps as u32) / STAMPS_REQUIRED as u32;
    assert_eq!(rewards(&sara_before), 0, "nobody has finished a card yet");
    assert_eq!(rewards(&hamza_before), 0, "nobody has finished a card yet");

    assert!(transfer(&mut svm, program_id, shop.business, sara_card, hamza_card, &sara, &relayer, 10).is_ok());

    let sara_after = read_card(&svm, sara_card);
    let hamza_after = read_card(&svm, hamza_card);
    assert_eq!(rewards(&sara_after), 0, "giving ten stamps away must not count as finishing a card");
    assert_eq!(rewards(&hamza_after), 0, "receiving ten stamps must not count as finishing a card either");
    assert_eq!(
        sara_after.lifetime_stamps, sara_before.lifetime_stamps - 10,
        "lifetime stamps should follow the stamps out"
    );
    assert_eq!(
        hamza_after.lifetime_stamps, hamza_before.lifetime_stamps + 10,
        "lifetime stamps should follow the stamps in"
    );
}

#[test]
fn test_transfer_refuses_more_stamps_than_the_card_holds() {
    let program_id = loyalty::id();
    let mut svm = LiteSVM::new();
    let (shop, sara, _hamza, relayer, sara_card, hamza_card) = setup(&mut svm, program_id, 2, 1);

    let res = transfer(&mut svm, program_id, shop.business, sara_card, hamza_card, &sara, &relayer, 3);
    assert_fails_with(res, "NotEnoughStamps");

    assert_eq!(read_card(&svm, sara_card).stamps, 2, "a refused transfer must change nothing");
    assert_eq!(read_card(&svm, hamza_card).stamps, 1, "a refused transfer must change nothing");
}

#[test]
fn test_transfer_refuses_zero_stamps() {
    let program_id = loyalty::id();
    let mut svm = LiteSVM::new();
    let (shop, sara, _hamza, relayer, sara_card, hamza_card) = setup(&mut svm, program_id, 2, 1);

    let res = transfer(&mut svm, program_id, shop.business, sara_card, hamza_card, &sara, &relayer, 0);
    assert_fails_with(res, "InvalidStampAmount");
}

/// Anchor refuses the same account passed twice when both are mutable, so a card can never send stamps
/// to itself and quietly double them. The check lives in the framework rather than in the handler, which
/// is why this asserts Anchor's own error.
#[test]
fn test_transfer_refuses_a_card_sending_to_itself() {
    let program_id = loyalty::id();
    let mut svm = LiteSVM::new();
    let (shop, sara, _hamza, relayer, sara_card, _hamza_card) = setup(&mut svm, program_id, 5, 1);

    let res = transfer(&mut svm, program_id, shop.business, sara_card, sara_card, &sara, &relayer, 2);
    assert_fails_with(res, "ConstraintDuplicateMutableAccount");

    assert_eq!(read_card(&svm, sara_card).stamps, 5, "a refused transfer must change nothing");
}

/// The rule that makes stamps safe to move at all: a stamp is a debt of the shop that issued it, so it
/// must never land on a card at a different shop.
#[test]
fn test_transfer_refuses_a_card_at_a_different_shop() {
    let program_id = loyalty::id();
    let mut svm = LiteSVM::new();
    let (shop, sara, _hamza, relayer, sara_card, _hamza_card) = setup(&mut svm, program_id, 5, 1);

    // A second shop, with its own customer holding a card there.
    let other_owner = Keypair::new();
    let bilal = Keypair::new();
    svm.airdrop(&other_owner.pubkey(), 10_000_000_000).unwrap();
    svm.airdrop(&bilal.pubkey(), 10_000_000_000).unwrap();
    let (other_business, _) =
        Pubkey::find_program_address(&[b"business", other_owner.pubkey().as_ref()], &program_id);
    register(&mut svm, program_id, &other_owner, &relayer, other_business, "Naan Corner");
    add_stamp(&mut svm, program_id, &other_owner, &relayer, other_business, &bilal, 200);
    let bilal_card = card_pda(program_id, other_business, &bilal);

    // Sara's shop, Sara's card, but a card belonging to the other shop on the receiving end.
    let res = transfer(&mut svm, program_id, shop.business, sara_card, bilal_card, &sara, &relayer, 2);
    assert_fails_with(res, "ConstraintHasOne");

    assert_eq!(read_card(&svm, sara_card).stamps, 5, "a refused transfer must change nothing");
    assert_eq!(read_card(&svm, bilal_card).stamps, 1, "a refused transfer must change nothing");
}

#[test]
fn test_transfer_refuses_someone_elses_card() {
    let program_id = loyalty::id();
    let mut svm = LiteSVM::new();
    let (shop, _sara, hamza, relayer, sara_card, hamza_card) = setup(&mut svm, program_id, 6, 1);

    // Hamza signs, but tries to send stamps off Sara's card.
    let res = transfer(&mut svm, program_id, shop.business, sara_card, hamza_card, &hamza, &relayer, 3);
    assert_fails_with(res, "ConstraintHasOne");

    assert_eq!(read_card(&svm, sara_card).stamps, 6, "a refused transfer must change nothing");
}

/// There is deliberately no `init_if_needed` on the receiving card: without that rule, one person could
/// send a single stamp to a thousand new addresses and leave the relayer paying rent on a thousand
/// accounts for good.
#[test]
fn test_transfer_refuses_a_receiving_card_that_does_not_exist_yet() {
    let program_id = loyalty::id();
    let mut svm = LiteSVM::new();
    let (shop, sara, _hamza, relayer, sara_card, _hamza_card) = setup(&mut svm, program_id, 5, 1);

    let stranger = Keypair::new();
    let never_opened = card_pda(program_id, shop.business, &stranger);

    let res = transfer(&mut svm, program_id, shop.business, sara_card, never_opened, &sara, &relayer, 2);
    assert_fails_with(res, "AccountNotInitialized");

    assert_eq!(read_card(&svm, sara_card).stamps, 5, "a refused transfer must change nothing");
}

/// Pooling stamps is the whole point: two people who would each have gone home empty-handed end up with
/// one finished card between them, and the reward can actually be claimed.
#[test]
fn test_pooled_stamps_can_be_cashed_in_as_a_real_reward() {
    let program_id = loyalty::id();
    let mut svm = LiteSVM::new();
    let (shop, sara, _hamza, relayer, sara_card, hamza_card) = setup(&mut svm, program_id, 5, 5);

    // Neither card is worth a reward on its own.
    assert!(read_card(&svm, sara_card).stamps < STAMPS_REQUIRED);
    assert!(read_card(&svm, hamza_card).stamps < STAMPS_REQUIRED);

    assert!(transfer(&mut svm, program_id, shop.business, sara_card, hamza_card, &sara, &relayer, 5).is_ok());

    let hamza_after = read_card(&svm, hamza_card);
    assert_eq!(hamza_after.stamps, STAMPS_REQUIRED, "the pooled card should now be full");
    assert!(
        hamza_after.stamps >= hamza_after.stamps_required_snapshot,
        "the pooled card should pass the same check mint_voucher makes"
    );
    assert_eq!(read_card(&svm, sara_card).stamps, 0, "the stamps really left the other card");
}
