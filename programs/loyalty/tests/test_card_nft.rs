mod common;
use common::assert_fails_with;

use {
    anchor_lang::{
        prelude::Pubkey,
        solana_program::{instruction::Instruction, system_program},
        AccountDeserialize,
        InstructionData, ToAccountMetas,
    },
    anchor_spl::{
        associated_token::{
            get_associated_token_address_with_program_id,
            spl_associated_token_account::instruction::create_associated_token_account,
        },
        token_2022::spl_token_2022::{
            self,
            extension::{
                mint_close_authority::MintCloseAuthority, non_transferable::NonTransferable,
                permanent_delegate::PermanentDelegate, BaseStateWithExtensions,
                StateWithExtensions,
            },
            state::{Account as TokenState, Mint as MintState},
        },
        token_2022_extensions::spl_token_metadata_interface::state::TokenMetadata,
    },
    litesvm::{types::TransactionResult, LiteSVM},
    solana_clock::Clock,
    solana_message::{Message, VersionedMessage},
    solana_signer::Signer,
    solana_keypair::Keypair,
    solana_transaction::versioned::VersionedTransaction,
    solana_keccak_hasher as keccak,
};

const URI: &str = "https://passdari.example/c/0.json";

fn send(svm: &mut LiteSVM, ixs: &[Instruction], payer: &Keypair, signers: &[&Keypair]) -> TransactionResult {
    // A fresh blockhash each time, or a repeated transaction would be refused as "AlreadyProcessed".
    svm.expire_blockhash();
    let blockhash = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(ixs, Some(&payer.pubkey()), &blockhash);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), signers).unwrap();
    svm.send_transaction(tx)
}

fn warp(svm: &mut LiteSVM, seconds: i64) {
    let mut clock = svm.get_sysvar::<Clock>();
    clock.unix_timestamp += seconds;
    svm.set_sysvar::<Clock>(&clock);
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
    assert!(send(svm, &[instruction], &relayer, &[&owner, &relayer]).is_ok(), "setup registration should succeed");
    (owner, customer, relayer, business_pda)
}

fn card_pda_for(program_id: Pubkey, business_pda: Pubkey, customer: Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[b"card", business_pda.as_ref(), customer.as_ref()], &program_id).0
}

fn card_mint_for(program_id: Pubkey, card: Pubkey, cycle: u32) -> Pubkey {
    Pubkey::find_program_address(&[b"card_mint", card.as_ref(), &cycle.to_le_bytes()], &program_id).0
}

fn ata(wallet: Pubkey, mint: Pubkey) -> Pubkey {
    get_associated_token_address_with_program_id(&wallet, &mint, &spl_token_2022::ID)
}

fn issue_and_claim_ixs(
    svm: &mut LiteSVM,
    program_id: Pubkey,
    owner: &Keypair,
    business_pda: Pubkey,
    customer: &Keypair,
    relayer: &Keypair,
    secret_byte: u8,
) -> Instruction {
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
    assert!(send(svm, &[issue_ix], relayer, &[owner, relayer]).is_ok(), "setup issue should succeed");

    Instruction::new_with_bytes(
        program_id,
        &loyalty::instruction::ClaimReceipt { secret }.data(),
        loyalty::accounts::ClaimReceipt {
            business: business_pda,
            receipt: receipt_pda,
            card: card_pda_for(program_id, business_pda, customer.pubkey()),
            customer: customer.pubkey(),
            relayer: relayer.pubkey(),
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

fn mint_card_nft_ix(
    program_id: Pubkey,
    business_pda: Pubkey,
    customer: Pubkey,
    relayer: Pubkey,
    cycle: u32,
    uri: &str,
) -> Instruction {
    let card = card_pda_for(program_id, business_pda, customer);
    let mint = card_mint_for(program_id, card, cycle);
    Instruction::new_with_bytes(
        program_id,
        &loyalty::instruction::MintCardNft { uri: uri.to_string() }.data(),
        loyalty::accounts::MintCardNft {
            business: business_pda,
            card,
            mint,
            customer_token: ata(customer, mint),
            customer,
            relayer,
            token_program: spl_token_2022::ID,
            associated_token_program: anchor_spl::associated_token::ID,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

fn mint_voucher_ix(
    program_id: Pubkey,
    business_pda: Pubkey,
    customer: Pubkey,
    relayer: Pubkey,
    voucher_id: u64,
    card_mint: Pubkey,
    card_token: Pubkey,
) -> Instruction {
    let mint = Pubkey::find_program_address(&[b"voucher_mint", business_pda.as_ref(), &voucher_id.to_le_bytes()], &program_id).0;
    Instruction::new_with_bytes(
        program_id,
        &loyalty::instruction::MintVoucher { voucher_id, uri: "https://passdari.example/v/0.json".to_string() }.data(),
        loyalty::accounts::MintVoucher {
            business: business_pda,
            card: card_pda_for(program_id, business_pda, customer),
            voucher: Pubkey::find_program_address(&[b"voucher", business_pda.as_ref(), &voucher_id.to_le_bytes()], &program_id).0,
            mint,
            customer_token: ata(customer, mint),
            card_mint,
            card_token,
            customer,
            relayer,
            token_program: spl_token_2022::ID,
            associated_token_program: anchor_spl::associated_token::ID,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

/// The card's state as the chain has it.
fn card_state(svm: &LiteSVM, program_id: Pubkey, business_pda: Pubkey, customer: Pubkey) -> loyalty::LoyaltyCard {
    let account = svm.get_account(&card_pda_for(program_id, business_pda, customer)).expect("card should exist");
    loyalty::LoyaltyCard::try_deserialize(&mut account.data.as_slice()).unwrap()
}

/// One stamp, plus (when `with_nft`) the card NFT for the card's current cycle in the same
/// transaction, the way the app sends it.
fn stamp(
    svm: &mut LiteSVM,
    program_id: Pubkey,
    owner: &Keypair,
    business_pda: Pubkey,
    customer: &Keypair,
    relayer: &Keypair,
    secret_byte: u8,
    nft_cycle: Option<u32>,
) -> TransactionResult {
    let claim = issue_and_claim_ixs(svm, program_id, owner, business_pda, customer, relayer, secret_byte);
    let mut ixs = vec![claim];
    if let Some(cycle) = nft_cycle {
        ixs.push(mint_card_nft_ix(program_id, business_pda, customer.pubkey(), relayer.pubkey(), cycle, URI));
    }
    let res = send(svm, &ixs, relayer, &[customer, relayer]);
    warp(svm, 61);
    res
}

/// Cashes in with the card NFT of the given cycle, the way the app sends it.
fn cash_in(
    svm: &mut LiteSVM,
    program_id: Pubkey,
    business_pda: Pubkey,
    customer: &Keypair,
    relayer: &Keypair,
    voucher_id: u64,
    cycle: u32,
) -> TransactionResult {
    let card = card_pda_for(program_id, business_pda, customer.pubkey());
    let card_mint = card_mint_for(program_id, card, cycle);
    let ix = mint_voucher_ix(program_id, business_pda, customer.pubkey(), relayer.pubkey(), voucher_id, card_mint, ata(customer.pubkey(), card_mint));
    send(svm, &[ix], relayer, &[customer, relayer])
}

fn is_gone(svm: &LiteSVM, address: Pubkey) -> bool {
    svm.get_account(&address).map_or(true, |a| a.lamports == 0)
}

#[test]
fn test_first_stamp_brings_a_soulbound_nft() {
    let program_id = loyalty::id();
    let mut svm = LiteSVM::new();
    let (owner, customer, relayer, business_pda) = setup_base(&mut svm, program_id, 2);

    let res = stamp(&mut svm, program_id, &owner, business_pda, &customer, &relayer, 1, Some(0));
    assert!(res.is_ok(), "stamp plus card NFT should succeed: {:?}", res);
    println!("claim_receipt + mint_card_nft used {} compute units", res.unwrap().compute_units_consumed);

    let card_pda = card_pda_for(program_id, business_pda, customer.pubkey());
    let mint_pda = card_mint_for(program_id, card_pda, 0);

    let mint_account = svm.get_account(&mint_pda).expect("mint should exist");
    assert_eq!(mint_account.owner, spl_token_2022::ID, "the mint should belong to Token-2022");
    let mint_data = StateWithExtensions::<MintState>::unpack(&mint_account.data).unwrap();
    assert_eq!(mint_data.base.supply, 1, "exactly one token should exist");
    assert_eq!(mint_data.base.decimals, 0, "an NFT has no decimals");
    assert!(mint_data.base.mint_authority.is_none(), "minting must be locked after the first token");
    assert!(mint_data.base.freeze_authority.is_none(), "a card is never frozen, so nobody holds that power");
    assert!(mint_data.get_extension::<NonTransferable>().is_ok(), "the card must be soulbound");
    let delegate = mint_data.get_extension::<PermanentDelegate>().unwrap().delegate;
    assert_eq!(Option::<Pubkey>::from(delegate), Some(card_pda), "the card account should be the permanent delegate");
    let closer = mint_data.get_extension::<MintCloseAuthority>().unwrap().close_authority;
    assert_eq!(Option::<Pubkey>::from(closer), Some(card_pda), "the card account should be able to close the empty mint");

    let metadata = mint_data.get_variable_len_extension::<TokenMetadata>().unwrap();
    assert_eq!(metadata.name, "Coffee Corner stamp card", "the name comes from the business, not the client");
    assert_eq!(metadata.symbol, "PSDC");
    assert_eq!(metadata.uri, URI);
    assert_eq!(metadata.mint, mint_pda);

    let token_account = svm.get_account(&ata(customer.pubkey(), mint_pda)).expect("token account should exist");
    let token = StateWithExtensions::<TokenState>::unpack(&token_account.data).unwrap().base;
    assert_eq!(token.owner, customer.pubkey(), "the customer should hold the card");
    assert_eq!(token.mint, mint_pda);
    assert_eq!(token.amount, 1);

    let card = card_state(&svm, program_id, business_pda, customer.pubkey());
    assert_eq!(card.stamps, 1, "the live stamp count stays in the card account");
    assert_eq!(card.nft_cycle, 0);
}

#[test]
fn test_card_nft_cannot_be_moved_by_the_holder() {
    let program_id = loyalty::id();
    let mut svm = LiteSVM::new();
    let (owner, customer, relayer, business_pda) = setup_base(&mut svm, program_id, 2);
    assert!(stamp(&mut svm, program_id, &owner, business_pda, &customer, &relayer, 1, Some(0)).is_ok());

    let card_pda = card_pda_for(program_id, business_pda, customer.pubkey());
    let mint_pda = card_mint_for(program_id, card_pda, 0);
    let friend = Keypair::new();
    let create_friend_account = create_associated_token_account(&relayer.pubkey(), &friend.pubkey(), &mint_pda, &spl_token_2022::ID);
    let res = send(&mut svm, &[create_friend_account], &relayer, &[&relayer]);
    assert!(res.is_ok(), "opening an account for the token is allowed: {:?}", res);

    // A plain Token-2022 transfer, the way a wallet like Phantom would do it.
    let transfer = spl_token_2022::instruction::transfer_checked(
        &spl_token_2022::ID,
        &ata(customer.pubkey(), mint_pda),
        &mint_pda,
        &ata(friend.pubkey(), mint_pda),
        &customer.pubkey(),
        &[],
        1,
        0,
    )
    .unwrap();
    let res = send(&mut svm, &[transfer], &relayer, &[&customer, &relayer]);
    assert_fails_with(res, "Transfer is disabled for this mint");
}

#[test]
fn test_cashing_in_burns_the_card_nft_and_keeps_leftover_stamps() {
    let program_id = loyalty::id();
    let mut svm = LiteSVM::new();
    let (owner, customer, relayer, business_pda) = setup_base(&mut svm, program_id, 2);
    assert!(stamp(&mut svm, program_id, &owner, business_pda, &customer, &relayer, 1, Some(0)).is_ok());
    assert!(stamp(&mut svm, program_id, &owner, business_pda, &customer, &relayer, 2, None).is_ok());
    assert!(stamp(&mut svm, program_id, &owner, business_pda, &customer, &relayer, 3, None).is_ok());

    let card_pda = card_pda_for(program_id, business_pda, customer.pubkey());
    let mint_pda = card_mint_for(program_id, card_pda, 0);
    let token_pda = ata(customer.pubkey(), mint_pda);
    assert!(!is_gone(&svm, mint_pda) && !is_gone(&svm, token_pda), "the NFT exists before cashing in");

    let res = cash_in(&mut svm, program_id, business_pda, &customer, &relayer, 0, 0);
    assert!(res.is_ok(), "cashing in should succeed: {:?}", res);
    println!("mint_voucher with a card NFT used {} compute units", res.unwrap().compute_units_consumed);

    assert!(is_gone(&svm, mint_pda), "the card NFT's mint should be closed");
    assert!(is_gone(&svm, token_pda), "the customer's token account should be closed");
    let card = card_state(&svm, program_id, business_pda, customer.pubkey());
    assert_eq!(card.stamps, 1, "3 stamps minus the 2 spent leaves 1");
    assert_eq!(card.nft_cycle, 1, "the card moves on to its next NFT");
}

#[test]
fn test_next_stamp_after_cashing_in_mints_a_fresh_nft() {
    let program_id = loyalty::id();
    let mut svm = LiteSVM::new();
    let (owner, customer, relayer, business_pda) = setup_base(&mut svm, program_id, 2);
    assert!(stamp(&mut svm, program_id, &owner, business_pda, &customer, &relayer, 1, Some(0)).is_ok());
    assert!(stamp(&mut svm, program_id, &owner, business_pda, &customer, &relayer, 2, None).is_ok());
    assert!(cash_in(&mut svm, program_id, business_pda, &customer, &relayer, 0, 0).is_ok());

    let res = stamp(&mut svm, program_id, &owner, business_pda, &customer, &relayer, 3, Some(1));
    assert!(res.is_ok(), "the next stamp should bring a new card NFT: {:?}", res);

    let card_pda = card_pda_for(program_id, business_pda, customer.pubkey());
    let old_mint = card_mint_for(program_id, card_pda, 0);
    let new_mint = card_mint_for(program_id, card_pda, 1);
    assert_ne!(old_mint, new_mint, "each NFT gets its own address");
    assert!(is_gone(&svm, old_mint), "the old NFT stays gone");
    let mint_account = svm.get_account(&new_mint).expect("the new mint should exist");
    let mint_data = StateWithExtensions::<MintState>::unpack(&mint_account.data).unwrap();
    assert_eq!(mint_data.base.supply, 1);
    let token = svm.get_account(&ata(customer.pubkey(), new_mint)).expect("the new token account should exist");
    assert_eq!(StateWithExtensions::<TokenState>::unpack(&token.data).unwrap().base.amount, 1);
    assert_eq!(card_state(&svm, program_id, business_pda, customer.pubkey()).stamps, 1);
}

#[test]
fn test_card_made_before_card_nfts_can_cash_in_and_get_one_later() {
    let program_id = loyalty::id();
    let mut svm = LiteSVM::new();
    let (owner, customer, relayer, business_pda) = setup_base(&mut svm, program_id, 2);
    // Two stamps and no NFT at all, like a card from before card NFTs existed.
    assert!(stamp(&mut svm, program_id, &owner, business_pda, &customer, &relayer, 1, None).is_ok());
    assert!(stamp(&mut svm, program_id, &owner, business_pda, &customer, &relayer, 2, None).is_ok());

    let res = cash_in(&mut svm, program_id, business_pda, &customer, &relayer, 0, 0);
    assert!(res.is_ok(), "a card with no NFT must still be able to cash in: {:?}", res);
    let card = card_state(&svm, program_id, business_pda, customer.pubkey());
    assert_eq!(card.stamps, 0);
    assert_eq!(card.nft_cycle, 0, "nothing was burned, so the cycle stays");

    let res = stamp(&mut svm, program_id, &owner, business_pda, &customer, &relayer, 3, Some(0));
    assert!(res.is_ok(), "the next stamp should bring the card its first NFT: {:?}", res);
    let card_pda = card_pda_for(program_id, business_pda, customer.pubkey());
    assert!(!is_gone(&svm, card_mint_for(program_id, card_pda, 0)));
}

/// The customer burns their own card NFT with a plain Token-2022 burn, and
/// optionally closes the token account too.
fn burn_own_card_nft(svm: &mut LiteSVM, program_id: Pubkey, business_pda: Pubkey, customer: &Keypair, relayer: &Keypair, also_close: bool) {
    let card_pda = card_pda_for(program_id, business_pda, customer.pubkey());
    let mint = card_mint_for(program_id, card_pda, 0);
    let token = ata(customer.pubkey(), mint);
    let mut ixs = vec![spl_token_2022::instruction::burn_checked(&spl_token_2022::ID, &token, &mint, &customer.pubkey(), &[], 1, 0).unwrap()];
    if also_close {
        ixs.push(spl_token_2022::instruction::close_account(&spl_token_2022::ID, &token, &customer.pubkey(), &customer.pubkey(), &[]).unwrap());
    }
    let res = send(svm, &ixs, relayer, &[customer, relayer]);
    assert!(res.is_ok(), "the holder can burn their own card: {:?}", res);
}

#[test]
fn test_customer_who_burned_their_card_nft_can_still_cash_in() {
    let program_id = loyalty::id();
    for also_close in [false, true] {
        let mut svm = LiteSVM::new();
        let (owner, customer, relayer, business_pda) = setup_base(&mut svm, program_id, 2);
        assert!(stamp(&mut svm, program_id, &owner, business_pda, &customer, &relayer, 1, Some(0)).is_ok());
        assert!(stamp(&mut svm, program_id, &owner, business_pda, &customer, &relayer, 2, None).is_ok());
        burn_own_card_nft(&mut svm, program_id, business_pda, &customer, &relayer, also_close);

        let res = cash_in(&mut svm, program_id, business_pda, &customer, &relayer, 0, 0);
        assert!(res.is_ok(), "cashing in must not get stuck on a burned card NFT (also_close={also_close}): {:?}", res);
        let card_pda = card_pda_for(program_id, business_pda, customer.pubkey());
        assert!(is_gone(&svm, card_mint_for(program_id, card_pda, 0)), "the empty mint should be closed");
        assert_eq!(card_state(&svm, program_id, business_pda, customer.pubkey()).nft_cycle, 1);
    }
}

#[test]
fn test_second_nft_for_the_same_cycle_fails() {
    let program_id = loyalty::id();
    let mut svm = LiteSVM::new();
    let (owner, customer, relayer, business_pda) = setup_base(&mut svm, program_id, 2);
    assert!(stamp(&mut svm, program_id, &owner, business_pda, &customer, &relayer, 1, Some(0)).is_ok());

    let again = mint_card_nft_ix(program_id, business_pda, customer.pubkey(), relayer.pubkey(), 0, URI);
    let res = send(&mut svm, &[again], &relayer, &[&customer, &relayer]);
    assert_fails_with(res, "already in use");
}

#[test]
fn test_sending_lamports_to_the_mint_address_does_not_block_the_nft() {
    let program_id = loyalty::id();
    let mut svm = LiteSVM::new();
    let (owner, customer, relayer, business_pda) = setup_base(&mut svm, program_id, 2);

    // Anyone can do this in advance, since the address is easy to work out.
    let card_pda = card_pda_for(program_id, business_pda, customer.pubkey());
    let mint_pda = card_mint_for(program_id, card_pda, 0);
    svm.airdrop(&mint_pda, 1).unwrap();

    let res = stamp(&mut svm, program_id, &owner, business_pda, &customer, &relayer, 1, Some(0));
    assert!(res.is_ok(), "a pre-funded mint address must not stop the NFT: {:?}", res);
    let mint_account = svm.get_account(&mint_pda).unwrap();
    assert_eq!(mint_account.owner, spl_token_2022::ID);
    assert_eq!(StateWithExtensions::<MintState>::unpack(&mint_account.data).unwrap().base.supply, 1);
}

#[test]
fn test_nobody_can_mint_a_card_nft_for_someone_elses_card() {
    let program_id = loyalty::id();
    let mut svm = LiteSVM::new();
    let (owner, customer, relayer, business_pda) = setup_base(&mut svm, program_id, 2);
    assert!(stamp(&mut svm, program_id, &owner, business_pda, &customer, &relayer, 1, None).is_ok());

    // An outsider signs as themselves but points at the real customer's card.
    let outsider = Keypair::new();
    let victim_card = card_pda_for(program_id, business_pda, customer.pubkey());
    let mint = card_mint_for(program_id, victim_card, 0);
    let ix = Instruction::new_with_bytes(
        program_id,
        &loyalty::instruction::MintCardNft { uri: URI.to_string() }.data(),
        loyalty::accounts::MintCardNft {
            business: business_pda,
            card: victim_card,
            mint,
            customer_token: ata(outsider.pubkey(), mint),
            customer: outsider.pubkey(),
            relayer: relayer.pubkey(),
            token_program: spl_token_2022::ID,
            associated_token_program: anchor_spl::associated_token::ID,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    let res = send(&mut svm, &[ix], &relayer, &[&outsider, &relayer]);
    assert_fails_with(res, "ConstraintSeeds");
}

#[test]
fn test_card_nft_uri_too_long_fails() {
    let program_id = loyalty::id();
    let mut svm = LiteSVM::new();
    let (owner, customer, relayer, business_pda) = setup_base(&mut svm, program_id, 2);
    let claim = issue_and_claim_ixs(&mut svm, program_id, &owner, business_pda, &customer, &relayer, 1);
    let long_uri = "https://passdari.example/".to_string() + &"x".repeat(100);
    let nft = mint_card_nft_ix(program_id, business_pda, customer.pubkey(), relayer.pubkey(), 0, &long_uri);
    let res = send(&mut svm, &[claim, nft], &relayer, &[&customer, &relayer]);
    assert_fails_with(res, "UriTooLong");
}

#[test]
fn test_cashing_in_cannot_dodge_the_burn_with_another_mint() {
    let program_id = loyalty::id();
    let mut svm = LiteSVM::new();
    let (owner, customer, relayer, business_pda) = setup_base(&mut svm, program_id, 2);
    assert!(stamp(&mut svm, program_id, &owner, business_pda, &customer, &relayer, 1, Some(0)).is_ok());
    assert!(stamp(&mut svm, program_id, &owner, business_pda, &customer, &relayer, 2, None).is_ok());

    // Naming an address where no card NFT exists would skip the burn, so the address has to match the card's real one.
    let card_pda = card_pda_for(program_id, business_pda, customer.pubkey());
    let wrong_mint = card_mint_for(program_id, card_pda, 7);
    let ix = mint_voucher_ix(program_id, business_pda, customer.pubkey(), relayer.pubkey(), 0, wrong_mint, ata(customer.pubkey(), wrong_mint));
    let res = send(&mut svm, &[ix], &relayer, &[&customer, &relayer]);
    assert_fails_with(res, "ConstraintSeeds");

    // The right mint with some other token account is refused too.
    let real_mint = card_mint_for(program_id, card_pda, 0);
    let ix = mint_voucher_ix(program_id, business_pda, customer.pubkey(), relayer.pubkey(), 0, real_mint, ata(Keypair::new().pubkey(), real_mint));
    let res = send(&mut svm, &[ix], &relayer, &[&customer, &relayer]);
    assert_fails_with(res, "ConstraintAddress");

    assert!(!is_gone(&svm, real_mint), "the card NFT is untouched by the refused attempts");
}

#[test]
fn test_relayer_gets_all_the_card_nft_rent_back() {
    let program_id = loyalty::id();
    // Two identical shops and customers. One card gets an NFT, the other doesn't.
    let mut with_nft = LiteSVM::new();
    let mut without_nft = LiteSVM::new();
    let (owner_a, customer_a, relayer_a, business_a) = setup_base(&mut with_nft, program_id, 2);
    let (owner_b, customer_b, relayer_b, business_b) = setup_base(&mut without_nft, program_id, 2);

    let before_a = with_nft.get_balance(&relayer_a.pubkey()).unwrap();
    let before_b = without_nft.get_balance(&relayer_b.pubkey()).unwrap();
    assert!(stamp(&mut with_nft, program_id, &owner_a, business_a, &customer_a, &relayer_a, 1, Some(0)).is_ok());
    assert!(stamp(&mut without_nft, program_id, &owner_b, business_b, &customer_b, &relayer_b, 1, None).is_ok());
    let nft_cost = (before_a - with_nft.get_balance(&relayer_a.pubkey()).unwrap())
        - (before_b - without_nft.get_balance(&relayer_b.pubkey()).unwrap());
    println!("a card NFT costs the relayer {nft_cost} lamports while it exists");
    assert!(nft_cost > 0);

    assert!(stamp(&mut with_nft, program_id, &owner_a, business_a, &customer_a, &relayer_a, 2, None).is_ok());
    assert!(stamp(&mut without_nft, program_id, &owner_b, business_b, &customer_b, &relayer_b, 2, None).is_ok());
    assert!(cash_in(&mut with_nft, program_id, business_a, &customer_a, &relayer_a, 0, 0).is_ok());
    assert!(cash_in(&mut without_nft, program_id, business_b, &customer_b, &relayer_b, 0, 0).is_ok());

    assert_eq!(
        with_nft.get_balance(&relayer_a.pubkey()).unwrap(),
        without_nft.get_balance(&relayer_b.pubkey()).unwrap(),
        "after cashing in, the card NFT should have cost the relayer nothing"
    );
}
