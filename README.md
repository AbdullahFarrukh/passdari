# Passdari — Solana Receipt dApp (Program)

A customer holds a transferable, unforgeable claim on a real reward — redeemable
only with the issuing business's cooperation, without ever installing a wallet
extension or holding any cryptocurrency themselves. Every fee and every account's
rent is covered by a backend relayer, for both merchants and customers.

Underneath that: a digital replacement for the paper stamp card, built as one
dApp serving many businesses. A merchant issues an unclaimed receipt on-chain at
the moment of purchase; the customer scans it to claim a stamp; once enough
stamps are collected, they mint a transferable voucher for the free item — which
they can keep, gift to someone else, or redeem themselves.

**Why blockchain:** gift-card and loyalty fraud is a real, ongoing industry
problem, and the two-party redemption handshake this app relies on — a merchant
can never unilaterally burn a customer's voucher, a customer can never
unilaterally claim the item without the merchant's cooperation — is genuinely
hard to build trustlessly in a conventional backend. On top of that, each
business's reward terms are published on-chain, where they can't be quietly
applied inconsistently to different customers, and the stamp count itself is
forge-proof and can't be silently erased by either side.

This repo holds the on-chain Anchor program. The web frontend lives in a
separate repo: [`stampcoin-app`](https://github.com/AbdullahFarrukh/stampcoin-app).

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

**Network status: live on devnet.** Program ID `HWvvvwSEounpNXcbD4JUNmniB5YxTcFNYoAestzJJCuL`, deployed via Helius's devnet RPC after the free public endpoint repeatedly failed on large deploys. Every Rust test still runs locally against LiteSVM — see "Build and test," above — but the actual deployed program is real, live, and independently verifiable on Solana Explorer.

---

## Who pays

Every instruction is signed by the real party authorizing it — the merchant or
the customer — but a separate relayer keypair, held only server-side, is named
as fee payer and rent payer on every one. Neither side ever needs to hold SOL.
See `plan.md`'s "Who pays" section for the full reasoning, including the one
known gap: the relay endpoint currently trusts anything it's asked to sign, with
no rate-limiting or instruction validation yet.

## Account model

Business — one per merchant wallet, holds reward terms and running counters
LoyaltyCard — one per (business, customer) pair, holds stamp count and a locked-in copy of the threshold it was created under
Receipt — one per issued QR code, closed on claim
Voucher — one per minted reward, transferable, closed on redemption

There is no `Customer` account, and no `Merchant` account either — both sides
authenticate the same way: a local Solana keypair, generated from a real BIP-39
phrase, password-encrypted in the browser. See `plan.md` for the full reasoning
behind every design decision.

## Instructions

`register_business` · `update_business_config` · `issue_receipt` · `claim_receipt` · `mint_voucher` · `transfer_voucher` · `present_voucher` · `redeem_voucher` · `cancel_presentation` · `reclaim_expired_receipt`

## Build and test

```bash
anchor build
cargo test
```

## Test coverage

25 tests across 7 files, all currently passing:

| File | Tests | Covers |
|---|---|---|
| `test_initialize.rs` | 2 | PDA creation, seeds constraint rejection |
| `test_register_business.rs` | 3 | Registration, duplicate rejection, multi-tenant isolation |
| `test_update_business_config.rs` | 2 | Owner can update, impostor cannot |
| `test_issue_receipt.rs` | 3 | Issuance, zero-band rejection, unregistered-wallet rejection |
| `test_claim_receipt.rs` | 6 | First/repeat claims, double-claim, wrong secret, expiry, cross-tenant isolation |
| `test_vouchers.rs` | 8 | Full voucher lifecycle failure cases, plus raising the reward threshold never voiding a card's already-earned reward — see file for details |
| `lib.rs` (built-in) | 1 | Program ID sanity check |

None of these test the frontend — they run entirely against the Rust program in a simulated local environment (LiteSVM), with no browser involved.

## What we'd build next

- Rate-limiting and instruction validation for the fee-payer relayer (currently trusts anything handed to it)
- Staff delegate keys, so a tablet at the counter can't approve redemptions with the owner's own key
- Ed25519 signature verification for receipts
- Vouchers as real SPL/Token-2022 tokens, so they show up in a normal wallet
- Voucher expiry dates
- A way for a merchant to deregister a business (currently leaves an orphaned account)
- Real devnet deployment and testing

**Operational note:** the program's upgrade keypair (`target/deploy/loyalty-keypair.json`) is backed up outside the build directory — losing it would mean any redeploy generates a new program ID, silently invalidating every reference to the old one.