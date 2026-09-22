mod common;
use common::assert_fails_with;

use {
    anchor_lang::{
        prelude::Pubkey,
        solana_program::{instruction::Instruction, system_program},
        AccountDeserialize,
        InstructionData, ToAccountMetas,
    },
    litesvm::{types::TransactionResult, LiteSVM},
    solana_clock::Clock,
    solana_message::{Message, VersionedMessage},
    solana_signer::Signer,
    solana_keypair::Keypair,
    solana_transaction::versioned::VersionedTransaction,
    solana_keccak_hasher as keccak,
};

fn register(svm: &mut LiteSVM, program_id: Pubkey, owner: &Keypair, relayer: &Keypair, business_pda: Pubkey) {
    let instruction = Instruction::new_with_bytes(
        program_id,
        &loyalty::instruction::RegisterBusiness {
            name: "Coffee Corner".to_string(),
            category: "cafe".to_string(),
            reward_label: "Free coffee".to_string(),
            stamps_required: 10,
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
    let blockhash = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(&[instruction], Some(&relayer.pubkey()), &blockhash);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[owner, relayer]).unwrap();
    assert!(svm.send_transaction(tx).is_ok(), "setup registration should succeed");
}

fn warp(svm: &mut LiteSVM, seconds: i64) {
    let mut clock = svm.get_sysvar::<Clock>();
    clock.unix_timestamp += seconds;
    svm.set_sysvar::<Clock>(&clock);
}

fn issue_receipt(
    svm: &mut LiteSVM,
    program_id: Pubkey,
    owner: &Keypair,
    relayer: &Keypair,
    business_pda: Pubkey,
    secret_hash: [u8; 32],
    amount_band: u8,
) -> Pubkey {
    let (receipt_pda, _) = Pubkey::find_program_address(
        &[b"receipt", business_pda.as_ref(), secret_hash.as_ref()],
        &program_id,
    );
    let ix = Instruction::new_with_bytes(
        program_id,
        &loyalty::instruction::IssueReceipt { secret_hash, amount_band }.data(),
        loyalty::accounts::IssueReceipt {
            business: business_pda,
            receipt: receipt_pda,
            authority: owner.pubkey(),
            relayer: relayer.pubkey(),
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    let bh = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(&[ix], Some(&relayer.pubkey()), &bh);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[owner, relayer]).unwrap();
    assert!(svm.send_transaction(tx).is_ok(), "setup issue should succeed");
    receipt_pda
}

fn setup_base(svm: &mut LiteSVM, program_id: Pubkey) -> (Keypair, Keypair, Keypair, Pubkey) {
    let owner = Keypair::new();
    let customer = Keypair::new();
    let relayer = Keypair::new();
    svm.add_program(program_id, include_bytes!("../../../target/deploy/loyalty.so")).unwrap();
    svm.airdrop(&owner.pubkey(), 1_000_000_000).unwrap();
    svm.airdrop(&customer.pubkey(), 1_000_000_000).unwrap();
    svm.airdrop(&relayer.pubkey(), 1_000_000_000).unwrap();
    let (business_pda, _) = Pubkey::find_program_address(&[b"business", owner.pubkey().as_ref()], &program_id);
    register(svm, program_id, &owner, &relayer, business_pda);
    (owner, customer, relayer, business_pda)
}

fn claim(
    svm: &mut LiteSVM,
    program_id: Pubkey,
    business_pda: Pubkey,
    receipt_pda: Pubkey,
    card_pda: Pubkey,
    customer: &Keypair,
    relayer: &Keypair,
    secret: [u8; 32],
) -> TransactionResult {
    let ix = Instruction::new_with_bytes(
        program_id,
        &loyalty::instruction::ClaimReceipt { secret }.data(),
        loyalty::accounts::ClaimReceipt {
            business: business_pda,
            receipt: receipt_pda,
            card: card_pda,
            customer: customer.pubkey(),
            relayer: relayer.pubkey(),
            rent_payer: relayer.pubkey(),
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    // A fresh blockhash every time: claiming the same receipt twice sends the same transaction twice, and LiteSVM
    // would refuse the repeat as "AlreadyProcessed" before the program ever ran.
    svm.expire_blockhash();
    let bh = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(&[ix], Some(&relayer.pubkey()), &bh);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[customer, relayer]).unwrap();
    svm.send_transaction(tx)
}

#[test]
fn test_claim_receipt_first_stamp_creates_card() {
    let program_id = loyalty::id();
    let mut svm = LiteSVM::new();
    let (owner, customer, relayer, business_pda) = setup_base(&mut svm, program_id);

    let secret = [1u8; 32];
    let secret_hash = keccak::hash(secret.as_ref()).to_bytes();
    let receipt_pda = issue_receipt(&mut svm, program_id, &owner, &relayer, business_pda, secret_hash, 1);

    let (card_pda, _) = Pubkey::find_program_address(
        &[b"card", business_pda.as_ref(), customer.pubkey().as_ref()],
        &program_id,
    );

    let res = claim(&mut svm, program_id, business_pda, receipt_pda, card_pda, &customer, &relayer, secret);
    assert!(res.is_ok(), "first claim should succeed");

    let card_account = svm.get_account(&card_pda).unwrap();
    let card = loyalty::LoyaltyCard::try_deserialize(&mut card_account.data.as_slice()).unwrap();
    assert_eq!(card.stamps, 1, "first stamp should bring the card to 1");
}

#[test]
fn test_claim_receipt_second_stamp_reuses_card() {
    let program_id = loyalty::id();
    let mut svm = LiteSVM::new();
    let (owner, customer, relayer, business_pda) = setup_base(&mut svm, program_id);

    let (card_pda, _) = Pubkey::find_program_address(
        &[b"card", business_pda.as_ref(), customer.pubkey().as_ref()],
        &program_id,
    );

    let secret1 = [1u8; 32];
    let hash1 = keccak::hash(secret1.as_ref()).to_bytes();
    let receipt1 = issue_receipt(&mut svm, program_id, &owner, &relayer, business_pda, hash1, 1);
    assert!(claim(&mut svm, program_id, business_pda, receipt1, card_pda, &customer, &relayer, secret1).is_ok());

    warp(&mut svm, 61); // clear the stamp cooldown

    let secret2 = [2u8; 32];
    let hash2 = keccak::hash(secret2.as_ref()).to_bytes();
    let receipt2 = issue_receipt(&mut svm, program_id, &owner, &relayer, business_pda, hash2, 1);
    let res = claim(&mut svm, program_id, business_pda, receipt2, card_pda, &customer, &relayer, secret2);
    assert!(res.is_ok(), "second claim should succeed and reuse the same card");

    let card_account = svm.get_account(&card_pda).unwrap();
    let card = loyalty::LoyaltyCard::try_deserialize(&mut card_account.data.as_slice()).unwrap();
    assert_eq!(card.stamps, 2, "card should now have 2 stamps");

    
    let business_account = svm.get_account(&business_pda).unwrap();
    let business_data = loyalty::Business::try_deserialize(&mut business_account.data.as_slice()).unwrap();
    assert_eq!(
        business_data.total_cards, 1,
        "two claims by the same customer at the same business should leave total_cards at 1, not 2"
    );
    
}

#[test]
fn test_claim_receipt_twice_fails() {
    let program_id = loyalty::id();
    let mut svm = LiteSVM::new();
    let (owner, customer, relayer, business_pda) = setup_base(&mut svm, program_id);

    let secret = [1u8; 32];
    let secret_hash = keccak::hash(secret.as_ref()).to_bytes();
    let receipt_pda = issue_receipt(&mut svm, program_id, &owner, &relayer, business_pda, secret_hash, 1);

    let (card_pda, _) = Pubkey::find_program_address(
        &[b"card", business_pda.as_ref(), customer.pubkey().as_ref()],
        &program_id,
    );

    assert!(claim(&mut svm, program_id, business_pda, receipt_pda, card_pda, &customer, &relayer, secret).is_ok());

    let res = claim(&mut svm, program_id, business_pda, receipt_pda, card_pda, &customer, &relayer, secret);
    assert_fails_with(res, "AccountNotInitialized");
}

#[test]
fn test_claim_receipt_wrong_secret_fails() {
    let program_id = loyalty::id();
    let mut svm = LiteSVM::new();
    let (owner, customer, relayer, business_pda) = setup_base(&mut svm, program_id);

    let real_secret = [1u8; 32];
    let secret_hash = keccak::hash(real_secret.as_ref()).to_bytes();
    let receipt_pda = issue_receipt(&mut svm, program_id, &owner, &relayer, business_pda, secret_hash, 1);

    let (card_pda, _) = Pubkey::find_program_address(
        &[b"card", business_pda.as_ref(), customer.pubkey().as_ref()],
        &program_id,
    );

    let wrong_secret = [99u8; 32];
    let res = claim(&mut svm, program_id, business_pda, receipt_pda, card_pda, &customer, &relayer, wrong_secret);
    assert_fails_with(res, "InvalidSecret");
}

#[test]
fn test_claim_receipt_expired_fails() {
    let program_id = loyalty::id();
    let mut svm = LiteSVM::new();
    let (owner, customer, relayer, business_pda) = setup_base(&mut svm, program_id);

    let secret = [1u8; 32];
    let secret_hash = keccak::hash(secret.as_ref()).to_bytes();
    let receipt_pda = issue_receipt(&mut svm, program_id, &owner, &relayer, business_pda, secret_hash, 1);

    warp(&mut svm, 301); // past the 300-second receipt_ttl_seconds used in registration

    let (card_pda, _) = Pubkey::find_program_address(
        &[b"card", business_pda.as_ref(), customer.pubkey().as_ref()],
        &program_id,
    );

    let res = claim(&mut svm, program_id, business_pda, receipt_pda, card_pda, &customer, &relayer, secret);
    assert_fails_with(res, "ReceiptExpired");
}

#[test]
fn test_claim_receipt_cross_tenant_fails() {
    let program_id = loyalty::id();
    let mut svm = LiteSVM::new();
    let owner_a = Keypair::new();
    let owner_b = Keypair::new();
    let customer = Keypair::new();
    let relayer = Keypair::new();
    svm.add_program(program_id, include_bytes!("../../../target/deploy/loyalty.so")).unwrap();
    svm.airdrop(&owner_a.pubkey(), 1_000_000_000).unwrap();
    svm.airdrop(&owner_b.pubkey(), 1_000_000_000).unwrap();
    svm.airdrop(&customer.pubkey(), 1_000_000_000).unwrap();
    svm.airdrop(&relayer.pubkey(), 1_000_000_000).unwrap();

    let (business_a, _) = Pubkey::find_program_address(&[b"business", owner_a.pubkey().as_ref()], &program_id);
    let (business_b, _) = Pubkey::find_program_address(&[b"business", owner_b.pubkey().as_ref()], &program_id);
    register(&mut svm, program_id, &owner_a, &relayer, business_a);
    register(&mut svm, program_id, &owner_b, &relayer, business_b);

    let secret = [1u8; 32];
    let secret_hash = keccak::hash(secret.as_ref()).to_bytes();
    let receipt_pda = issue_receipt(&mut svm, program_id, &owner_a, &relayer, business_a, secret_hash, 1);

    // Try to claim business A's receipt against business B's card.
    let (card_pda_b, _) = Pubkey::find_program_address(
        &[b"card", business_b.as_ref(), customer.pubkey().as_ref()],
        &program_id,
    );

    let res = claim(&mut svm, program_id, business_b, receipt_pda, card_pda_b, &customer, &relayer, secret);
    assert_fails_with(res, "InvalidSecret");
}
fn claim_ix_paying_back(program_id: Pubkey, business_pda: Pubkey, receipt_pda: Pubkey, customer: Pubkey, relayer: Pubkey, rent_payer: Pubkey, secret: [u8; 32]) -> Instruction {
    let (card_pda, _) = Pubkey::find_program_address(&[b"card", business_pda.as_ref(), customer.as_ref()], &program_id);
    Instruction::new_with_bytes(
        program_id,
        &loyalty::instruction::ClaimReceipt { secret }.data(),
        loyalty::accounts::ClaimReceipt {
            business: business_pda,
            receipt: receipt_pda,
            card: card_pda,
            customer,
            relayer,
            rent_payer,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

fn reclaim_ix(program_id: Pubkey, receipt_pda: Pubkey, rent_payer: Pubkey) -> Instruction {
    Instruction::new_with_bytes(
        program_id,
        &loyalty::instruction::ReclaimExpiredReceipt {}.data(),
        loyalty::accounts::ReclaimExpiredReceipt { receipt: receipt_pda, rent_payer }.to_account_metas(None),
    )
}

fn send_signed(svm: &mut LiteSVM, ix: Instruction, payer: &Keypair, signers: &[&Keypair]) -> TransactionResult {
    svm.expire_blockhash();
    let bh = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(&[ix], Some(&payer.pubkey()), &bh);
    svm.send_transaction(VersionedTransaction::try_new(VersionedMessage::Legacy(msg), signers).unwrap())
}

const FEE_PER_SIGNATURE: u64 = 5_000;

#[test]
fn test_claimed_receipt_rent_goes_back_to_the_relayer() {
    let program_id = loyalty::id();
    let mut svm = LiteSVM::new();
    let (owner, customer, relayer, business_pda) = setup_base(&mut svm, program_id);
    let (card_pda, _) = Pubkey::find_program_address(&[b"card", business_pda.as_ref(), customer.pubkey().as_ref()], &program_id);

    // A first stamp, so the card exists and the next claim creates nothing new.
    let first = issue_receipt(&mut svm, program_id, &owner, &relayer, business_pda, keccak::hash(&[1u8; 32]).to_bytes(), 1);
    assert!(claim(&mut svm, program_id, business_pda, first, card_pda, &customer, &relayer, [1u8; 32]).is_ok());
    warp(&mut svm, 61);

    let relayer_before = svm.get_balance(&relayer.pubkey()).unwrap();
    let business_before = svm.get_balance(&business_pda).unwrap();
    svm.expire_blockhash();
    let second = issue_receipt(&mut svm, program_id, &owner, &relayer, business_pda, keccak::hash(&[2u8; 32]).to_bytes(), 1);
    let receipt = loyalty::Receipt::try_deserialize(&mut svm.get_account(&second).unwrap().data.as_slice()).unwrap();
    assert_eq!(receipt.rent_payer, relayer.pubkey(), "the receipt records who paid its rent");
    assert!(claim(&mut svm, program_id, business_pda, second, card_pda, &customer, &relayer, [2u8; 32]).is_ok());

    let spent = relayer_before - svm.get_balance(&relayer.pubkey()).unwrap();
    println!("a stamp cost the relayer {spent} lamports in total");
    assert_eq!(spent, 2 * 2 * FEE_PER_SIGNATURE, "only the two transaction fees are spent; the receipt's rent came back");
    assert_eq!(svm.get_balance(&business_pda).unwrap(), business_before, "the business gains nothing: it never paid");
    assert!(svm.get_account(&second).map_or(true, |a| a.lamports == 0), "the receipt is closed");
}

#[test]
fn test_claim_cannot_send_the_receipt_rent_elsewhere() {
    let program_id = loyalty::id();
    let mut svm = LiteSVM::new();
    let (owner, customer, relayer, business_pda) = setup_base(&mut svm, program_id);
    let secret = [3u8; 32];
    let receipt_pda = issue_receipt(&mut svm, program_id, &owner, &relayer, business_pda, keccak::hash(&secret).to_bytes(), 1);

    // The customer tries to collect the rent the relayer paid.
    let ix = claim_ix_paying_back(program_id, business_pda, receipt_pda, customer.pubkey(), relayer.pubkey(), customer.pubkey(), secret);
    let res = send_signed(&mut svm, ix, &relayer, &[&customer, &relayer]);
    assert_fails_with(res, "ConstraintHasOne");
}

#[test]
fn test_expired_receipt_rent_goes_back_to_the_relayer() {
    let program_id = loyalty::id();
    let mut svm = LiteSVM::new();
    let (owner, _customer, relayer, business_pda) = setup_base(&mut svm, program_id);
    let relayer_before = svm.get_balance(&relayer.pubkey()).unwrap();
    let receipt_pda = issue_receipt(&mut svm, program_id, &owner, &relayer, business_pda, keccak::hash(&[4u8; 32]).to_bytes(), 1);
    warp(&mut svm, 301);

    // The clean-up needs no merchant: only whoever pays the fee signs.
    let ix = reclaim_ix(program_id, receipt_pda, relayer.pubkey());
    assert!(send_signed(&mut svm, ix, &relayer, &[&relayer]).is_ok(), "reclaim should succeed");
    let spent = relayer_before - svm.get_balance(&relayer.pubkey()).unwrap();
    assert_eq!(spent, 2 * FEE_PER_SIGNATURE + FEE_PER_SIGNATURE, "only the issue fee and the one-signature clean-up fee are spent");
}

#[test]
fn test_anyone_can_clean_up_an_expired_receipt_for_the_relayer() {
    let program_id = loyalty::id();
    let mut svm = LiteSVM::new();
    let (owner, _customer, relayer, business_pda) = setup_base(&mut svm, program_id);
    let receipt_pda = issue_receipt(&mut svm, program_id, &owner, &relayer, business_pda, keccak::hash(&[6u8; 32]).to_bytes(), 1);
    let rent = svm.get_balance(&receipt_pda).unwrap();

    let stranger = Keypair::new();
    svm.airdrop(&stranger.pubkey(), 1_000_000_000).unwrap();
    let early = send_signed(&mut svm, reclaim_ix(program_id, receipt_pda, relayer.pubkey()), &stranger, &[&stranger]);
    assert_fails_with(early, "ReceiptNotYetExpired");

    warp(&mut svm, 301);
    let relayer_before = svm.get_balance(&relayer.pubkey()).unwrap();
    assert!(send_signed(&mut svm, reclaim_ix(program_id, receipt_pda, relayer.pubkey()), &stranger, &[&stranger]).is_ok());
    assert_eq!(svm.get_balance(&relayer.pubkey()).unwrap() - relayer_before, rent, "the whole rent went to the relayer, who paid it");
}

#[test]
fn test_nobody_can_redirect_an_expired_receipts_rent() {
    let program_id = loyalty::id();
    let mut svm = LiteSVM::new();
    let (owner, _customer, relayer, business_pda) = setup_base(&mut svm, program_id);
    let receipt_pda = issue_receipt(&mut svm, program_id, &owner, &relayer, business_pda, keccak::hash(&[5u8; 32]).to_bytes(), 1);
    warp(&mut svm, 301);

    // The merchant names themselves as the one to be paid back.
    let res = send_signed(&mut svm, reclaim_ix(program_id, receipt_pda, owner.pubkey()), &owner, &[&owner]);
    assert_fails_with(res, "ConstraintHasOne");
}
