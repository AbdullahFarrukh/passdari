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
                mint_close_authority::MintCloseAuthority, permanent_delegate::PermanentDelegate, BaseStateWithExtensions,
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

const URI: &str = "https://passdari.example/v/0.json";

fn send(svm: &mut LiteSVM, ixs: &[Instruction], payer: &Keypair, signers: &[&Keypair]) -> TransactionResult {
    // LiteSVM refuses two identical transactions that share a blockhash
    // ("AlreadyProcessed"), which would hide the real reason a repeat fails.
    svm.expire_blockhash();
    let blockhash = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(ixs, Some(&payer.pubkey()), &blockhash);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), signers).unwrap();
    svm.send_transaction(tx)
}

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
    assert!(send(svm, &[instruction], relayer, &[owner, relayer]).is_ok(), "setup registration should succeed");
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
    assert!(send(svm, &[issue_ix], relayer, &[owner, relayer]).is_ok(), "setup issue should succeed");

    let claim_ix = Instruction::new_with_bytes(
        program_id,
        &loyalty::instruction::ClaimReceipt { secret }.data(),
        loyalty::accounts::ClaimReceipt {
            business: business_pda,
            receipt: receipt_pda,
            card: card_pda_for(program_id, business_pda, customer.pubkey()),
            customer: customer.pubkey(),
            relayer: relayer.pubkey(),
            rent_payer: relayer.pubkey(),
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    assert!(send(svm, &[claim_ix], relayer, &[customer, relayer]).is_ok(), "setup claim should succeed");
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
        warp(svm, 61);
    }
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

fn voucher_pda_for(program_id: Pubkey, business_pda: Pubkey, voucher_id: u64) -> Pubkey {
    let (voucher_pda, _) = Pubkey::find_program_address(
        &[b"voucher", business_pda.as_ref(), &voucher_id.to_le_bytes()],
        &program_id,
    );
    voucher_pda
}

fn mint_pda_for(program_id: Pubkey, business_pda: Pubkey, voucher_id: u64) -> Pubkey {
    let (mint_pda, _) = Pubkey::find_program_address(
        &[b"voucher_mint", business_pda.as_ref(), &voucher_id.to_le_bytes()],
        &program_id,
    );
    mint_pda
}

fn ata(wallet: Pubkey, mint: Pubkey) -> Pubkey {
    get_associated_token_address_with_program_id(&wallet, &mint, &spl_token_2022::ID)
}

fn mint_voucher_ix(program_id: Pubkey, business_pda: Pubkey, customer: Pubkey, relayer: Pubkey, voucher_id: u64, uri: &str) -> Instruction {
    let mint = mint_pda_for(program_id, business_pda, voucher_id);
    // These cards have no NFT (they were only stamped), so the card-NFT accounts point at addresses
    // where nothing exists yet, and cashing in skips that step.
    let card = card_pda_for(program_id, business_pda, customer);
    let card_mint = Pubkey::find_program_address(&[b"card_mint", card.as_ref(), &0u32.to_le_bytes()], &program_id).0;
    Instruction::new_with_bytes(
        program_id,
        &loyalty::instruction::MintVoucher { voucher_id, uri: uri.to_string() }.data(),
        loyalty::accounts::MintVoucher {
            business: business_pda,
            card,
            voucher: voucher_pda_for(program_id, business_pda, voucher_id),
            mint,
            customer_token: ata(customer, mint),
            card_mint,
            card_token: ata(customer, card_mint),
            card_nft_record: Pubkey::find_program_address(&[b"card_nft", card_mint.as_ref()], &program_id).0,
            card_rent_payer: relayer,
            customer,
            relayer,
            token_program: spl_token_2022::ID,
            associated_token_program: anchor_spl::associated_token::ID,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

fn present_ix(program_id: Pubkey, voucher: Pubkey, mint: Pubkey, holder_token: Pubkey, owner: Pubkey) -> Instruction {
    Instruction::new_with_bytes(
        program_id,
        &loyalty::instruction::PresentVoucher {}.data(),
        loyalty::accounts::PresentVoucher { voucher, mint, holder_token, owner, token_program: spl_token_2022::ID }
            .to_account_metas(None),
    )
}

fn cancel_ix(program_id: Pubkey, voucher: Pubkey, mint: Pubkey, holder_token: Pubkey, owner: Pubkey) -> Instruction {
    Instruction::new_with_bytes(
        program_id,
        &loyalty::instruction::CancelPresentation {}.data(),
        loyalty::accounts::CancelPresentation { voucher, mint, holder_token, owner, token_program: spl_token_2022::ID }
            .to_account_metas(None),
    )
}

fn transfer_ix(program_id: Pubkey, voucher: Pubkey, mint: Pubkey, from_owner: Pubkey, new_owner: Pubkey, relayer: Pubkey) -> Instruction {
    Instruction::new_with_bytes(
        program_id,
        &loyalty::instruction::TransferVoucher {}.data(),
        loyalty::accounts::TransferVoucher {
            voucher,
            mint,
            from_token: ata(from_owner, mint),
            to_token: ata(new_owner, mint),
            new_owner,
            owner: from_owner,
            relayer,
            rent_payer: relayer,
            token_program: spl_token_2022::ID,
            associated_token_program: anchor_spl::associated_token::ID,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

fn redeem_ix(program_id: Pubkey, business: Pubkey, voucher: Pubkey, mint: Pubkey, holder_token: Pubkey, authority: Pubkey, relayer: Pubkey) -> Instruction {
    Instruction::new_with_bytes(
        program_id,
        &loyalty::instruction::RedeemVoucher {}.data(),
        loyalty::accounts::RedeemVoucher {
            business,
            voucher,
            mint,
            holder_token,
            authority,
            relayer,
            rent_payer: relayer,
            token_program: spl_token_2022::ID,
        }
        .to_account_metas(None),
    )
}

/// A plain Token-2022 transfer, the way a wallet like Phantom would do it,
/// without going through this program at all.
fn wallet_transfer_ix(mint: Pubkey, from: Pubkey, to: Pubkey, authority: Pubkey) -> Instruction {
    spl_token_2022::instruction::transfer_checked(&spl_token_2022::ID, &from, &mint, &to, &authority, &[], 1, 0).unwrap()
}

/// Mints voucher `voucher_id` for `customer` and returns (voucher, mint).
fn mint(
    svm: &mut LiteSVM,
    program_id: Pubkey,
    business_pda: Pubkey,
    customer: &Keypair,
    relayer: &Keypair,
    voucher_id: u64,
) -> (Pubkey, Pubkey) {
    let ix = mint_voucher_ix(program_id, business_pda, customer.pubkey(), relayer.pubkey(), voucher_id, URI);
    let res = send(svm, &[ix], relayer, &[customer, relayer]);
    assert!(res.is_ok(), "setup mint should succeed: {:?}", res);
    (
        voucher_pda_for(program_id, business_pda, voucher_id),
        mint_pda_for(program_id, business_pda, voucher_id),
    )
}

fn present(svm: &mut LiteSVM, program_id: Pubkey, voucher: Pubkey, mint: Pubkey, holder: &Keypair) {
    let ix = present_ix(program_id, voucher, mint, ata(holder.pubkey(), mint), holder.pubkey());
    let res = send(svm, &[ix], holder, &[holder]);
    assert!(res.is_ok(), "setup present should succeed: {:?}", res);
}

fn token_state(svm: &LiteSVM, address: Pubkey) -> TokenState {
    let account = svm.get_account(&address).expect("token account should exist");
    StateWithExtensions::<TokenState>::unpack(&account.data).unwrap().base
}

fn voucher_state(svm: &LiteSVM, address: Pubkey) -> loyalty::Voucher {
    let account = svm.get_account(&address).expect("voucher account should exist");
    loyalty::Voucher::try_deserialize(&mut account.data.as_slice()).unwrap()
}

fn business_state(svm: &LiteSVM, address: Pubkey) -> loyalty::Business {
    let account = svm.get_account(&address).expect("business account should exist");
    loyalty::Business::try_deserialize(&mut account.data.as_slice()).unwrap()
}

fn card_stamps(svm: &LiteSVM, address: Pubkey) -> u8 {
    let account = svm.get_account(&address).expect("card account should exist");
    loyalty::LoyaltyCard::try_deserialize(&mut account.data.as_slice()).unwrap().stamps
}

#[test]
fn test_mint_creates_a_real_nft() {
    let program_id = loyalty::id();
    let mut svm = LiteSVM::new();
    let (owner, customer, relayer, business_pda) = setup_base(&mut svm, program_id, 2);

    fill_card(&mut svm, program_id, &owner, business_pda, &customer, &relayer, 3);

    let ix = mint_voucher_ix(program_id, business_pda, customer.pubkey(), relayer.pubkey(), 0, URI);
    let res = send(&mut svm, &[ix], &relayer, &[&customer, &relayer]);
    assert!(res.is_ok(), "mint should succeed: {:?}", res);
    println!("mint_voucher used {} compute units", res.unwrap().compute_units_consumed);

    let voucher_pda = voucher_pda_for(program_id, business_pda, 0);
    let mint_pda = mint_pda_for(program_id, business_pda, 0);

    let mint_account = svm.get_account(&mint_pda).expect("mint should exist");
    assert_eq!(mint_account.owner, spl_token_2022::ID, "the mint should belong to Token-2022");
    let mint_data = StateWithExtensions::<MintState>::unpack(&mint_account.data).unwrap();
    assert_eq!(mint_data.base.supply, 1, "exactly one token should exist");
    assert_eq!(mint_data.base.decimals, 0, "an NFT has no decimals");
    assert!(mint_data.base.mint_authority.is_none(), "minting must be locked after the first token");
    assert_eq!(mint_data.base.freeze_authority.unwrap(), voucher_pda, "the voucher account should be the freeze authority");
    let delegate = mint_data.get_extension::<PermanentDelegate>().unwrap().delegate;
    assert_eq!(Option::<Pubkey>::from(delegate), Some(voucher_pda), "the voucher account should be the permanent delegate");
    let closer = mint_data.get_extension::<MintCloseAuthority>().unwrap().close_authority;
    assert_eq!(Option::<Pubkey>::from(closer), Some(voucher_pda), "the voucher account should be able to close the burned mint");
    assert_eq!(voucher_state(&svm, voucher_pda).rent_payer, relayer.pubkey(), "the voucher records who paid for it");

    let metadata = mint_data.get_variable_len_extension::<TokenMetadata>().unwrap();
    assert_eq!(metadata.name, "Coffee Corner - Free coffee");
    assert_eq!(metadata.symbol, "PSDR");
    assert_eq!(metadata.uri, URI);
    assert_eq!(metadata.mint, mint_pda);

    let token = token_state(&svm, ata(customer.pubkey(), mint_pda));
    assert_eq!(token.amount, 1, "the customer should hold the token");
    assert_eq!(token.owner, customer.pubkey());
    assert!(!token.is_frozen(), "a fresh voucher should not be frozen");

    let voucher = voucher_state(&svm, voucher_pda);
    assert_eq!(voucher.mint, mint_pda);
    assert_eq!(voucher.owner, customer.pubkey());
    assert_eq!(voucher.business, business_pda);

    assert_eq!(card_stamps(&svm, card_pda_for(program_id, business_pda, customer.pubkey())), 1, "2 of the 3 stamps should be spent");
    assert_eq!(business_state(&svm, business_pda).total_vouchers_issued, 1);
}

#[test]
fn test_mint_with_too_long_uri_fails() {
    let program_id = loyalty::id();
    let mut svm = LiteSVM::new();
    let (owner, customer, relayer, business_pda) = setup_base(&mut svm, program_id, 1);
    fill_card(&mut svm, program_id, &owner, business_pda, &customer, &relayer, 1);

    let long_uri = "a".repeat(101);
    let ix = mint_voucher_ix(program_id, business_pda, customer.pubkey(), relayer.pubkey(), 0, &long_uri);
    let res = send(&mut svm, &[ix], &relayer, &[&customer, &relayer]);
    assert_fails_with(res, "UriTooLong");
}

#[test]
fn test_mint_voucher_below_threshold_fails() {
    let program_id = loyalty::id();
    let mut svm = LiteSVM::new();
    let (owner, customer, relayer, business_pda) = setup_base(&mut svm, program_id, 5);

    fill_card(&mut svm, program_id, &owner, business_pda, &customer, &relayer, 2);

    let ix = mint_voucher_ix(program_id, business_pda, customer.pubkey(), relayer.pubkey(), 0, URI);
    let res = send(&mut svm, &[ix], &relayer, &[&customer, &relayer]);
    assert_fails_with(res, "NotEnoughStamps");
}

#[test]
fn test_raising_threshold_does_not_void_earned_reward() {
    let program_id = loyalty::id();
    let mut svm = LiteSVM::new();
    let (owner, customer, relayer, business_pda) = setup_base(&mut svm, program_id, 5);

    fill_card(&mut svm, program_id, &owner, business_pda, &customer, &relayer, 5);

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
    assert!(send(&mut svm, &[update_ix], &owner, &[&owner]).is_ok(), "raising the threshold should succeed");

    let ix = mint_voucher_ix(program_id, business_pda, customer.pubkey(), relayer.pubkey(), 0, URI);
    let res = send(&mut svm, &[ix], &relayer, &[&customer, &relayer]);
    assert!(
        res.is_ok(),
        "a customer who already earned the original threshold must still be able to mint, even after the merchant raises it: {:?}",
        res
    );
}

#[test]
fn test_present_freezes_and_cancel_thaws() {
    let program_id = loyalty::id();
    let mut svm = LiteSVM::new();
    let (owner, customer, relayer, business_pda) = setup_base(&mut svm, program_id, 1);
    fill_card(&mut svm, program_id, &owner, business_pda, &customer, &relayer, 1);
    let (voucher_pda, mint_pda) = mint(&mut svm, program_id, business_pda, &customer, &relayer, 0);
    let holder_token = ata(customer.pubkey(), mint_pda);

    let cancel_early = cancel_ix(program_id, voucher_pda, mint_pda, holder_token, customer.pubkey());
    let res = send(&mut svm, &[cancel_early], &customer, &[&customer]);
    assert_fails_with(res, "VoucherNotPresented");

    present(&mut svm, program_id, voucher_pda, mint_pda, &customer);
    assert!(token_state(&svm, holder_token).is_frozen(), "presenting should freeze the token account");

    let present_again = present_ix(program_id, voucher_pda, mint_pda, holder_token, customer.pubkey());
    let res = send(&mut svm, &[present_again], &customer, &[&customer]);
    assert_fails_with(res, "VoucherPending");

    let cancel = cancel_ix(program_id, voucher_pda, mint_pda, holder_token, customer.pubkey());
    let res = send(&mut svm, &[cancel], &customer, &[&customer]);
    assert!(res.is_ok(), "cancelling should succeed: {:?}", res);
    assert!(!token_state(&svm, holder_token).is_frozen(), "cancelling should thaw the token account");
}

#[test]
fn test_present_by_someone_else_fails() {
    let program_id = loyalty::id();
    let mut svm = LiteSVM::new();
    let (owner, customer, relayer, business_pda) = setup_base(&mut svm, program_id, 1);
    let attacker = Keypair::new();
    svm.airdrop(&attacker.pubkey(), 1_000_000_000).unwrap();

    fill_card(&mut svm, program_id, &owner, business_pda, &customer, &relayer, 1);
    let (voucher_pda, mint_pda) = mint(&mut svm, program_id, business_pda, &customer, &relayer, 0);
    let holder_token = ata(customer.pubkey(), mint_pda);

    let ix = present_ix(program_id, voucher_pda, mint_pda, holder_token, attacker.pubkey());
    let res = send(&mut svm, &[ix], &attacker, &[&attacker]);
    assert_fails_with(res, "ConstraintTokenOwner");
    assert!(!token_state(&svm, holder_token).is_frozen());
}

#[test]
fn test_transfer_moves_the_token() {
    let program_id = loyalty::id();
    let mut svm = LiteSVM::new();
    let (owner, customer, relayer, business_pda) = setup_base(&mut svm, program_id, 1);
    let recipient = Keypair::new();

    fill_card(&mut svm, program_id, &owner, business_pda, &customer, &relayer, 1);
    let (voucher_pda, mint_pda) = mint(&mut svm, program_id, business_pda, &customer, &relayer, 0);

    let relayer_before = svm.get_balance(&relayer.pubkey()).unwrap();
    let ix = transfer_ix(program_id, voucher_pda, mint_pda, customer.pubkey(), recipient.pubkey(), relayer.pubkey());
    let res = send(&mut svm, &[ix], &relayer, &[&customer, &relayer]);
    assert!(res.is_ok(), "transfer should succeed: {:?}", res);

    assert!(is_gone(&svm, ata(customer.pubkey(), mint_pda)), "the sender's empty token account should be closed");
    // The relayer paid for the recipient's account and got the sender's back, so only the fee is spent.
    assert_eq!(relayer_before - svm.get_balance(&relayer.pubkey()).unwrap(), 2 * FEE_PER_SIGNATURE);
    let received = token_state(&svm, ata(recipient.pubkey(), mint_pda));
    assert_eq!(received.amount, 1, "the recipient should now hold the token");
    assert_eq!(received.owner, recipient.pubkey());
    assert_eq!(voucher_state(&svm, voucher_pda).owner, recipient.pubkey(), "the owner hint should follow a transfer");
    assert!(svm.get_balance(&recipient.pubkey()).unwrap_or(0) == 0, "the recipient should not have had to pay anything");
}

#[test]
fn test_old_owner_locked_out_after_transfer() {
    let program_id = loyalty::id();
    let mut svm = LiteSVM::new();
    let (owner, customer, relayer, business_pda) = setup_base(&mut svm, program_id, 1);
    let new_owner = Keypair::new();
    svm.airdrop(&new_owner.pubkey(), 1_000_000_000).unwrap();

    fill_card(&mut svm, program_id, &owner, business_pda, &customer, &relayer, 1);
    let (voucher_pda, mint_pda) = mint(&mut svm, program_id, business_pda, &customer, &relayer, 0);

    let transfer = transfer_ix(program_id, voucher_pda, mint_pda, customer.pubkey(), new_owner.pubkey(), relayer.pubkey());
    assert!(send(&mut svm, &[transfer], &relayer, &[&customer, &relayer]).is_ok(), "setup transfer should succeed");

    let old_present = present_ix(program_id, voucher_pda, mint_pda, ata(customer.pubkey(), mint_pda), customer.pubkey());
    let res = send(&mut svm, &[old_present], &customer, &[&customer]);
    // The old owner's token account was closed when they gave the voucher away.
    assert_fails_with(res, "caused by account: holder_token. Error Code: AccountNotInitialized");

    // Trying to use the new holder's token account while signing as the old owner.
    let borrowed_present = present_ix(program_id, voucher_pda, mint_pda, ata(new_owner.pubkey(), mint_pda), customer.pubkey());
    let res = send(&mut svm, &[borrowed_present], &customer, &[&customer]);
    assert_fails_with(res, "ConstraintTokenOwner");

    let transfer_back = transfer_ix(program_id, voucher_pda, mint_pda, customer.pubkey(), owner.pubkey(), relayer.pubkey());
    let res = send(&mut svm, &[transfer_back], &relayer, &[&customer, &relayer]);
    assert_fails_with(res, "caused by account: from_token. Error Code: AccountNotInitialized");
}

#[test]
fn test_transfer_while_pending_fails() {
    let program_id = loyalty::id();
    let mut svm = LiteSVM::new();
    let (owner, customer, relayer, business_pda) = setup_base(&mut svm, program_id, 1);
    let recipient = Keypair::new();

    fill_card(&mut svm, program_id, &owner, business_pda, &customer, &relayer, 1);
    let (voucher_pda, mint_pda) = mint(&mut svm, program_id, business_pda, &customer, &relayer, 0);
    present(&mut svm, program_id, voucher_pda, mint_pda, &customer);

    let ix = transfer_ix(program_id, voucher_pda, mint_pda, customer.pubkey(), recipient.pubkey(), relayer.pubkey());
    let res = send(&mut svm, &[ix], &relayer, &[&customer, &relayer]);
    assert_fails_with(res, "VoucherPending");
    assert_eq!(token_state(&svm, ata(customer.pubkey(), mint_pda)).amount, 1, "the customer should still hold it");
}

#[test]
fn test_wallet_cannot_move_a_presented_voucher() {
    let program_id = loyalty::id();
    let mut svm = LiteSVM::new();
    let (owner, customer, relayer, business_pda) = setup_base(&mut svm, program_id, 1);
    let recipient = Keypair::new();

    fill_card(&mut svm, program_id, &owner, business_pda, &customer, &relayer, 1);
    let (voucher_pda, mint_pda) = mint(&mut svm, program_id, business_pda, &customer, &relayer, 0);
    present(&mut svm, program_id, voucher_pda, mint_pda, &customer);

    // Skip this program and use a plain Token-2022 transfer, like a wallet would.
    let create_ata = create_associated_token_account(&relayer.pubkey(), &recipient.pubkey(), &mint_pda, &spl_token_2022::ID);
    let move_token = wallet_transfer_ix(mint_pda, ata(customer.pubkey(), mint_pda), ata(recipient.pubkey(), mint_pda), customer.pubkey());
    let res = send(&mut svm, &[create_ata, move_token], &relayer, &[&customer, &relayer]);
    assert_fails_with(res, "Account is frozen");
    assert_eq!(token_state(&svm, ata(customer.pubkey(), mint_pda)).amount, 1);
}

#[test]
fn test_wallet_transfer_then_new_holder_can_redeem() {
    let program_id = loyalty::id();
    let mut svm = LiteSVM::new();
    let (owner, customer, relayer, business_pda) = setup_base(&mut svm, program_id, 1);
    let buyer = Keypair::new();
    svm.airdrop(&buyer.pubkey(), 1_000_000_000).unwrap();

    fill_card(&mut svm, program_id, &owner, business_pda, &customer, &relayer, 1);
    let (voucher_pda, mint_pda) = mint(&mut svm, program_id, business_pda, &customer, &relayer, 0);

    // The voucher moves without this program knowing, so the owner hint goes stale.
    let create_ata = create_associated_token_account(&relayer.pubkey(), &buyer.pubkey(), &mint_pda, &spl_token_2022::ID);
    let move_token = wallet_transfer_ix(mint_pda, ata(customer.pubkey(), mint_pda), ata(buyer.pubkey(), mint_pda), customer.pubkey());
    let res = send(&mut svm, &[create_ata, move_token], &relayer, &[&customer, &relayer]);
    assert!(res.is_ok(), "an unpresented voucher can be moved by a wallet: {:?}", res);
    assert_eq!(voucher_state(&svm, voucher_pda).owner, customer.pubkey(), "the hint is stale, as expected");

    // The token decides, not the hint: the new holder presents and it gets redeemed.
    present(&mut svm, program_id, voucher_pda, mint_pda, &buyer);
    let redeem = redeem_ix(program_id, business_pda, voucher_pda, mint_pda, ata(buyer.pubkey(), mint_pda), owner.pubkey(), relayer.pubkey());
    let res = send(&mut svm, &[redeem], &relayer, &[&owner, &relayer]);
    assert!(res.is_ok(), "the new holder's voucher should redeem: {:?}", res);
}

#[test]
fn test_redeem_burns_the_nft_and_closes_the_voucher() {
    let program_id = loyalty::id();
    let mut svm = LiteSVM::new();
    let (owner, customer, relayer, business_pda) = setup_base(&mut svm, program_id, 1);

    fill_card(&mut svm, program_id, &owner, business_pda, &customer, &relayer, 1);
    let (voucher_pda, mint_pda) = mint(&mut svm, program_id, business_pda, &customer, &relayer, 0);
    present(&mut svm, program_id, voucher_pda, mint_pda, &customer);

    let holder_token = ata(customer.pubkey(), mint_pda);
    let redeem = redeem_ix(program_id, business_pda, voucher_pda, mint_pda, holder_token, owner.pubkey(), relayer.pubkey());
    let res = send(&mut svm, &[redeem], &relayer, &[&owner, &relayer]);
    assert!(res.is_ok(), "redeem should succeed: {:?}", res);
    let meta = res.unwrap();
    println!("redeem_voucher used {} compute units", meta.compute_units_consumed);

    // The burn is the proof the voucher was used, and it stays in the chain's history.
    assert!(meta.logs.iter().any(|l| l.contains("Instruction: BurnChecked")), "the token should be burned");
    // Then every empty account is closed, so its rent goes back to whoever paid for it.
    let gone = |a: Pubkey| svm.get_account(&a).map_or(true, |acc| acc.lamports == 0);
    assert!(gone(mint_pda), "the burned NFT's mint should be closed");
    assert!(gone(holder_token), "the holder's empty token account should be closed");
    assert!(gone(voucher_pda), "the voucher account should be closed");
    assert_eq!(business_state(&svm, business_pda).total_redemptions, 1);
}

#[test]
fn test_redeem_does_not_touch_card_stamps() {
    let program_id = loyalty::id();
    let mut svm = LiteSVM::new();
    let (owner, customer, relayer, business_pda) = setup_base(&mut svm, program_id, 2);

    fill_card(&mut svm, program_id, &owner, business_pda, &customer, &relayer, 3);
    let card_pda = card_pda_for(program_id, business_pda, customer.pubkey());

    let (voucher_pda, mint_pda) = mint(&mut svm, program_id, business_pda, &customer, &relayer, 0);
    present(&mut svm, program_id, voucher_pda, mint_pda, &customer);
    let stamps_before = card_stamps(&svm, card_pda);

    let redeem = redeem_ix(program_id, business_pda, voucher_pda, mint_pda, ata(customer.pubkey(), mint_pda), owner.pubkey(), relayer.pubkey());
    let res = send(&mut svm, &[redeem], &relayer, &[&owner, &relayer]);
    assert!(res.is_ok(), "redeem should succeed: {:?}", res);

    assert_eq!(
        stamps_before, card_stamps(&svm, card_pda),
        "redeeming a voucher must never change the card's stamp count"
    );
}

#[test]
fn test_redeem_gifted_voucher_without_a_card() {
    let program_id = loyalty::id();
    let mut svm = LiteSVM::new();
    let (owner, customer, relayer, business_pda) = setup_base(&mut svm, program_id, 1);
    let stranger = Keypair::new();
    svm.airdrop(&stranger.pubkey(), 1_000_000_000).unwrap();

    fill_card(&mut svm, program_id, &owner, business_pda, &customer, &relayer, 1);
    let (voucher_pda, mint_pda) = mint(&mut svm, program_id, business_pda, &customer, &relayer, 0);

    let gift = transfer_ix(program_id, voucher_pda, mint_pda, customer.pubkey(), stranger.pubkey(), relayer.pubkey());
    assert!(send(&mut svm, &[gift], &relayer, &[&customer, &relayer]).is_ok(), "setup gift should succeed");

    let stranger_card = card_pda_for(program_id, business_pda, stranger.pubkey());
    assert!(svm.get_account(&stranger_card).is_none(), "the stranger has never earned a stamp here");

    present(&mut svm, program_id, voucher_pda, mint_pda, &stranger);
    let redeem = redeem_ix(program_id, business_pda, voucher_pda, mint_pda, ata(stranger.pubkey(), mint_pda), owner.pubkey(), relayer.pubkey());
    let res = send(&mut svm, &[redeem], &relayer, &[&owner, &relayer]);
    assert!(res.is_ok(), "a gifted voucher must redeem even if the holder has no card: {:?}", res);
    assert_eq!(business_state(&svm, business_pda).total_redemptions, 1, "the redemption is counted on the business");
}

#[test]
fn test_redeem_unpresented_voucher_fails() {
    let program_id = loyalty::id();
    let mut svm = LiteSVM::new();
    let (owner, customer, relayer, business_pda) = setup_base(&mut svm, program_id, 1);

    fill_card(&mut svm, program_id, &owner, business_pda, &customer, &relayer, 1);
    let (voucher_pda, mint_pda) = mint(&mut svm, program_id, business_pda, &customer, &relayer, 0);

    let redeem = redeem_ix(program_id, business_pda, voucher_pda, mint_pda, ata(customer.pubkey(), mint_pda), owner.pubkey(), relayer.pubkey());
    let res = send(&mut svm, &[redeem], &relayer, &[&owner, &relayer]);
    assert_fails_with(res, "VoucherNotPresented");
    assert_eq!(token_state(&svm, ata(customer.pubkey(), mint_pda)).amount, 1, "the token must not be burned");
}

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
    let (voucher_pda, mint_pda) = mint(&mut svm, program_id, business_a, &customer, &relayer, 0);
    present(&mut svm, program_id, voucher_pda, mint_pda, &customer);

    let redeem = redeem_ix(program_id, business_b, voucher_pda, mint_pda, ata(customer.pubkey(), mint_pda), owner_b.pubkey(), relayer.pubkey());
    let res = send(&mut svm, &[redeem], &relayer, &[&owner_b, &relayer]);
    assert_fails_with(res, "ConstraintHasOne");
    assert_eq!(token_state(&svm, ata(customer.pubkey(), mint_pda)).amount, 1, "the token must not be burned");
}

#[test]
fn test_redeem_twice_fails() {
    let program_id = loyalty::id();
    let mut svm = LiteSVM::new();
    let (owner, customer, relayer, business_pda) = setup_base(&mut svm, program_id, 1);

    fill_card(&mut svm, program_id, &owner, business_pda, &customer, &relayer, 1);
    let (voucher_pda, mint_pda) = mint(&mut svm, program_id, business_pda, &customer, &relayer, 0);
    present(&mut svm, program_id, voucher_pda, mint_pda, &customer);

    let holder_token = ata(customer.pubkey(), mint_pda);
    let redeem = || redeem_ix(program_id, business_pda, voucher_pda, mint_pda, holder_token, owner.pubkey(), relayer.pubkey());

    assert!(send(&mut svm, &[redeem()], &relayer, &[&owner, &relayer]).is_ok(), "first redemption should succeed");

    let res = send(&mut svm, &[redeem()], &relayer, &[&owner, &relayer]);
    assert_fails_with(res, "AccountNotInitialized");
}

const FEE_PER_SIGNATURE: u64 = 5_000;

fn is_gone(svm: &LiteSVM, address: Pubkey) -> bool {
    svm.get_account(&address).map_or(true, |a| a.lamports == 0)
}

fn redeem_ix_paying_back(program_id: Pubkey, business: Pubkey, voucher: Pubkey, mint: Pubkey, holder_token: Pubkey, authority: Pubkey, relayer: Pubkey, rent_payer: Pubkey) -> Instruction {
    Instruction::new_with_bytes(
        program_id,
        &loyalty::instruction::RedeemVoucher {}.data(),
        loyalty::accounts::RedeemVoucher { business, voucher, mint, holder_token, authority, relayer, rent_payer, token_program: spl_token_2022::ID }
            .to_account_metas(None),
    )
}

#[test]
fn test_a_whole_voucher_costs_the_relayer_only_fees() {
    let program_id = loyalty::id();
    let mut svm = LiteSVM::new();
    let (owner, customer, relayer, business_pda) = setup_base(&mut svm, program_id, 1);
    fill_card(&mut svm, program_id, &owner, business_pda, &customer, &relayer, 1);

    let relayer_before = svm.get_balance(&relayer.pubkey()).unwrap();
    let merchant_before = svm.get_balance(&owner.pubkey()).unwrap();
    let (voucher_pda, mint_pda) = mint(&mut svm, program_id, business_pda, &customer, &relayer, 0);
    let locked = relayer_before - svm.get_balance(&relayer.pubkey()).unwrap();
    println!("while it exists, a voucher holds {locked} lamports of the relayer's (rent + fee)");

    present(&mut svm, program_id, voucher_pda, mint_pda, &customer);
    let holder_token = ata(customer.pubkey(), mint_pda);
    let redeem = redeem_ix(program_id, business_pda, voucher_pda, mint_pda, holder_token, owner.pubkey(), relayer.pubkey());
    assert!(send(&mut svm, &[redeem], &relayer, &[&owner, &relayer]).is_ok(), "redeem should succeed");

    let spent = relayer_before - svm.get_balance(&relayer.pubkey()).unwrap();
    println!("after redeeming, the voucher cost the relayer {spent} lamports in total");
    assert_eq!(spent, 2 * 2 * FEE_PER_SIGNATURE, "only the mint and redeem fees are spent; all the rent came back");
    assert_eq!(svm.get_balance(&owner.pubkey()).unwrap(), merchant_before, "the merchant gains nothing: they never paid");
}

#[test]
fn test_redeem_cannot_send_the_refund_elsewhere() {
    let program_id = loyalty::id();
    let mut svm = LiteSVM::new();
    let (owner, customer, relayer, business_pda) = setup_base(&mut svm, program_id, 1);
    fill_card(&mut svm, program_id, &owner, business_pda, &customer, &relayer, 1);
    let (voucher_pda, mint_pda) = mint(&mut svm, program_id, business_pda, &customer, &relayer, 0);
    present(&mut svm, program_id, voucher_pda, mint_pda, &customer);

    // The merchant tries to collect the rent the relayer paid.
    let holder_token = ata(customer.pubkey(), mint_pda);
    let ix = redeem_ix_paying_back(program_id, business_pda, voucher_pda, mint_pda, holder_token, owner.pubkey(), relayer.pubkey(), owner.pubkey());
    let res = send(&mut svm, &[ix], &relayer, &[&owner, &relayer]);
    assert_fails_with(res, "ConstraintHasOne");
    assert!(!is_gone(&svm, mint_pda), "nothing was burned or closed");
}

#[test]
fn test_minting_lets_the_voucher_close_the_token_account() {
    let program_id = loyalty::id();
    let mut svm = LiteSVM::new();
    let (owner, customer, relayer, business_pda) = setup_base(&mut svm, program_id, 1);
    fill_card(&mut svm, program_id, &owner, business_pda, &customer, &relayer, 1);
    let (voucher_pda, mint_pda) = mint(&mut svm, program_id, business_pda, &customer, &relayer, 0);
    let holder_token = ata(customer.pubkey(), mint_pda);

    // The customer signs the minting transaction, so the close right is granted right then, not later at
    // present — the voucher (and so a future close-when-expired) doesn't have to wait on the holder ever
    // presenting it.
    let state = token_state(&svm, holder_token);
    assert_eq!(Option::<Pubkey>::from(state.close_authority), Some(voucher_pda), "minting hands the voucher the right to close it");
    assert_eq!(state.owner, customer.pubkey(), "the holder still owns it");
    assert_eq!(state.amount, 1, "and still holds the token");

    present(&mut svm, program_id, voucher_pda, mint_pda, &customer);
    assert_eq!(Option::<Pubkey>::from(token_state(&svm, holder_token).close_authority), Some(voucher_pda), "presenting doesn't change it");
}

#[test]
fn test_gift_after_cancelling_still_returns_the_rent() {
    let program_id = loyalty::id();
    let mut svm = LiteSVM::new();
    let (owner, customer, relayer, business_pda) = setup_base(&mut svm, program_id, 1);
    let recipient = Keypair::new();
    fill_card(&mut svm, program_id, &owner, business_pda, &customer, &relayer, 1);
    let (voucher_pda, mint_pda) = mint(&mut svm, program_id, business_pda, &customer, &relayer, 0);
    let holder_token = ata(customer.pubkey(), mint_pda);

    // Present and cancel: the close right now belongs to the voucher, not the holder.
    present(&mut svm, program_id, voucher_pda, mint_pda, &customer);
    let cancel = cancel_ix(program_id, voucher_pda, mint_pda, holder_token, customer.pubkey());
    assert!(send(&mut svm, &[cancel], &customer, &[&customer]).is_ok(), "cancel should succeed");

    let relayer_before = svm.get_balance(&relayer.pubkey()).unwrap();
    let gift = transfer_ix(program_id, voucher_pda, mint_pda, customer.pubkey(), recipient.pubkey(), relayer.pubkey());
    assert!(send(&mut svm, &[gift], &relayer, &[&customer, &relayer]).is_ok(), "gift should succeed");
    assert!(is_gone(&svm, holder_token), "the sender's empty account is closed by the voucher");
    assert_eq!(relayer_before - svm.get_balance(&relayer.pubkey()).unwrap(), 2 * FEE_PER_SIGNATURE);

    // The recipient can still present and redeem it, and the rent still comes back.
    svm.airdrop(&recipient.pubkey(), 1_000_000_000).unwrap(); // presenting is paid by the holder in these tests
    present(&mut svm, program_id, voucher_pda, mint_pda, &recipient);
    let redeem = redeem_ix(program_id, business_pda, voucher_pda, mint_pda, ata(recipient.pubkey(), mint_pda), owner.pubkey(), relayer.pubkey());
    assert!(send(&mut svm, &[redeem], &relayer, &[&owner, &relayer]).is_ok(), "the recipient's voucher should redeem");
    assert!(is_gone(&svm, mint_pda) && is_gone(&svm, voucher_pda) && is_gone(&svm, ata(recipient.pubkey(), mint_pda)));
}

const NINETY_DAYS: i64 = 90 * 24 * 60 * 60;

fn close_expired_ix(program_id: Pubkey, voucher: Pubkey, mint: Pubkey, holder_token: Pubkey, rent_payer: Pubkey) -> Instruction {
    Instruction::new_with_bytes(
        program_id,
        &loyalty::instruction::CloseExpiredVoucher {}.data(),
        loyalty::accounts::CloseExpiredVoucher { voucher, mint, holder_token, rent_payer, token_program: spl_token_2022::ID }
            .to_account_metas(None),
    )
}

fn funded_stranger(svm: &mut LiteSVM) -> Keypair {
    let stranger = Keypair::new();
    svm.airdrop(&stranger.pubkey(), 1_000_000_000).unwrap();
    stranger
}

#[test]
fn test_voucher_is_valid_for_90_days_from_minting() {
    let program_id = loyalty::id();
    let mut svm = LiteSVM::new();
    let (owner, customer, relayer, business_pda) = setup_base(&mut svm, program_id, 1);
    fill_card(&mut svm, program_id, &owner, business_pda, &customer, &relayer, 1);
    let (voucher_pda, mint_pda) = mint(&mut svm, program_id, business_pda, &customer, &relayer, 0);

    let voucher = voucher_state(&svm, voucher_pda);
    assert_eq!(voucher.expires_at, voucher.minted_at + NINETY_DAYS, "the date is fixed on the voucher when it is minted");
    let token = token_state(&svm, ata(customer.pubkey(), mint_pda));
    assert_eq!(Option::<Pubkey>::from(token.close_authority), Some(voucher_pda), "the customer hands the voucher the close right at minting");
}

#[test]
fn test_expired_voucher_cannot_be_presented_gifted_or_redeemed() {
    let program_id = loyalty::id();
    let mut svm = LiteSVM::new();
    let (owner, customer, relayer, business_pda) = setup_base(&mut svm, program_id, 1);
    fill_card(&mut svm, program_id, &owner, business_pda, &customer, &relayer, 2);
    let (unused, unused_mint) = mint(&mut svm, program_id, business_pda, &customer, &relayer, 0);
    let (presented, presented_mint) = mint(&mut svm, program_id, business_pda, &customer, &relayer, 1);
    present(&mut svm, program_id, presented, presented_mint, &customer);

    // On the last day it still works...
    warp(&mut svm, NINETY_DAYS - 3 * 61);
    let cancel = cancel_ix(program_id, presented, presented_mint, ata(customer.pubkey(), presented_mint), customer.pubkey());
    assert!(send(&mut svm, &[cancel], &customer, &[&customer]).is_ok(), "cancelling works at any time");
    present(&mut svm, program_id, presented, presented_mint, &customer);

    // ...and after it, it doesn't.
    warp(&mut svm, 3 * 61 + 1);
    let res = send(&mut svm, &[present_ix(program_id, unused, unused_mint, ata(customer.pubkey(), unused_mint), customer.pubkey())], &customer, &[&customer]);
    assert_fails_with(res, "VoucherExpired");
    let gift = transfer_ix(program_id, unused, unused_mint, customer.pubkey(), Keypair::new().pubkey(), relayer.pubkey());
    assert_fails_with(send(&mut svm, &[gift], &relayer, &[&customer, &relayer]), "VoucherExpired");
    let redeem = redeem_ix(program_id, business_pda, presented, presented_mint, ata(customer.pubkey(), presented_mint), owner.pubkey(), relayer.pubkey());
    assert_fails_with(send(&mut svm, &[redeem], &relayer, &[&owner, &relayer]), "VoucherExpired");
}

#[test]
fn test_expired_voucher_is_closed_and_all_its_rent_returned() {
    let program_id = loyalty::id();
    let mut svm = LiteSVM::new();
    let (owner, customer, relayer, business_pda) = setup_base(&mut svm, program_id, 1);
    fill_card(&mut svm, program_id, &owner, business_pda, &customer, &relayer, 1);
    let relayer_before = svm.get_balance(&relayer.pubkey()).unwrap();
    let customer_before = svm.get_balance(&customer.pubkey()).unwrap();
    let (voucher_pda, mint_pda) = mint(&mut svm, program_id, business_pda, &customer, &relayer, 0);
    let holder_token = ata(customer.pubkey(), mint_pda);
    let stranger = funded_stranger(&mut svm);

    let early = send(&mut svm, &[close_expired_ix(program_id, voucher_pda, mint_pda, holder_token, relayer.pubkey())], &stranger, &[&stranger]);
    assert_fails_with(early, "VoucherNotExpired");

    warp(&mut svm, NINETY_DAYS + 1);
    let res = send(&mut svm, &[close_expired_ix(program_id, voucher_pda, mint_pda, holder_token, relayer.pubkey())], &stranger, &[&stranger]);
    assert!(res.is_ok(), "anyone can close an expired voucher: {:?}", res);
    assert!(res.unwrap().logs.iter().any(|l| l.contains("Instruction: BurnChecked")), "the token is burned");
    assert!(is_gone(&svm, mint_pda) && is_gone(&svm, holder_token) && is_gone(&svm, voucher_pda), "every account is closed");
    assert_eq!(relayer_before - svm.get_balance(&relayer.pubkey()).unwrap(), 2 * FEE_PER_SIGNATURE, "only the minting fee is spent");
    assert_eq!(svm.get_balance(&customer.pubkey()).unwrap(), customer_before, "the customer gains nothing: they never paid");
}

#[test]
fn test_expired_voucher_left_presented_is_still_closed() {
    let program_id = loyalty::id();
    let mut svm = LiteSVM::new();
    let (owner, customer, relayer, business_pda) = setup_base(&mut svm, program_id, 1);
    fill_card(&mut svm, program_id, &owner, business_pda, &customer, &relayer, 1);
    let (voucher_pda, mint_pda) = mint(&mut svm, program_id, business_pda, &customer, &relayer, 0);
    present(&mut svm, program_id, voucher_pda, mint_pda, &customer);
    let holder_token = ata(customer.pubkey(), mint_pda);

    warp(&mut svm, NINETY_DAYS + 1);
    let res = send(&mut svm, &[close_expired_ix(program_id, voucher_pda, mint_pda, holder_token, relayer.pubkey())], &relayer, &[&relayer]);
    assert!(res.is_ok(), "a frozen voucher is thawed and burned: {:?}", res);
    assert!(is_gone(&svm, mint_pda) && is_gone(&svm, holder_token) && is_gone(&svm, voucher_pda));
}

#[test]
fn test_expired_gift_that_was_never_presented() {
    let program_id = loyalty::id();
    let mut svm = LiteSVM::new();
    let (owner, customer, relayer, business_pda) = setup_base(&mut svm, program_id, 1);
    let recipient = Keypair::new();
    fill_card(&mut svm, program_id, &owner, business_pda, &customer, &relayer, 1);
    let (voucher_pda, mint_pda) = mint(&mut svm, program_id, business_pda, &customer, &relayer, 0);
    let gift = transfer_ix(program_id, voucher_pda, mint_pda, customer.pubkey(), recipient.pubkey(), relayer.pubkey());
    assert!(send(&mut svm, &[gift], &relayer, &[&customer, &relayer]).is_ok());
    let recipient_token = ata(recipient.pubkey(), mint_pda);

    warp(&mut svm, NINETY_DAYS + 1);
    let res = send(&mut svm, &[close_expired_ix(program_id, voucher_pda, mint_pda, recipient_token, relayer.pubkey())], &relayer, &[&relayer]);
    assert!(res.is_ok(), "an expired gift is closed too: {:?}", res);
    assert!(is_gone(&svm, mint_pda) && is_gone(&svm, voucher_pda), "the NFT and the voucher record are closed");
    // The recipient never signed anything, so the program can't close their account; it stays theirs, empty.
    assert_eq!(token_state(&svm, recipient_token).amount, 0);
}

#[test]
fn test_expired_voucher_needs_the_account_that_really_holds_it() {
    let program_id = loyalty::id();
    let mut svm = LiteSVM::new();
    let (owner, customer, relayer, business_pda) = setup_base(&mut svm, program_id, 1);
    fill_card(&mut svm, program_id, &owner, business_pda, &customer, &relayer, 1);
    let (voucher_pda, mint_pda) = mint(&mut svm, program_id, business_pda, &customer, &relayer, 0);

    // An empty account for the same NFT, owned by someone else.
    let stranger = funded_stranger(&mut svm);
    let open = create_associated_token_account(&stranger.pubkey(), &stranger.pubkey(), &mint_pda, &spl_token_2022::ID);
    assert!(send(&mut svm, &[open], &stranger, &[&stranger]).is_ok());

    warp(&mut svm, NINETY_DAYS + 1);
    let res = send(&mut svm, &[close_expired_ix(program_id, voucher_pda, mint_pda, ata(stranger.pubkey(), mint_pda), relayer.pubkey())], &stranger, &[&stranger]);
    assert_fails_with(res, "NotVoucherHolder");
    assert!(!is_gone(&svm, voucher_pda), "nothing was closed while the token still exists");
}

#[test]
fn test_expired_voucher_rent_cannot_be_redirected() {
    let program_id = loyalty::id();
    let mut svm = LiteSVM::new();
    let (owner, customer, relayer, business_pda) = setup_base(&mut svm, program_id, 1);
    fill_card(&mut svm, program_id, &owner, business_pda, &customer, &relayer, 1);
    let (voucher_pda, mint_pda) = mint(&mut svm, program_id, business_pda, &customer, &relayer, 0);
    let stranger = funded_stranger(&mut svm);

    warp(&mut svm, NINETY_DAYS + 1);
    let res = send(&mut svm, &[close_expired_ix(program_id, voucher_pda, mint_pda, ata(customer.pubkey(), mint_pda), stranger.pubkey())], &stranger, &[&stranger]);
    assert_fails_with(res, "ConstraintHasOne");
}
