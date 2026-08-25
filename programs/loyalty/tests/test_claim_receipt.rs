use {
    anchor_lang::{
        prelude::Pubkey,
        solana_program::{instruction::Instruction, system_program},
        AccountDeserialize,
        InstructionData, ToAccountMetas,
    },
    litesvm::LiteSVM,
    solana_clock::Clock,
    solana_message::{Message, VersionedMessage},
    solana_signer::Signer,
    solana_keypair::Keypair,
    solana_transaction::versioned::VersionedTransaction,
    solana_keccak_hasher as keccak,
};

fn register(svm: &mut LiteSVM, program_id: Pubkey, owner: &Keypair, business_pda: Pubkey, stamps_required: u8, receipt_ttl_seconds: u32) {
    let instruction = Instruction::new_with_bytes(
        program_id,
        &loyalty::instruction::RegisterBusiness {
            name: "Coffee Corner".to_string(),
            category: "cafe".to_string(),
            reward_label: "Free coffee".to_string(),
            stamps_required,
            min_purchase_amount: 100_000,
            currency: "PKR".to_string(),
            receipt_ttl_seconds,
        }
        .data(),
        loyalty::accounts::RegisterBusiness {
            business: business_pda,
            authority: owner.pubkey(),
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    let blockhash = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(&[instruction], Some(&owner.pubkey()), &blockhash);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[owner]).unwrap();
    assert!(svm.send_transaction(tx).is_ok(), "setup registration should succeed");
}

fn issue(svm: &mut LiteSVM, program_id: Pubkey, owner: &Keypair, business_pda: Pubkey, secret: [u8; 32]) -> Pubkey {
    let secret_hash = keccak::hash(secret.as_ref()).to_bytes();
    let (receipt_pda, _bump) = Pubkey::find_program_address(
        &[b"receipt", business_pda.as_ref(), secret_hash.as_ref()],
        &program_id,
    );
    let instruction = Instruction::new_with_bytes(
        program_id,
        &loyalty::instruction::IssueReceipt { secret_hash, amount_band: 1 }.data(),
        loyalty::accounts::IssueReceipt {
            business: business_pda,
            receipt: receipt_pda,
            authority: owner.pubkey(),
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    let blockhash = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(&[instruction], Some(&owner.pubkey()), &blockhash);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[owner]).unwrap();
    assert!(svm.send_transaction(tx).is_ok(), "setup issue_receipt should succeed");
    receipt_pda
}

fn claim_ix(program_id: Pubkey, business_pda: Pubkey, receipt_pda: Pubkey, card_pda: Pubkey, customer: Pubkey, secret: [u8; 32]) -> Instruction {
    Instruction::new_with_bytes(
        program_id,
        &loyalty::instruction::ClaimReceipt { secret }.data(),
        loyalty::accounts::ClaimReceipt {
            business: business_pda,
            receipt: receipt_pda,
            card: card_pda,
            customer,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

#[test]
fn test_claim_receipt_first_stamp_creates_card() {
    let program_id = loyalty::id();
    let owner = Keypair::new();
    let customer = Keypair::new();
    let mut svm = LiteSVM::new();
    svm.add_program(program_id, include_bytes!("../../../target/deploy/loyalty.so")).unwrap();
    svm.airdrop(&owner.pubkey(), 1_000_000_000).unwrap();
    svm.airdrop(&customer.pubkey(), 1_000_000_000).unwrap();

    let (business_pda, _) = Pubkey::find_program_address(&[b"business", owner.pubkey().as_ref()], &program_id);
    register(&mut svm, program_id, &owner, business_pda, 10, 300);

    let secret = [11u8; 32];
    let receipt_pda = issue(&mut svm, program_id, &owner, business_pda, secret);
    let (card_pda, _) = Pubkey::find_program_address(&[b"card", business_pda.as_ref(), customer.pubkey().as_ref()], &program_id);

    let ix = claim_ix(program_id, business_pda, receipt_pda, card_pda, customer.pubkey(), secret);
    let blockhash = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(&[ix], Some(&customer.pubkey()), &blockhash);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[&customer]).unwrap();
    assert!(svm.send_transaction(tx).is_ok(), "claim should succeed");

    let card_account = svm.get_account(&card_pda).expect("card should exist");
    let card = loyalty::LoyaltyCard::try_deserialize(&mut card_account.data.as_slice()).unwrap();
    assert_eq!(card.stamps, 1);
    assert_eq!(card.business, business_pda);
    assert_eq!(card.customer, customer.pubkey());
    assert!(svm.get_account(&receipt_pda).is_none(), "receipt should be closed after claim");

    let business_account = svm.get_account(&business_pda).unwrap();
    let business = loyalty::Business::try_deserialize(&mut business_account.data.as_slice()).unwrap();
    assert_eq!(business.total_cards, 1);
    assert_eq!(business.total_stamps_issued, 1);
}

#[test]
fn test_claim_receipt_second_stamp_reuses_card() {
    let program_id = loyalty::id();
    let owner = Keypair::new();
    let customer = Keypair::new();
    let mut svm = LiteSVM::new();
    svm.add_program(program_id, include_bytes!("../../../target/deploy/loyalty.so")).unwrap();
    svm.airdrop(&owner.pubkey(), 1_000_000_000).unwrap();
    svm.airdrop(&customer.pubkey(), 1_000_000_000).unwrap();

    let (business_pda, _) = Pubkey::find_program_address(&[b"business", owner.pubkey().as_ref()], &program_id);
    register(&mut svm, program_id, &owner, business_pda, 10, 300);
    let (card_pda, _) = Pubkey::find_program_address(&[b"card", business_pda.as_ref(), customer.pubkey().as_ref()], &program_id);

    for secret_byte in [21u8, 22u8] {
        let secret = [secret_byte; 32];
        let receipt_pda = issue(&mut svm, program_id, &owner, business_pda, secret);
        let ix = claim_ix(program_id, business_pda, receipt_pda, card_pda, customer.pubkey(), secret);
        let blockhash = svm.latest_blockhash();
        let msg = Message::new_with_blockhash(&[ix], Some(&customer.pubkey()), &blockhash);
        let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[&customer]).unwrap();
        assert!(svm.send_transaction(tx).is_ok(), "claim should succeed");
    }

    let card_account = svm.get_account(&card_pda).unwrap();
    let card = loyalty::LoyaltyCard::try_deserialize(&mut card_account.data.as_slice()).unwrap();
    assert_eq!(card.stamps, 2);

    let business_account = svm.get_account(&business_pda).unwrap();
    let business = loyalty::Business::try_deserialize(&mut business_account.data.as_slice()).unwrap();
    assert_eq!(business.total_cards, 1, "the same card must not be counted twice");
    assert_eq!(business.total_stamps_issued, 2);
}

#[test]
fn test_claim_receipt_wrong_secret_fails() {
    let program_id = loyalty::id();
    let owner = Keypair::new();
    let customer = Keypair::new();
    let mut svm = LiteSVM::new();
    svm.add_program(program_id, include_bytes!("../../../target/deploy/loyalty.so")).unwrap();
    svm.airdrop(&owner.pubkey(), 1_000_000_000).unwrap();
    svm.airdrop(&customer.pubkey(), 1_000_000_000).unwrap();

    let (business_pda, _) = Pubkey::find_program_address(&[b"business", owner.pubkey().as_ref()], &program_id);
    register(&mut svm, program_id, &owner, business_pda, 10, 300);

    let real_secret = [31u8; 32];
    let receipt_pda = issue(&mut svm, program_id, &owner, business_pda, real_secret);
    let (card_pda, _) = Pubkey::find_program_address(&[b"card", business_pda.as_ref(), customer.pubkey().as_ref()], &program_id);

    let wrong_secret = [99u8; 32];
    let ix = claim_ix(program_id, business_pda, receipt_pda, card_pda, customer.pubkey(), wrong_secret);
    let blockhash = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(&[ix], Some(&customer.pubkey()), &blockhash);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[&customer]).unwrap();
    assert!(svm.send_transaction(tx).is_err(), "wrong secret should be rejected, but it succeeded");
}

#[test]
fn test_claim_receipt_expired_fails() {
    let program_id = loyalty::id();
    let owner = Keypair::new();
    let customer = Keypair::new();
    let mut svm = LiteSVM::new();
    svm.add_program(program_id, include_bytes!("../../../target/deploy/loyalty.so")).unwrap();
    svm.airdrop(&owner.pubkey(), 1_000_000_000).unwrap();
    svm.airdrop(&customer.pubkey(), 1_000_000_000).unwrap();

    let (business_pda, _) = Pubkey::find_program_address(&[b"business", owner.pubkey().as_ref()], &program_id);
    register(&mut svm, program_id, &owner, business_pda, 10, 1); // 1-second validity

    let secret = [41u8; 32];
    let receipt_pda = issue(&mut svm, program_id, &owner, business_pda, secret);

    // Fast-forward the simulated chain's clock, no real waiting involved.
    let mut clock = svm.get_sysvar::<Clock>();
    clock.unix_timestamp += 3600;
    svm.set_sysvar::<Clock>(&clock);

    let (card_pda, _) = Pubkey::find_program_address(&[b"card", business_pda.as_ref(), customer.pubkey().as_ref()], &program_id);
    let ix = claim_ix(program_id, business_pda, receipt_pda, card_pda, customer.pubkey(), secret);
    let blockhash = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(&[ix], Some(&customer.pubkey()), &blockhash);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[&customer]).unwrap();
    assert!(svm.send_transaction(tx).is_err(), "expired receipt should be rejected, but it succeeded");
}

#[test]
fn test_claim_receipt_cross_tenant_fails() {
    let program_id = loyalty::id();
    let owner_a = Keypair::new();
    let owner_b = Keypair::new();
    let customer = Keypair::new();
    let mut svm = LiteSVM::new();
    svm.add_program(program_id, include_bytes!("../../../target/deploy/loyalty.so")).unwrap();
    svm.airdrop(&owner_a.pubkey(), 1_000_000_000).unwrap();
    svm.airdrop(&owner_b.pubkey(), 1_000_000_000).unwrap();
    svm.airdrop(&customer.pubkey(), 1_000_000_000).unwrap();

    let (business_a, _) = Pubkey::find_program_address(&[b"business", owner_a.pubkey().as_ref()], &program_id);
    let (business_b, _) = Pubkey::find_program_address(&[b"business", owner_b.pubkey().as_ref()], &program_id);
    register(&mut svm, program_id, &owner_a, business_a, 10, 300);
    register(&mut svm, program_id, &owner_b, business_b, 10, 300);

    let secret = [51u8; 32];
    let receipt_pda = issue(&mut svm, program_id, &owner_a, business_a, secret); // issued by A

    // Try to claim it against business B's card instead.
    let (card_pda, _) = Pubkey::find_program_address(&[b"card", business_b.as_ref(), customer.pubkey().as_ref()], &program_id);
    let ix = claim_ix(program_id, business_b, receipt_pda, card_pda, customer.pubkey(), secret);
    let blockhash = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(&[ix], Some(&customer.pubkey()), &blockhash);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[&customer]).unwrap();
    assert!(svm.send_transaction(tx).is_err(), "a receipt from business A must not stamp business B's card");
}