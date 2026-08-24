use {
    anchor_lang::{
        prelude::Pubkey,
        solana_program::{instruction::Instruction, system_program},
        InstructionData, ToAccountMetas,
    },
    litesvm::LiteSVM,
    solana_message::{Message, VersionedMessage},
    solana_signer::Signer,
    solana_keypair::Keypair,
    solana_transaction::versioned::VersionedTransaction,
};

#[test]
fn test_register_business() {
    let program_id = loyalty::id();
    let authority = Keypair::new();
    let mut svm = LiteSVM::new();
    let bytes = include_bytes!("../../../target/deploy/loyalty.so");
    svm.add_program(program_id, bytes).unwrap();
    svm.airdrop(&authority.pubkey(), 1_000_000_000).unwrap();

    let (business_pda, _bump) = Pubkey::find_program_address(
        &[b"business", authority.pubkey().as_ref()],
        &program_id,
    );

    let instruction = Instruction::new_with_bytes(
        program_id,
        &loyalty::instruction::RegisterBusiness {
            name: "Coffee Corner".to_string(),
            category: "cafe".to_string(),
            reward_label: "Free coffee".to_string(),
            stamps_required: 10,
            min_purchase_amount: 100_000, // 1000.00 PKR in minor units
            currency: "PKR".to_string(),
            receipt_ttl_seconds: 300,
        }
        .data(),
        loyalty::accounts::RegisterBusiness {
            business: business_pda,
            authority: authority.pubkey(),
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );

    let blockhash = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(&[instruction], Some(&authority.pubkey()), &blockhash);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[authority]).unwrap();

    let res = svm.send_transaction(tx);
    assert!(res.is_ok(), "register_business should succeed: {:?}", res);
}


#[test]
fn test_register_business_twice_fails() {
    let program_id = loyalty::id();
    let authority = Keypair::new();
    let mut svm = LiteSVM::new();
    let bytes = include_bytes!("../../../target/deploy/loyalty.so");
    svm.add_program(program_id, bytes).unwrap();
    svm.airdrop(&authority.pubkey(), 1_000_000_000).unwrap();

    let (business_pda, _bump) = Pubkey::find_program_address(
        &[b"business", authority.pubkey().as_ref()],
        &program_id,
    );

    let build_instruction = || {
        Instruction::new_with_bytes(
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
                authority: authority.pubkey(),
                system_program: system_program::ID,
            }
            .to_account_metas(None),
        )
    };

    // First registration: should succeed.
    let blockhash = svm.latest_blockhash();
    let msg1 = Message::new_with_blockhash(&[build_instruction()], Some(&authority.pubkey()), &blockhash);
    let tx1 = VersionedTransaction::try_new(VersionedMessage::Legacy(msg1), &[&authority]).unwrap();
    let res1 = svm.send_transaction(tx1);
    assert!(res1.is_ok(), "first registration should succeed: {:?}", res1);

    // Second registration, same wallet, same PDA: must fail.
    let blockhash2 = svm.latest_blockhash();
    let msg2 = Message::new_with_blockhash(&[build_instruction()], Some(&authority.pubkey()), &blockhash2);
    let tx2 = VersionedTransaction::try_new(VersionedMessage::Legacy(msg2), &[&authority]).unwrap();
    let res2 = svm.send_transaction(tx2);
    assert!(res2.is_err(), "second registration should fail, but it succeeded");
}