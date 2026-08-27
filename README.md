
# Loyalty — Solana Receipt dApp (Program)
A multi-tenant, on-chain loyalty stamp-card system. Businesses issue receipts
at purchase; customers scan to claim a stamp; ten stamps mint a transferable
voucher redeemable for a free item.   
OR
A digital replacement for the paper stamp card, built as one dApp serving many businesses. A merchant issues an unclaimed receipt on-chain at the moment of purchase; the customer scans it to claim a stamp; once enough stamps are collected, they mint a transferable voucher for the free item.

**Why blockchain:** the stamp count is forge-proof, permanent, and cannot be quietly erased by either side, and each business's reward terms are published on-chain where they cannot be applied inconsistently to different customers.

This repo holds the on-chain Anchor program. The web frontend lives in a separate repo: [`loyalty-app`](https://github.com/YOUR-USERNAME/loyalty-app).

---

## Environment

| Tool | Version |
|---|---|
| Anchor | `=1.0.2` (pinned exactly — do not let this drift) |
| Rust | (your `rustc --version` output) |
| Solana CLI | (your `solana --version` output) |
| Node.js | LTS, installed via `nvm` |
| Local validator | Surfpool (`anchor localnet` uses this by default under Anchor 1.0) |

**Program ID:** `HWvvvwSEounpNXcbD4JUNmniB5YxTcFNYoAestzJJCuL`

**Network status: local only.** Every test and demo has run against a local Surfpool validator. This program has not yet been deployed to devnet.

---

## Account model

Business — one per merchant wallet, holds reward terms and running counters
LoyaltyCard — one per (business, customer) pair, holds stamp count
Receipt — one per issued QR code, closed on claim
Voucher — one per minted reward, transferable, closed on redemption




There is no `Customer` account — a customer is just a wallet. See `plan.md` (kept alongside this project during development) for the full reasoning behind every design decision.

## Instructions

`register_business` · `update_business_config` · `issue_receipt` · `claim_receipt` · `mint_voucher` · `transfer_voucher` · `present_voucher` · `redeem_voucher` · `cancel_presentation` · `reclaim_expired_receipt`

## Build and test

```bash
anchor build
cargo test
```

## Test coverage

24 tests across 7 files, all currently passing:

| File | Tests | Covers |
|---|---|---|
| `test_initialize.rs` | 2 | PDA creation, seeds constraint rejection |
| `test_register_business.rs` | 3 | Registration, duplicate rejection, multi-tenant isolation |
| `test_update_business_config.rs` | 2 | Owner can update, impostor cannot |
| `test_issue_receipt.rs` | 3 | Issuance, zero-band rejection, unregistered-wallet rejection |
| `test_claim_receipt.rs` | 6 | First/repeat claims, double-claim, wrong secret, expiry, cross-tenant isolation |
| `test_vouchers.rs` | 7 | Full voucher lifecycle failure cases — see file for details |
| `lib.rs` (built-in) | 1 | Program ID sanity check |

None of these test the frontend — they run entirely against the Rust program in a simulated local environment (LiteSVM), with no browser or wallet involved.

## What we'd build next

- A fee-payer relayer, so customers never need to hold SOL themselves (currently they're airdropped a small amount locally)
- Staff delegate keys, so a tablet at the counter can't approve redemptions with the owner's own key
- Ed25519 signature verification for receipts
- Vouchers as real SPL/Token-2022 tokens, so they show up in a normal wallet
- Voucher expiry dates
- Real devnet deployment and testing