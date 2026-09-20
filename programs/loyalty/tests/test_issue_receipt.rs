mod common;
use common::assert_fails_with;

use {
    anchor_lang::{
        prelude::Pubkey,
        solana_program::{instruction::Instruction, system_program},
        AccountDeserialize,
        InstructionData, ToAccountMetas,
    },
    litesvm::LiteSVM,
    solana_message::{Message, VersionedMessage},
    solana_signer::Signer,
    solana_keypair::Keypair,
    solana_transaction::versioned::VersionedTransaction,
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

#[test]
fn test_issue_receipt_succeeds() {
    let program_id = loyalty::id();
    let owner = Keypair::new();
    let relayer = Keypair::new();
    let mut svm = LiteSVM::new();
    let bytes = include_bytes!("../../../target/deploy/loyalty.so");
    svm.add_program(program_id, bytes).unwrap();
    svm.airdrop(&owner.pubkey(), 1_000_000_000).unwrap();
    svm.airdrop(&relayer.pubkey(), 1_000_000_000).unwrap();

    let (business_pda, _bump) =
        Pubkey::find_program_address(&[b"business", owner.pubkey().as_ref()], &program_id);
    register(&mut svm, program_id, &owner, &relayer, business_pda);

    let secret_hash: [u8; 32] = [7u8; 32];
    let (receipt_pda, _rbump) = Pubkey::find_program_address(
        &[b"receipt", business_pda.as_ref(), secret_hash.as_ref()],
        &program_id,
    );

    let ix = Instruction::new_with_bytes(
        program_id,
        &loyalty::instruction::IssueReceipt {
            secret_hash,
            amount_band: 1,
        }
        .data(),
        loyalty::accounts::IssueReceipt {
            business: business_pda,
            receipt: receipt_pda,
            authority: owner.pubkey(),
            relayer: relayer.pubkey(),
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );

    let blockhash = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(&[ix], Some(&relayer.pubkey()), &blockhash);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[owner, relayer]).unwrap();
    let res = svm.send_transaction(tx);
    assert!(res.is_ok(), "issue_receipt should succeed: {:?}", res);

    let account = svm.get_account(&receipt_pda).expect("receipt account should exist");
    let data = loyalty::Receipt::try_deserialize(&mut account.data.as_slice()).unwrap();
    assert_eq!(data.business, business_pda);
    assert_eq!(data.amount_band, 1);
    assert_eq!(data.expires_at, data.issued_at + 300);
}

#[test]
fn test_issue_receipt_zero_amount_band_fails() {
    let program_id = loyalty::id();
    let owner = Keypair::new();
    let relayer = Keypair::new();
    let mut svm = LiteSVM::new();
    let bytes = include_bytes!("../../../target/deploy/loyalty.so");
    svm.add_program(program_id, bytes).unwrap();
    svm.airdrop(&owner.pubkey(), 1_000_000_000).unwrap();
    svm.airdrop(&relayer.pubkey(), 1_000_000_000).unwrap();

    let (business_pda, _bump) =
        Pubkey::find_program_address(&[b"business", owner.pubkey().as_ref()], &program_id);
    register(&mut svm, program_id, &owner, &relayer, business_pda);

    let secret_hash: [u8; 32] = [9u8; 32];
    let (receipt_pda, _rbump) = Pubkey::find_program_address(
        &[b"receipt", business_pda.as_ref(), secret_hash.as_ref()],
        &program_id,
    );

    let ix = Instruction::new_with_bytes(
        program_id,
        &loyalty::instruction::IssueReceipt {
            secret_hash,
            amount_band: 0,
        }
        .data(),
        loyalty::accounts::IssueReceipt {
            business: business_pda,
            receipt: receipt_pda,
            authority: owner.pubkey(),
            relayer: relayer.pubkey(),
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );

    let blockhash = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(&[ix], Some(&relayer.pubkey()), &blockhash);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[owner, relayer]).unwrap();
    let res = svm.send_transaction(tx);
    assert_fails_with(res, "InvalidAmountBand");
}

#[test]
fn test_issue_receipt_unregistered_wallet_fails() {
    let program_id = loyalty::id();
    let stranger = Keypair::new();
    let relayer = Keypair::new();
    let mut svm = LiteSVM::new();
    let bytes = include_bytes!("../../../target/deploy/loyalty.so");
    svm.add_program(program_id, bytes).unwrap();
    svm.airdrop(&stranger.pubkey(), 1_000_000_000).unwrap();
    svm.airdrop(&relayer.pubkey(), 1_000_000_000).unwrap();

    // stranger never called register_business — this PDA has never been created.
    let (business_pda, _bump) =
        Pubkey::find_program_address(&[b"business", stranger.pubkey().as_ref()], &program_id);

    let secret_hash: [u8; 32] = [3u8; 32];
    let (receipt_pda, _rbump) = Pubkey::find_program_address(
        &[b"receipt", business_pda.as_ref(), secret_hash.as_ref()],
        &program_id,
    );

    let ix = Instruction::new_with_bytes(
        program_id,
        &loyalty::instruction::IssueReceipt {
            secret_hash,
            amount_band: 1,
        }
        .data(),
        loyalty::accounts::IssueReceipt {
            business: business_pda,
            receipt: receipt_pda,
            authority: stranger.pubkey(),
            relayer: relayer.pubkey(),
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );

    let blockhash = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(&[ix], Some(&relayer.pubkey()), &blockhash);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[stranger, relayer]).unwrap();
    let res = svm.send_transaction(tx);
    assert_fails_with(res, "AccountNotInitialized");
}