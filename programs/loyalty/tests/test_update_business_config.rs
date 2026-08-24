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

fn register(svm: &mut LiteSVM, program_id: Pubkey, owner: &Keypair, business_pda: Pubkey) {
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
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    let blockhash = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(&[instruction], Some(&owner.pubkey()), &blockhash);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[owner]).unwrap();
    assert!(svm.send_transaction(tx).is_ok(), "setup registration should succeed");
}

#[test]
fn test_owner_can_update_config() {
    let program_id = loyalty::id();
    let owner = Keypair::new();
    let mut svm = LiteSVM::new();
    let bytes = include_bytes!("../../../target/deploy/loyalty.so");
    svm.add_program(program_id, bytes).unwrap();
    svm.airdrop(&owner.pubkey(), 1_000_000_000).unwrap();

    let (business_pda, _bump) =
        Pubkey::find_program_address(&[b"business", owner.pubkey().as_ref()], &program_id);

    register(&mut svm, program_id, &owner, business_pda);

    let update_ix = Instruction::new_with_bytes(
        program_id,
        &loyalty::instruction::UpdateBusinessConfig {
            reward_label: "Free large coffee".to_string(),
            stamps_required: 12,
            min_purchase_amount: 150_000,
            receipt_ttl_seconds: 600,
        }
        .data(),
        loyalty::accounts::UpdateBusinessConfig {
            business: business_pda,
            authority: owner.pubkey(),
        }
        .to_account_metas(None),
    );
    let blockhash = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(&[update_ix], Some(&owner.pubkey()), &blockhash);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[owner]).unwrap();
    let res = svm.send_transaction(tx);
    assert!(res.is_ok(), "owner's update should succeed: {:?}", res);

    let account = svm.get_account(&business_pda).expect("business account should exist");
    let data = loyalty::Business::try_deserialize(&mut account.data.as_slice()).unwrap();
    assert_eq!(data.reward_label, "Free large coffee");
    assert_eq!(data.stamps_required, 12);
    assert_eq!(data.min_purchase_amount, 150_000);
    // Unchanged fields should still be the original values.
    assert_eq!(data.name, "Coffee Corner");
}

#[test]
fn test_impostor_cannot_update_config() {
    let program_id = loyalty::id();
    let owner = Keypair::new();
    let impostor = Keypair::new();
    let mut svm = LiteSVM::new();
    let bytes = include_bytes!("../../../target/deploy/loyalty.so");
    svm.add_program(program_id, bytes).unwrap();
    svm.airdrop(&owner.pubkey(), 1_000_000_000).unwrap();
    svm.airdrop(&impostor.pubkey(), 1_000_000_000).unwrap();

    let (business_pda, _bump) =
        Pubkey::find_program_address(&[b"business", owner.pubkey().as_ref()], &program_id);

    register(&mut svm, program_id, &owner, business_pda);

    // The impostor signs, but targets the owner's business PDA and claims
    // (falsely) to be its authority.
    let update_ix = Instruction::new_with_bytes(
        program_id,
        &loyalty::instruction::UpdateBusinessConfig {
            reward_label: "Hijacked".to_string(),
            stamps_required: 1,
            min_purchase_amount: 1,
            receipt_ttl_seconds: 1,
        }
        .data(),
        loyalty::accounts::UpdateBusinessConfig {
            business: business_pda,
            authority: impostor.pubkey(),
        }
        .to_account_metas(None),
    );
    let blockhash = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(&[update_ix], Some(&impostor.pubkey()), &blockhash);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[impostor]).unwrap();
    let res = svm.send_transaction(tx);
    assert!(res.is_err(), "impostor's update should have been rejected, but it succeeded");
}