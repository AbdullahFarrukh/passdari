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

fn register(svm: &mut LiteSVM, program_id: Pubkey, owner: &Keypair, relayer: &Keypair, business_pda: Pubkey, stamps_required: u8) {
    let instruction = Instruction::new_with_bytes(
        program_id,
        &loyalty::instruction::RegisterBusiness {
            name: "Coffee Corner".to_string(),
            category: "cafe".to_string(),
            reward_label: "Free coffee".to_string(),
            stamps_required,
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

fn issue_and_claim(
    svm: &mut LiteSVM,
    program_id: Pubkey,
    owner: &Keypair,
    business_pda: Pubkey,
    customer: &Keypair,
    relayer: &Keypair,
    secret_byte: u8,
) {
    let secret = [secret_byte; 32];
    let secret_hash = keccak::hash(secret.as_ref()).to_bytes();
    let (receipt_pda, _) = Pubkey::find_program_address(
        &[b"receipt", business_pda.as_ref(), secret_hash.as_ref()],
        &program_id,
    );

    let issue_ix = Instruction::new_with_bytes(
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
    let bh1 = svm.latest_blockhash();
    let msg1 = Message::new_with_blockhash(&[issue_ix], Some(&relayer.pubkey()), &bh1);
    let tx1 = VersionedTransaction::try_new(VersionedMessage::Legacy(msg1), &[owner, relayer]).unwrap();
    assert!(svm.send_transaction(tx1).is_ok(), "setup issue should succeed");

    let (card_pda, _) = Pubkey::find_program_address(
        &[b"card", business_pda.as_ref(), customer.pubkey().as_ref()],
        &program_id,
    );

    let claim_ix = Instruction::new_with_bytes(
        program_id,
        &loyalty::instruction::ClaimReceipt { secret }.data(),
        loyalty::accounts::ClaimReceipt {
            business: business_pda,
            receipt: receipt_pda,
            card: card_pda,
            customer: customer.pubkey(),
            relayer: relayer.pubkey(),
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    let bh2 = svm.latest_blockhash();
    let msg2 = Message::new_with_blockhash(&[claim_ix], Some(&relayer.pubkey()), &bh2);
    let tx2 = VersionedTransaction::try_new(VersionedMessage::Legacy(msg2), &[customer, relayer]).unwrap();
    assert!(svm.send_transaction(tx2).is_ok(), "setup claim should succeed");
}

fn warp(svm: &mut LiteSVM, seconds: i64) {
    let mut clock = svm.get_sysvar::<Clock>();
    clock.unix_timestamp += seconds;
    svm.set_sysvar::<Clock>(&clock);
}

fn fill_card(
    svm: &mut LiteSVM,
    program_id: Pubkey,
    owner: &Keypair,
    business_pda: Pubkey,
    customer: &Keypair,
    relayer: &Keypair,
    stamps: u8,
) {
    for i in 0..stamps {
        issue_and_claim(svm, program_id, owner, business_pda, customer, relayer, 100u8.wrapping_add(i));
        warp(svm, 61); // clear the stamp cooldown before the next claim
    }
}

fn mint(
    svm: &mut LiteSVM,
    program_id: Pubkey,
    business_pda: Pubkey,
    customer: &Keypair,
    relayer: &Keypair,
    voucher_id: u64,
) -> Pubkey {
    let (card_pda, _) = Pubkey::find_program_address(
        &[b"card", business_pda.as_ref(), customer.pubkey().as_ref()],
        &program_id,
    );
    let (voucher_pda, _) = Pubkey::find_program_address(
        &[b"voucher", business_pda.as_ref(), &voucher_id.to_le_bytes()],
        &program_id,
    );

    let ix = Instruction::new_with_bytes(
        program_id,
        &loyalty::instruction::MintVoucher { voucher_id }.data(),
        loyalty::accounts::MintVoucher {
            business: business_pda,
            card: card_pda,
            voucher: voucher_pda,
            customer: customer.pubkey(),
            relayer: relayer.pubkey(),
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    let bh = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(&[ix], Some(&relayer.pubkey()), &bh);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[customer, relayer]).unwrap();
    assert!(svm.send_transaction(tx).is_ok(), "setup mint should succeed");
    voucher_pda
}

fn present(svm: &mut LiteSVM, program_id: Pubkey, voucher_pda: Pubkey, owner: &Keypair) {
    let ix = Instruction::new_with_bytes(
        program_id,
        &loyalty::instruction::PresentVoucher {}.data(),
        loyalty::accounts::PresentVoucher { voucher: voucher_pda, owner: owner.pubkey() }
            .to_account_metas(None),
    );
    let bh = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(&[ix], Some(&owner.pubkey()), &bh);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[owner]).unwrap();
    assert!(svm.send_transaction(tx).is_ok(), "setup present should succeed");
}

fn setup_base(svm: &mut LiteSVM, program_id: Pubkey, stamps_required: u8) -> (Keypair, Keypair, Keypair, Pubkey) {
    let owner = Keypair::new();
    let customer = Keypair::new();
    let relayer = Keypair::new();
    svm.add_program(program_id, include_bytes!("../../../target/deploy/loyalty.so")).unwrap();
    svm.airdrop(&owner.pubkey(), 1_000_000_000).unwrap();
    svm.airdrop(&customer.pubkey(), 1_000_000_000).unwrap();
    svm.airdrop(&relayer.pubkey(), 1_000_000_000).unwrap();
    let (business_pda, _) = Pubkey::find_program_address(&[b"business", owner.pubkey().as_ref()], &program_id);
    register(svm, program_id, &owner, &relayer, business_pda, stamps_required);
    (owner, customer, relayer, business_pda)
}

fn card_pda_for(program_id: Pubkey, business_pda: Pubkey, customer: Pubkey) -> Pubkey {
    let (card_pda, _) = Pubkey::find_program_address(
        &[b"card", business_pda.as_ref(), customer.as_ref()],
        &program_id,
    );
    card_pda
}

// --- Test 1: a merchant cannot reduce a card's stamps by any instruction ---
#[test]
fn test_redeem_does_not_touch_card_stamps() {
    let program_id = loyalty::id();
    let mut svm = LiteSVM::new();
    let (owner, customer, relayer, business_pda) = setup_base(&mut svm, program_id, 2);

    fill_card(&mut svm, program_id, &owner, business_pda, &customer, &relayer, 2);

    let (card_pda, _) = Pubkey::find_program_address(
        &[b"card", business_pda.as_ref(), customer.pubkey().as_ref()],
        &program_id,
    );

    let voucher_pda = mint(&mut svm, program_id, business_pda, &customer, &relayer, 0);
    present(&mut svm, program_id, voucher_pda, &customer);

    let card_before = svm.get_account(&card_pda).unwrap();
    let stamps_before = loyalty::LoyaltyCard::try_deserialize(&mut card_before.data.as_slice()).unwrap().stamps;

    let redeem_ix = Instruction::new_with_bytes(
        program_id,
        &loyalty::instruction::RedeemVoucher {}.data(),
        loyalty::accounts::RedeemVoucher {
            business: business_pda,
            voucher: voucher_pda,
            authority: owner.pubkey(),
        }
        .to_account_metas(None),
    );
    let bh = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(&[redeem_ix], Some(&owner.pubkey()), &bh);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[&owner]).unwrap();
    assert!(svm.send_transaction(tx).is_ok(), "redeem should succeed");

    let card_after = svm.get_account(&card_pda).unwrap();
    let stamps_after = loyalty::LoyaltyCard::try_deserialize(&mut card_after.data.as_slice()).unwrap().stamps;

    assert_eq!(
        stamps_before, stamps_after,
        "redeeming a voucher must never change the card's stamp count — mint_voucher is the only place stamps decrease"
    );
}

// --- Test 2: mint_voucher below the stamp threshold fails ---
#[test]
fn test_mint_voucher_below_threshold_fails() {
    let program_id = loyalty::id();
    let mut svm = LiteSVM::new();
    let (owner, customer, relayer, business_pda) = setup_base(&mut svm, program_id, 5);

    fill_card(&mut svm, program_id, &owner, business_pda, &customer, &relayer, 2); // only 2 of 5 needed

    let (card_pda, _) = Pubkey::find_program_address(
        &[b"card", business_pda.as_ref(), customer.pubkey().as_ref()],
        &program_id,
    );
    let (voucher_pda, _) = Pubkey::find_program_address(
        &[b"voucher", business_pda.as_ref(), &0u64.to_le_bytes()],
        &program_id,
    );

    let ix = Instruction::new_with_bytes(
        program_id,
        &loyalty::instruction::MintVoucher { voucher_id: 0u64 }.data(),
        loyalty::accounts::MintVoucher {
            business: business_pda,
            card: card_pda,
            voucher: voucher_pda,
            customer: customer.pubkey(),
            relayer: relayer.pubkey(),
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    let bh = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(&[ix], Some(&relayer.pubkey()), &bh);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[&customer, &relayer]).unwrap();
    let res = svm.send_transaction(tx);
    assert!(res.is_err(), "minting below the stamp threshold should fail, but it succeeded");
}


// --- Test: raising the threshold after a card exists does not retroactively lock it out ---
#[test]
fn test_raising_threshold_does_not_void_earned_reward() {
    let program_id = loyalty::id();
    let mut svm = LiteSVM::new();
    let (owner, customer, relayer, business_pda) = setup_base(&mut svm, program_id, 5);

    // Customer earns exactly the 5 stamps required at registration time.
    fill_card(&mut svm, program_id, &owner, business_pda, &customer, &relayer, 5);

    // Merchant raises the threshold after the fact.
    let update_ix = Instruction::new_with_bytes(
        program_id,
        &loyalty::instruction::UpdateBusinessConfig {
            reward_label: "Free coffee".to_string(),
            stamps_required: 20,
            min_purchase_amount: 100_000,
            receipt_ttl_seconds: 300,
        }
        .data(),
        loyalty::accounts::UpdateBusinessConfig {
            business: business_pda,
            authority: owner.pubkey(),
        }
        .to_account_metas(None),
    );
    let bh = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(&[update_ix], Some(&owner.pubkey()), &bh);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[&owner]).unwrap();
    assert!(svm.send_transaction(tx).is_ok(), "raising the threshold should succeed");

    // The customer, still sitting on only 5 stamps, should still be able to mint —
    // the card locked in "5" at creation time, before the threshold changed.
    let card_pda = card_pda_for(program_id, business_pda, customer.pubkey());
    let (voucher_pda, _) = Pubkey::find_program_address(
        &[b"voucher", business_pda.as_ref(), &0u64.to_le_bytes()],
        &program_id,
    );

    let mint_ix = Instruction::new_with_bytes(
        program_id,
        &loyalty::instruction::MintVoucher { voucher_id: 0u64 }.data(),
        loyalty::accounts::MintVoucher {
            business: business_pda,
            card: card_pda,
            voucher: voucher_pda,
            customer: customer.pubkey(),
            relayer: relayer.pubkey(),
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    let bh2 = svm.latest_blockhash();
    let msg2 = Message::new_with_blockhash(&[mint_ix], Some(&relayer.pubkey()), &bh2);
    let tx2 = VersionedTransaction::try_new(VersionedMessage::Legacy(msg2), &[&customer, &relayer]).unwrap();
    let res = svm.send_transaction(tx2);
    assert!(
        res.is_ok(),
        "a customer who already earned the original threshold must still be able to mint, even after the merchant raises it: {:?}",
        res
    );
}

// --- Test 3: redeem_voucher on a voucher that was never presented fails ---
#[test]
fn test_redeem_unpresented_voucher_fails() {
    let program_id = loyalty::id();
    let mut svm = LiteSVM::new();
    let (owner, customer, relayer, business_pda) = setup_base(&mut svm, program_id, 1);

    fill_card(&mut svm, program_id, &owner, business_pda, &customer, &relayer, 1);
    let voucher_pda = mint(&mut svm, program_id, business_pda, &customer, &relayer, 0);
    // Deliberately skip present_voucher.

    let ix = Instruction::new_with_bytes(
        program_id,
        &loyalty::instruction::RedeemVoucher {}.data(),
        loyalty::accounts::RedeemVoucher {
            business: business_pda,
            voucher: voucher_pda,
            authority: owner.pubkey(),
        }
        .to_account_metas(None),
    );
    let bh = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(&[ix], Some(&owner.pubkey()), &bh);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[&owner]).unwrap();
    let res = svm.send_transaction(tx);
    assert!(res.is_err(), "redeeming an unpresented voucher should fail, but it succeeded");
}

// --- Test 4: a merchant cannot redeem a voucher issued by a different business ---
#[test]
fn test_cross_business_redeem_fails() {
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
    register(&mut svm, program_id, &owner_a, &relayer, business_a, 1);
    register(&mut svm, program_id, &owner_b, &relayer, business_b, 1);

    fill_card(&mut svm, program_id, &owner_a, business_a, &customer, &relayer, 1);
    let voucher_pda = mint(&mut svm, program_id, business_a, &customer, &relayer, 0); // minted at A
    present(&mut svm, program_id, voucher_pda, &customer);

    let ix = Instruction::new_with_bytes(
        program_id,
        &loyalty::instruction::RedeemVoucher {}.data(),
        loyalty::accounts::RedeemVoucher {
            business: business_b,
            voucher: voucher_pda,
            authority: owner_b.pubkey(),
        }
        .to_account_metas(None),
    );
    let bh = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(&[ix], Some(&owner_b.pubkey()), &bh);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[&owner_b]).unwrap();
    let res = svm.send_transaction(tx);
    assert!(res.is_err(), "business B must not redeem a voucher issued by business A");
}

// --- Test 5: after transfer, the old owner can no longer transfer or present it ---
#[test]
fn test_old_owner_locked_out_after_transfer() {
    let program_id = loyalty::id();
    let mut svm = LiteSVM::new();
    let (owner, customer, relayer, business_pda) = setup_base(&mut svm, program_id, 1);
    let new_owner = Keypair::new();

    fill_card(&mut svm, program_id, &owner, business_pda, &customer, &relayer, 1);
    let voucher_pda = mint(&mut svm, program_id, business_pda, &customer, &relayer, 0);

    let transfer_ix = Instruction::new_with_bytes(
        program_id,
        &loyalty::instruction::TransferVoucher { new_owner: new_owner.pubkey() }.data(),
        loyalty::accounts::TransferVoucher { voucher: voucher_pda, owner: customer.pubkey() }
            .to_account_metas(None),
    );
    let bh1 = svm.latest_blockhash();
    let msg1 = Message::new_with_blockhash(&[transfer_ix], Some(&customer.pubkey()), &bh1);
    let tx1 = VersionedTransaction::try_new(VersionedMessage::Legacy(msg1), &[&customer]).unwrap();
    assert!(svm.send_transaction(tx1).is_ok(), "setup transfer should succeed");

    let present_ix = Instruction::new_with_bytes(
        program_id,
        &loyalty::instruction::PresentVoucher {}.data(),
        loyalty::accounts::PresentVoucher { voucher: voucher_pda, owner: customer.pubkey() }
            .to_account_metas(None),
    );
    let bh2 = svm.latest_blockhash();
    let msg2 = Message::new_with_blockhash(&[present_ix], Some(&customer.pubkey()), &bh2);
    let tx2 = VersionedTransaction::try_new(VersionedMessage::Legacy(msg2), &[&customer]).unwrap();
    let res2 = svm.send_transaction(tx2);
    assert!(res2.is_err(), "the old owner should not be able to present a voucher they no longer own");

    let transfer_again_ix = Instruction::new_with_bytes(
        program_id,
        &loyalty::instruction::TransferVoucher { new_owner: owner.pubkey() }.data(),
        loyalty::accounts::TransferVoucher { voucher: voucher_pda, owner: customer.pubkey() }
            .to_account_metas(None),
    );
    let bh3 = svm.latest_blockhash();
    let msg3 = Message::new_with_blockhash(&[transfer_again_ix], Some(&customer.pubkey()), &bh3);
    let tx3 = VersionedTransaction::try_new(VersionedMessage::Legacy(msg3), &[&customer]).unwrap();
    let res3 = svm.send_transaction(tx3);
    assert!(res3.is_err(), "the old owner should not be able to transfer a voucher they no longer own");
}

// --- Test 6: transfer_voucher while pending fails ---
#[test]
fn test_transfer_while_pending_fails() {
    let program_id = loyalty::id();
    let mut svm = LiteSVM::new();
    let (owner, customer, relayer, business_pda) = setup_base(&mut svm, program_id, 1);
    let recipient = Keypair::new();

    fill_card(&mut svm, program_id, &owner, business_pda, &customer, &relayer, 1);
    let voucher_pda = mint(&mut svm, program_id, business_pda, &customer, &relayer, 0);
    present(&mut svm, program_id, voucher_pda, &customer);

    let ix = Instruction::new_with_bytes(
        program_id,
        &loyalty::instruction::TransferVoucher { new_owner: recipient.pubkey() }.data(),
        loyalty::accounts::TransferVoucher { voucher: voucher_pda, owner: customer.pubkey() }
            .to_account_metas(None),
    );
    let bh = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(&[ix], Some(&customer.pubkey()), &bh);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[&customer]).unwrap();
    let res = svm.send_transaction(tx);
    assert!(res.is_err(), "gifting a voucher that's currently presented to a merchant should fail");
}

// --- Test 7: redeeming twice fails (the voucher is closed) ---
#[test]
fn test_redeem_twice_fails() {
    let program_id = loyalty::id();
    let mut svm = LiteSVM::new();
    let (owner, customer, relayer, business_pda) = setup_base(&mut svm, program_id, 1);

    fill_card(&mut svm, program_id, &owner, business_pda, &customer, &relayer, 1);
    let voucher_pda = mint(&mut svm, program_id, business_pda, &customer, &relayer, 0);
    present(&mut svm, program_id, voucher_pda, &customer);

    let redeem_ix = || {
        Instruction::new_with_bytes(
            program_id,
            &loyalty::instruction::RedeemVoucher {}.data(),
            loyalty::accounts::RedeemVoucher {
                business: business_pda,
                voucher: voucher_pda,
                authority: owner.pubkey(),
            }
            .to_account_metas(None),
        )
    };

    let bh1 = svm.latest_blockhash();
    let msg1 = Message::new_with_blockhash(&[redeem_ix()], Some(&owner.pubkey()), &bh1);
    let tx1 = VersionedTransaction::try_new(VersionedMessage::Legacy(msg1), &[&owner]).unwrap();
    assert!(svm.send_transaction(tx1).is_ok(), "first redemption should succeed");

    let bh2 = svm.latest_blockhash();
    let msg2 = Message::new_with_blockhash(&[redeem_ix()], Some(&owner.pubkey()), &bh2);
    let tx2 = VersionedTransaction::try_new(VersionedMessage::Legacy(msg2), &[&owner]).unwrap();
    let res2 = svm.send_transaction(tx2);
    assert!(res2.is_err(), "redeeming the same voucher twice should fail — it was closed after the first redemption");
}