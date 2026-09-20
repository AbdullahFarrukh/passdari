# Passdari — Solana Receipt dApp (Program)

**The problem.** Paper stamp cards get lost, get forged and can't be verified.
App-based loyalty programs keep points in the business's own database, so
customers don't truly own them and the business can change or erase them.
Blockchain alternatives ask ordinary customers to install a wallet and hold
crypto, which most won't do.

**The solution.** A customer holds a transferable, unforgeable claim on a real reward — redeemable
only with the issuing business's cooperation, without ever installing a wallet
extension or holding any cryptocurrency themselves. Every fee and every account's
rent is covered by a backend relayer, for both merchants and customers.

Underneath that: a digital replacement for the paper stamp card, built as one
dApp serving many businesses. A merchant issues an unclaimed receipt on-chain at
the moment of purchase; the customer scans it to claim a stamp; once enough
stamps are collected, they mint a voucher for the free item — a real Token-2022
NFT, which they can keep, gift to someone else, or redeem themselves. The stamp
card is an NFT too: a soulbound Token-2022 token that can't be sent to another
wallet and is burned when its stamps are spent.

**Why blockchain:** gift-card and loyalty fraud is a real, ongoing industry
problem, and the two-party redemption handshake this app relies on — a merchant
can never unilaterally burn a customer's voucher, a customer can never
unilaterally claim the item without the merchant's cooperation — is genuinely
hard to build trustlessly in a conventional backend (see "Trust model" below for
exactly how it is enforced). On top of that, each business's reward terms are
published on-chain, where they can't be quietly applied inconsistently to
different customers, and the stamp count itself is forge-proof and can't be
silently erased by either side.

This repo holds the on-chain Anchor program. The web frontend lives in a
separate repo: [`passdari-app`](https://github.com/AbdullahFarrukh/passdari-app),
running live at [passdari-app.vercel.app](https://passdari-app.vercel.app).

| At a glance | |
|---|---|
| **Live app** | [passdari-app.vercel.app](https://passdari-app.vercel.app), running on Solana devnet |
| **Repositories** | this repo (the on-chain program) and [`passdari-app`](https://github.com/AbdullahFarrukh/passdari-app) (the web app) |
| **Program on Explorer** | [`HWvv…JCuL`](https://explorer.solana.com/address/HWvvvwSEounpNXcbD4JUNmniB5YxTcFNYoAestzJJCuL?cluster=devnet) |
| **On-chain** | Rust, Anchor 1.0.2, Token-2022 NFTs (non-transferable, permanent delegate, on-chain metadata), tested with LiteSVM |
| **Web app** | Next.js 16, React 19, TypeScript, Tailwind CSS 4, `@solana/web3.js`, Anchor TypeScript client, BIP-39 keys with TweetNaCl, QR scan and generate, Upstash Redis, Google Gemini (merchant AI copilot), Helius RPC, Vercel |

---

## Environment

| Tool | Version |
|---|---|
| Anchor | `=1.0.2` (pinned exactly — do not let this drift) |
| `anchor-spl` | `=1.0.2` (Token-2022 helpers; no Metaplex needed) |
| Rust | 1.89.0 (pinned in `rust-toolchain.toml`) |
| Solana CLI | 3.1.10 (Agave) |
| Node.js | LTS, installed via `nvm` (the frontend needs it; the program itself does not) |
| Local validator | Surfpool (`anchor localnet` uses this by default under Anchor 1.0), or `solana-test-validator` |

**Program ID:** `HWvvvwSEounpNXcbD4JUNmniB5YxTcFNYoAestzJJCuL`

**Network status: live on devnet.** Program ID `HWvvvwSEounpNXcbD4JUNmniB5YxTcFNYoAestzJJCuL`, deployed and upgraded via Helius's devnet RPC (`solana program deploy --use-rpc`) after the free public endpoint repeatedly failed on large deploys. The program on devnet is the same build the tests run against (441,600 bytes; upgrading to a bigger build also needs the program's storage extended, which the CLI does automatically). Every Rust test still runs locally against LiteSVM — see "Build and test," below — but the actual deployed program is real, live, and independently verifiable on Solana Explorer.

---

## Who pays

Every instruction is signed by the real party authorizing it — the merchant or
the customer — but a separate relayer keypair, held only server-side, is named
as fee payer and rent payer on every one that creates an account. Neither side
ever needs to hold SOL. The relayer's own protections live in the web app's
relay endpoint (it only co-signs transactions made of instructions to this
program, and is rate limited) — see the `passdari-app` repo.

A voucher costs the relayer about 0.008 SOL (measured): the voucher account, the
NFT's mint, and the customer's token account. When a voucher is redeemed, the
voucher account's rent goes to the merchant.

A stamp card's NFT costs the relayer about 0.006 SOL (measured) while it exists,
and all of it comes back when the card is cashed in: the NFT's mint and token
account are closed and their rent goes back to the relayer.

## Account model

- **Business**: one per merchant wallet, holds reward terms and running counters
- **LoyaltyCard**: one per (business, customer) pair, holds the stamp count and a locked-in copy of the threshold it was created under
- **Receipt**: one per issued QR code, closed on claim
- **Voucher**: one per minted reward, the on-chain record of a voucher NFT (which business, which id, which mint). Closed on redemption.
- **Voucher NFT**: a Token-2022 mint (one per voucher) with exactly one token, held in the customer's own token account. The token is the source of truth for who owns the voucher.
- **Card NFT**: a Token-2022 mint with one token that can't be moved out of the customer's wallet. A card has one at a time; it is burned when the card's stamps are spent.

There is no `Customer` account, and no `Merchant` account either — both sides
authenticate the same way: a local Solana keypair, generated from a real BIP-39
phrase, password-encrypted in the browser.

Every account is a program-derived address, so anyone can recompute where an
object lives and check it on Solana Explorer:

| Account | Seeds |
|---|---|
| Business | `"business"`, the merchant's wallet |
| LoyaltyCard | `"card"`, the business, the customer's wallet |
| Receipt | `"receipt"`, the business, the hash of the receipt's secret |
| Voucher | `"voucher"`, the business, the voucher id (u64, little-endian) |
| Voucher NFT (mint) | `"voucher_mint"`, the business, the voucher id |
| Card NFT (mint) | `"card_mint"`, the card, the card's `nft_cycle` (u32, little-endian) |

## The voucher NFT

Each voucher is a Token-2022 NFT: 0 decimals, a supply of exactly 1, and its
metadata stored inside the mint itself (metadata pointer plus token metadata),
so it shows up with a name in explorers and wallets. The name is built on-chain
from the business and its reward (for example `Blue Door Cafe - Free coffee`),
the symbol is `PSDR`, and the metadata link is passed in by the client, capped
at 100 characters. The link points at a small page in the web app that returns a
description and a picture (a static PNG, `public/nft/passdari-voucher.png`), so the
picture costs nothing on-chain, and NFTs already minted through the live site pick it up without any change.

The mint is a PDA, and the **voucher account is its freeze authority, its
permanent delegate, and the update authority of its metadata**. The right to
mint more is given up as soon as the single token exists. So only this program
can freeze, thaw or burn a voucher.

| Instruction | What happens to the NFT |
|---|---|
| `mint_voucher(voucher_id, uri)` | Spends the card's stamps, creates the voucher, the mint and the customer's token account, mints 1 token, then locks minting |
| `present_voucher` | The holder freezes it, so it can't be moved while a merchant is looking at it |
| `cancel_presentation` | The holder thaws it |
| `transfer_voucher` | Moves the token to another wallet (the relayer pays for the recipient's token account) and updates the owner hint |
| `redeem_voucher` | The merchant thaws and burns it (through the permanent delegate) and the voucher account is closed |

Two design points worth knowing:

- **The token decides, not the account.** A wallet can move an unfrozen voucher
  without ever calling this program, so `Voucher.owner` can go stale. It is only a
  hint for listing a customer's vouchers; no permission check reads it. Present,
  cancel and redeem all check the token account instead.
- **Redeeming needs no card.** `redeem_voucher` doesn't touch the customer's
  `LoyaltyCard`, so a voucher gifted to someone who never earned a stamp at that
  business still redeems. Redemptions are counted on the business.

A burned voucher leaves its mint on-chain with a supply of 0, as a permanent
record. Measured in the tests: minting costs about 69,000 compute units and
redeeming about 20,000.

## The card NFT

Each stamp card also comes as a Token-2022 NFT: 0 decimals, a supply of exactly 1,
metadata inside the mint (name built on-chain as `<business> stamp card`, symbol
`PSDC`, link passed in by the client and capped at 100 characters; the link's page
returns a picture too, `public/nft/passdari-card.png`). It is
**soulbound**: the mint has the non-transferable extension, so the token program
itself refuses to move it out of the customer's wallet. The **card account is its
mint authority (given up right after minting), its permanent delegate and its close
authority**. There is no freeze authority.

| Instruction | What happens to the card NFT |
|---|---|
| `mint_card_nft(uri)` | Creates the mint and the customer's token account, mints 1 token, then locks minting. The customer signs; the relayer pays |
| `mint_voucher` | Spends the card's stamps, then burns the card NFT through the permanent delegate, closes its accounts (rent back to the relayer) and moves the card on to its next NFT |

How it fits together:

- **One transaction.** The app sends `claim_receipt` and `mint_card_nft` together
  when the card's current NFT doesn't exist yet, so the customer signs once. A card
  gets its NFT with its first stamp.
- **The live stamp count stays in the card account,** not in the token. The token
  proves "this wallet holds a card from this business"; the account says how many
  stamps it has.
- **Each NFT gets a fresh address.** It comes from the card and `nft_cycle`, a
  counter on the card that goes up each time the NFT is burned. (`nft_cycle` reuses
  the old, unused `redemptions` field, so cards made earlier keep working.) Leftover
  stamps stay on the card, and the next stamp brings a new NFT.
- **Cards made before this existed** have no NFT. They can still cash in (the burn is
  skipped when nothing is there) and get one with their next stamp.
- **Cashing in can't get stuck on the NFT.** A holder can burn their own card token
  or close its token account; cashing in still works and cleans up what is left. The
  addresses passed in are checked against the card's real ones, so they can't be
  swapped to dodge the burn.
- **Pre-funding the address doesn't block it.** Anyone can send lamports to the mint's
  address in advance, since it is easy to work out. The mint is created in steps
  (transfer, allocate, assign) so that can't stop a card from getting its NFT.

Measured in the tests: `claim_receipt` plus `mint_card_nft` together use about
88,000 compute units, and `mint_voucher` with a card NFT to burn about 93,000.

## Trust model

- **The merchant can't take a voucher.** `redeem_voucher` only works on a voucher
  its holder has presented (frozen), issued by that same business, and signed by
  that business's own authority. The holder can cancel any time before
  redemption. A frozen token can't be moved by anyone, including from a wallet —
  the token program itself refuses.
- **The permanent delegates can't be abused.** For a voucher it is this program's
  own voucher account, which only signs inside `mint_voucher`, `present_voucher`,
  `cancel_presentation` and `redeem_voucher`. For a card NFT it is the card account,
  which only signs inside `mint_card_nft` and `mint_voucher`. No instruction lets
  anyone else use either. Wallets and explorers may show a warning that the token has a permanent
  delegate; that is expected.
- **The customer can't claim without the merchant.** Unchanged: a receipt is
  created by the merchant and can be claimed once.
- **The program is upgradeable.** Its rules are only as fixed as its upgrade
  authority, which on devnet is the deployer wallet.

## Instructions

`register_business` · `update_business_config` · `issue_receipt` · `claim_receipt` · `mint_card_nft` · `mint_voucher` · `transfer_voucher` · `present_voucher` · `redeem_voucher` · `cancel_presentation` · `reclaim_expired_receipt`

There is also `initialize`, which creates a counter. It is left over from the
project template and the app doesn't use it.

## Build and test

```bash
anchor build
cargo test
```

Run `anchor build` first: the tests load the compiled program from
`target/deploy/loyalty.so`.

## Test coverage

46 tests across 8 files, all currently passing:

| File | Tests | Covers |
|---|---|---|
| `test_initialize.rs` | 2 | PDA creation, seeds constraint rejection |
| `test_register_business.rs` | 3 | Registration, duplicate rejection, multi-tenant isolation |
| `test_update_business_config.rs` | 2 | Owner can update, impostor cannot |
| `test_issue_receipt.rs` | 3 | Issuance, zero-band rejection, unregistered-wallet rejection |
| `test_claim_receipt.rs` | 6 | First/repeat claims, double-claim, wrong secret, expiry, cross-tenant isolation |
| `test_card_nft.rs` | 12 | The card NFT (supply, authorities, soulbound, name and symbol), a wallet can't move it, cashing in burns it and keeps leftover stamps, the next stamp brings a fresh one, cards without an NFT still cash in, a holder who burned their own card can still cash in, nobody can dodge the burn or mint for someone else's card, a pre-funded address doesn't block it, and the relayer gets all the rent back |
| `test_vouchers.rs` | 17 | The NFT itself (supply, authorities, metadata), presenting freezes and cancelling thaws, gifting moves the real token, a wallet can't move a presented voucher, redeeming burns it, a gifted voucher redeems without a card, and the failure cases (below the stamp threshold, unpresented, wrong business, redeemed twice). Also: raising the reward threshold never voids a card's already-earned reward — see file for details |
| `lib.rs` (built-in) | 1 | Program ID sanity check |

Every failure test checks for the specific error it should fail with (a shared
helper in `tests/common/mod.rs` reads it from the program logs), so a test can't
pass for the wrong reason. Repeated transactions get a fresh blockhash, because
LiteSVM would otherwise refuse the repeat as "AlreadyProcessed" before the
program ran. None of these test the frontend — they run entirely against the
Rust program in a simulated local environment (LiteSVM), with no browser
involved. LiteSVM bundles a slightly older Token-2022 than devnet runs; both
support everything used here.

## What we'd build next

- Staff delegate keys, so a tablet at the counter can't approve redemptions with the owner's own key
- Ed25519 signature verification for receipts
- Voucher expiry dates
- Host the NFT pictures somewhere permanent (today the web app serves them, one picture for all vouchers and one for all cards, so they depend on it staying up; ownership itself stays on-chain)
- Closing a burned voucher's mint to recover its rent (today it is left on-chain as a record)
- A way for a merchant to deregister a business (currently leaves an orphaned account)

**Operational note:** the program's upgrade keypair (`target/deploy/loyalty-keypair.json`) is backed up outside the build directory — losing it would mean any redeploy generates a new program ID, silently invalidating every reference to the old one.
