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

#[test]
fn test_register_business() {
    let program_id = loyalty::id();
    let authority = Keypair::new();
    let relayer = Keypair::new();
    let mut svm = LiteSVM::new();
    let bytes = include_bytes!("../../../target/deploy/loyalty.so");
    svm.add_program(program_id, bytes).unwrap();
    svm.airdrop(&authority.pubkey(), 1_000_000_000).unwrap();
    svm.airdrop(&relayer.pubkey(), 1_000_000_000).unwrap();

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
            relayer: relayer.pubkey(),
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );

    let blockhash = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(&[instruction], Some(&relayer.pubkey()), &blockhash);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[authority, relayer]).unwrap();

    let res = svm.send_transaction(tx);
    assert!(res.is_ok(), "register_business should succeed: {:?}", res);
}


#[test]
fn test_register_business_twice_fails() {
    let program_id = loyalty::id();
    let authority = Keypair::new();
    let relayer = Keypair::new();
    let mut svm = LiteSVM::new();
    let bytes = include_bytes!("../../../target/deploy/loyalty.so");
    svm.add_program(program_id, bytes).unwrap();
    svm.airdrop(&authority.pubkey(), 1_000_000_000).unwrap();
    svm.airdrop(&relayer.pubkey(), 1_000_000_000).unwrap();

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
                relayer: relayer.pubkey(),
                system_program: system_program::ID,
            }
            .to_account_metas(None),
        )
    };

    // First registration: should succeed.
    let blockhash = svm.latest_blockhash();
    let msg1 = Message::new_with_blockhash(&[build_instruction()], Some(&relayer.pubkey()), &blockhash);
    let tx1 = VersionedTransaction::try_new(VersionedMessage::Legacy(msg1), &[&authority, &relayer]).unwrap();
    let res1 = svm.send_transaction(tx1);
    assert!(res1.is_ok(), "first registration should succeed: {:?}", res1);

    // Second registration, same wallet, same PDA: must fail.
    let blockhash2 = svm.latest_blockhash();
    let msg2 = Message::new_with_blockhash(&[build_instruction()], Some(&relayer.pubkey()), &blockhash2);
    let tx2 = VersionedTransaction::try_new(VersionedMessage::Legacy(msg2), &[&authority, &relayer]).unwrap();
    let res2 = svm.send_transaction(tx2);
    assert!(res2.is_err(), "second registration should fail, but it succeeded");
}


#[test]
fn test_multiple_businesses_dont_collide() {
    let program_id = loyalty::id();
    let mut svm = LiteSVM::new();
    let bytes = include_bytes!("../../../target/deploy/loyalty.so");
    svm.add_program(program_id, bytes).unwrap();

    let cafe = Keypair::new();
    let bakery = Keypair::new();
    let restaurant = Keypair::new();
    let relayer = Keypair::new();

    svm.airdrop(&cafe.pubkey(), 1_000_000_000).unwrap();
    svm.airdrop(&bakery.pubkey(), 1_000_000_000).unwrap();
    svm.airdrop(&restaurant.pubkey(), 1_000_000_000).unwrap();
    svm.airdrop(&relayer.pubkey(), 1_000_000_000).unwrap();

    let owners = [
        (&cafe, "Coffee Corner", "cafe", "Free coffee", 10u8, 100_000u64),
        (&bakery, "Sweet Bakes", "bakery", "Free pastry", 8u8, 50_000u64),
        (&restaurant, "Grill House", "restaurant", "Free dessert", 5u8, 300_000u64),
    ];

    let mut pdas = vec![];

    for (owner, name, category, reward_label, stamps_required, min_purchase_amount) in owners.iter() {
        let (business_pda, _bump) =
            Pubkey::find_program_address(&[b"business", owner.pubkey().as_ref()], &program_id);
        pdas.push(business_pda);

        let instruction = Instruction::new_with_bytes(
            program_id,
            &loyalty::instruction::RegisterBusiness {
                name: name.to_string(),
                category: category.to_string(),
                reward_label: reward_label.to_string(),
                stamps_required: *stamps_required,
                min_purchase_amount: *min_purchase_amount,
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
        let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[*owner, &relayer]).unwrap();
        let res = svm.send_transaction(tx);
        assert!(res.is_ok(), "registering {} should succeed: {:?}", name, res);
    }

    // All three PDAs must be genuinely different addresses.
    assert_ne!(pdas[0], pdas[1], "cafe and bakery landed on the same address");
    assert_ne!(pdas[1], pdas[2], "bakery and restaurant landed on the same address");
    assert_ne!(pdas[0], pdas[2], "cafe and restaurant landed on the same address");

    // Read the bakery's account back and confirm it has the bakery's data, not
    // some other business's — proving the accounts don't just have different
    // addresses but genuinely separate, correct data behind each one.
    let bakery_account = svm.get_account(&pdas[1]).expect("bakery account should exist");
    let bakery_data = loyalty::Business::try_deserialize(&mut bakery_account.data.as_slice())
        .expect("should deserialize as Business");
    assert_eq!(bakery_data.name, "Sweet Bakes");
    assert_eq!(bakery_data.stamps_required, 8);
    assert_eq!(bakery_data.min_purchase_amount, 50_000);
}