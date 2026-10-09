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

**Network status: live on devnet.** Program ID `HWvvvwSEounpNXcbD4JUNmniB5YxTcFNYoAestzJJCuL`, deployed and upgraded via Helius's devnet RPC (`solana program deploy --use-rpc`) after the free public endpoint repeatedly failed on large deploys. The program on devnet is the same build the tests run against (624,520 bytes, verified byte-identical against the local build after every upgrade). Growing past the space already allocated needs `solana program extend <program-id> <bytes>` first, or the deploy fails. Every Rust test still runs locally against LiteSVM — see "Build and test," below — but the deployed program is real, live, and independently verifiable on Solana Explorer.

---

## Who pays

Every instruction is signed by the real party authorizing it — the merchant or
the customer — but a separate relayer keypair, held only server-side, is named
as fee payer and rent payer on every one that creates an account. Neither side
ever needs to hold SOL. The relayer's own protections live in the web app's
relay endpoint (it only co-signs transactions made of instructions to this
program, and is rate limited) — see the `passdari-app` repo.

**Rent always goes back to whoever paid it.** Every account records the wallet that
paid its rent (`rent_payer`), and every instruction that closes one sends the rent
back to exactly that wallet — never to the merchant or customer, who paid nothing,
and never to whoever happens to sign as the relayer.

**As of the October 2026 upgrade, nothing is permanently stranded.** Stamp cards
and business accounts used to be the two things whose rent could never come back;
`close_dead_card` and `close_business` now return both. What the relayer truly
spends is transaction fees.

### Fee parameters

A Solana transaction costs **5,000 lamports per signature**. The relayer is the fee
payer on every transaction this app builds, and a fee payer always signs — so an
instruction that doesn't name the relayer as an account still costs one extra
signature. Every action below carries two.

| Action | Transactions | Fee (lamports) | Fee (SOL) |
|---|---|---|---|
| Register a business | 1 | 10,000 | 0.00001 |
| One stamp (`issue_receipt` + `claim_receipt`) | 2 | 20,000 | 0.00002 |
| One reward (`mint_voucher` + `present_voucher` + `redeem_voucher`) | 3 | 30,000 | 0.00003 |
| Pass stamps to a friend (`transfer_stamps`) | 1 | 10,000 | 0.00001 |
| Any clean-up (close an expired receipt, voucher, idle NFT, dead card or shop) | 1 | 5,000 | 0.000005 |

### Rent parameters

Rent on devnet is `(128 + bytes) × 5,080` lamports, where the 128 is a fixed
per-account overhead every account pays before a byte of its own. Measured on
devnet, from live accounts:

| Account | Bytes | Lamports | SOL | Comes back |
|---|---|---|---|---|
| Business | 241 | 1,874,520 | 0.001875 | on `close_business` |
| LoyaltyCard | 139 | 1,356,360 | 0.001356 | on `close_dead_card` |
| Receipt | 90 | 1,107,440 | 0.001107 | on claim, or reclaim after expiry |
| Voucher | 161 | 1,468,120 | 0.001468 | on redeem, or close after 90 days |
| Voucher NFT mint | 483 | 3,144,520 | 0.003145 | with the voucher |
| Voucher token account | 170 | 1,513,840 | 0.001514 | with the voucher |
| Card NFT mint | 436 | 2,905,760 | 0.002906 | at cash-in, or after 90 idle days |
| Card NFT token account | 174 | 1,534,160 | 0.001534 | with the card NFT |

So an active stamp card with its NFT holds about **0.00580 SOL** of the relayer's
money, and a live voucher about **0.00613 SOL** more until it is redeemed.

**Mainnet rent is higher.** Devnet's rent sysvar reads `lamports_per_byte_year =
5080` with an exemption threshold of 1. Solana's standard defaults are 3,480 and
2.0, which works out to `(128 + bytes) × 6,960` — about **1.37x** these figures.
Re-measure before relying on any of this for a mainnet launch; fees are the same
everywhere.

**Nobody has to remember to redeem or use anything.** A voucher is valid for 90
days after it is minted; a card's NFT is recycled if 90 days pass with no new
stamp (the stamps stay on the card — only the NFT is replaced). After that,
`close_expired_voucher` or `retire_idle_card_nft` sends the rent back the same way
redeeming or cashing in would. Anyone can call either — the rent can only ever go
to the wallet the account already names, so there's nothing to gain by calling it
early or for someone else's account. The web app runs both as a background job and
offers a "Clean up" button; see its README for how.

## What a merchant can change, and what they can't

A shop's **name and category are fixed at registration**. The name is written into
every card and voucher NFT at the moment it is minted and is never rewritten, so
letting it change would leave tokens in customers' wallets naming a shop that no
longer calls itself that — and would let one shop rename itself as another.

Everything about the **reward** can change: its name, how many stamps it takes, the
minimum purchase that earns one, and how long a code stays valid
(`update_business_config`). Three rules keep that honest:

- **A card in progress keeps its own terms.** Each card stores the
  `stamps_required_snapshot` it was opened under, so a shop raising its price from 8
  stamps to 10 cannot move the goalposts on someone already seven stamps in. The new
  number applies to cards opened from then on — and to that same card's *next*
  round, since the snapshot is refreshed when a card is cashed in. Finish the card in
  your pocket under its old terms; the next one has the current rules on it.
- **A shop can commit to an offer.** `terms_locked_until` is a date the shop promises
  to stand behind its reward until, and while it stands the reward cannot be changed
  at all. Committing is optional (zero means no promise), at most a year, and a
  promise already made can only be brought forward with at least two weeks' notice —
  so an offer can be wound down without being yanked away from people collecting.
- **Settings are checked.** A card must need between 1 and 100 stamps, a name and a
  reward label are required, strings must fit the space reserved for them, and a
  receipt must last between a minute and a day. Before this, a shop could set "stamps
  needed" to zero, which satisfied `mint_voucher`'s threshold check with an empty card
  and took nothing away — one stamp bought reward after reward, each a real NFT the
  relayer paid rent for.

## Closing accounts

Every account this program creates can now be closed and its rent returned.

| Instruction | Closes | When |
|---|---|---|
| `claim_receipt` / `reclaim_expired_receipt` | a receipt | on claim, or once it has expired |
| `redeem_voucher` / `close_expired_voucher` | a voucher, its mint and token account | on redemption, or 90 days after minting |
| `mint_voucher` / `retire_idle_card_nft` | a card NFT, its mint and token account | at cash-in, or 90 days with no stamp |
| `close_dead_card` | a stamp card | no stamps left, no NFT, and a year untouched |
| `close_business` | a shop | its offer has ended, no cards are open and no rewards are owed |

`close_dead_card` requires the card's NFT to be gone first, and that is not
tidiness: closing resets the card's `nft_cycle` to zero, so the next NFT it ever
mints lands on the address the very first one used. Were that mint still alive the
card would be stuck unable to make one.

`close_business` leans on two counters that `total_cards` and `total_redemptions`
could never answer — `open_cards` and `open_vouchers`, which come *down* as things
are closed. A voucher that expires unclaimed is cleaned up without ever counting as
a redemption, so comparing issued against redeemed would never come true again once
a single reward went unclaimed.

## Passing stamps to a friend

`transfer_stamps` moves stamps from one card to another **at the same shop**. A
stamp is a debt the shop that issued it owes, so carrying stamps to a second shop
would make that shop owe a reward for a sale it never made; `has_one = business` on
both cards refuses it in the program, not just in the app.

Nothing is created: the sender's card falls by exactly what the receiver's rises by,
and the shop's `total_stamps_issued` is untouched. The receiving card must already
exist — there is deliberately no `init_if_needed`, or one person could send a single
stamp to a thousand fresh addresses and leave the relayer paying rent on a thousand
accounts for good.

`lifetime_stamps` moves with the stamps so a transfer cannot be used to climb the
merchant's leaderboard. How often one wallet may do this is decided by the relayer
before it co-signs, which is the right place for it: the limit protects the
relayer's own balance, and the relayer can simply decline.

## Account model

- **Business**: one per merchant wallet, holds reward terms and running counters
- **LoyaltyCard**: one per (business, customer) pair, holds the stamp count and a locked-in copy of the threshold it was created under
- **Receipt**: one per issued QR code, closed on claim; records who paid its rent, and the rent goes back there
- **Voucher**: one per minted reward, the on-chain record of a voucher NFT (which business, which id, which mint), and who paid for it. Closed on redemption, with all the rent going back to that payer.
- **Voucher NFT**: a Token-2022 mint (one per voucher) with exactly one token, held in the customer's own token account. The token is the source of truth for who owns the voucher.
- **Card NFT**: a Token-2022 mint with one token that can't be moved out of the customer's wallet. A card has one at a time; it is burned when the card's stamps are spent.

The `CardNft` record that used to hold each NFT's rent payer is gone. It was one
account per NFT holding 32 useful bytes for 858,520 lamports, three quarters of
which was the fixed price of being an account at all; the same field now sits on
the card, which already exists, for 162,560.

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

A voucher is only usable for **90 days** after it is minted (`Voucher.expires_at`,
fixed at minting, never shortened). `present_voucher`, `transfer_voucher` and
`redeem_voucher` all check this. Past it, `close_expired_voucher` burns whatever
is left (thawing first if it was still presented), closes the mint, the holder's
token account and the voucher record, and sends their rent back to whoever paid for
them — the same as a redeem, just triggered by time instead of a merchant. A gifted
voucher nobody ever presented leaves the recipient's own (now closed-to-nothing)
token account behind: the recipient never signed anything granting the right to
close it, so the program can't.

The mint is a PDA, and the **voucher account is its freeze authority, its
permanent delegate, its close authority and the update authority of its
metadata**. The right to mint more is given up as soon as the single token
exists. So only this program can freeze, thaw, burn or close a voucher.

| Instruction | What happens to the NFT |
|---|---|
| `mint_voucher(voucher_id, uri)` | Spends the card's stamps, creates the voucher, the mint and the customer's token account, mints 1 token, then locks minting |
| `present_voucher` | The holder freezes it, so it can't be moved while a merchant is looking at it, and hands the voucher the right to close their (soon empty) token account |
| `cancel_presentation` | The holder thaws it |
| `transfer_voucher` | Moves the token to another wallet (the relayer pays for the recipient's token account), closes the sender's emptied account (its rent goes back to the payer) and updates the owner hint |
| `redeem_voucher` | The merchant thaws and burns it (through the permanent delegate), then the mint, the holder's token account and the voucher account are closed and their rent goes back to the payer |
| `close_expired_voucher` | Past its 90 days: burns whatever's left (thawing first if needed), closes the mint, the holder's token account and the voucher record, rent back to the payer. Anyone can call it |

Two design points worth knowing:

- **The token decides, not the account.** A wallet can move an unfrozen voucher
  without ever calling this program, so `Voucher.owner` can go stale. It is only a
  hint for listing a customer's vouchers; no permission check reads it. Present,
  cancel and redeem all check the token account instead.
- **Redeeming needs no card.** `redeem_voucher` doesn't touch the customer's
  `LoyaltyCard`, so a voucher gifted to someone who never earned a stamp at that
  business still redeems. Redemptions are counted on the business.

The burn is the permanent proof a voucher was used: it stays in the chain's
transaction history for good, while the emptied accounts are closed (the card NFT
works the same way). A closed voucher can never come back, because only this
program can create that address and voucher ids only go up. Measured in the tests:
minting costs about 75,000 compute units and redeeming about 27,000.

## The card NFT

Each stamp card also comes as a Token-2022 NFT: 0 decimals, a supply of exactly 1,
metadata inside the mint (name built on-chain as the shop's own name, symbol
`PSDC`, link passed in by the client and capped at 100 characters; the link's page
returns a picture too, `public/nft/passdari-card.png`). Both the name and the link
are deliberately short: that text lives inside the mint account where every byte is
rent, so " stamp card" in the name cost 55,880 lamports a card to repeat what the
symbol already says, and the link is one shared `/c` page rather than `/c/<mint>`,
which only ever added back the mint address a wallet reading the metadata already
has. It is
**soulbound**: the mint has the non-transferable extension, so the token program
itself refuses to move it out of the customer's wallet. The **card account is its
mint authority (given up right after minting), its permanent delegate and its close
authority**. There is no freeze authority.

| Instruction | What happens to the card NFT |
|---|---|
| `mint_card_nft(uri)` | Creates the mint and the customer's token account, mints 1 token, then locks minting. The customer signs; the relayer pays |
| `mint_voucher` | Spends the card's stamps, then burns the card NFT through the permanent delegate, closes its accounts (rent back to whoever paid for it) and moves the card on to its next NFT |
| `retire_idle_card_nft` | 90 days with no stamp: burns the NFT, closes its accounts (rent back to the payer) and moves the card on to its next NFT, the same as cashing in. Anyone can call it |

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
- **Who paid for it is recorded on the card.** Both `mint_voucher` and
  `retire_idle_card_nft` read the card's own `rent_payer` before sending rent
  anywhere. A card made before that field existed has it blank, and its NFT's rent
  goes to the relayer signing the cash-in.
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
  `cancel_presentation`, `redeem_voucher` and `close_expired_voucher`. For a card NFT
  it is the card account, which only signs inside `mint_card_nft`, `mint_voucher` and
  `retire_idle_card_nft`. No instruction lets anyone else use either. Wallets and
  explorers may show a warning that the token has a permanent delegate; that is
  expected.
- **Anyone can trigger a clean-up, and that's safe by design.** `close_expired_voucher`,
  `retire_idle_card_nft` and `reclaim_expired_receipt` all take no signer but the fee
  payer. Every one of them sends rent only to the address the account already
  names, and only past its own deadline, so calling one early, or for someone
  else's account, or a thousand times a second, can never move a single lamport
  anywhere it wasn't already going.
- **Nobody can collect rent they didn't pay.** Closing an account checks the
  recorded `rent_payer`, so a merchant can't name themselves the "relayer" to keep a
  receipt's or voucher's rent (the old code allowed that when reclaiming expired
  receipts).
- **Expiry can only ever cost the holder the thing itself.** A voucher expiring loses
  the holder the reward; a card's NFT being recycled loses only the badge — the
  stamps themselves are never touched, on the card or in the count of stamps still
  needed. Neither date can be shortened after the fact: they're fixed once, at
  minting.
- **The customer can't claim without the merchant.** Unchanged: a receipt is
  created by the merchant and can be claimed once.
- **The program is upgradeable.** Its rules are only as fixed as its upgrade
  authority, which on devnet is the deployer wallet.

## Instructions

**Trading:** `register_business` · `update_business_config` · `issue_receipt` ·
`claim_receipt` · `mint_card_nft` · `mint_voucher` · `transfer_stamps` ·
`transfer_voucher` · `present_voucher` · `cancel_presentation` · `redeem_voucher`

**Returning rent:** `reclaim_expired_receipt` · `close_expired_voucher` ·
`retire_idle_card_nft` · `close_dead_card` · `close_business`

**One-off migrations:** `migrate_card` · `migrate_business`, which bring accounts
created by an earlier version up to the current shape. A fresh deployment never
needs either. They exist because an account that has grown is shorter than the
struct the program now expects, so the program cannot read it at all until it has
been through one — which is why they must run immediately after an upgrade, not at
leisure. There is a test proving an un-migrated card fails with
`AccountDidNotDeserialize`.

There is also `initialize`, which creates a counter. It is left over from the
project template and the app doesn't use it.

## Build and test

```bash
anchor build
cargo test
```

Run `anchor build` first, and **again after every source change**: the tests load
the already-compiled program from `target/deploy/loyalty.so` via `include_bytes!`,
so editing the program and running `cargo test` without rebuilding tests the old
binary and gives confusing results.

## Test coverage

103 tests across 10 files, all currently passing:

| File | Tests | Covers |
|---|---|---|
| `test_initialize.rs` | 2 | PDA creation, seeds constraint rejection |
| `test_register_business.rs` | 3 | Registration, duplicate rejection, multi-tenant isolation |
| `test_update_business_config.rs` | 2 | Owner can update, impostor cannot |
| `test_issue_receipt.rs` | 3 | Issuance, zero-band rejection, unregistered-wallet rejection |
| `test_claim_receipt.rs` | 11 | First/repeat claims, double-claim, wrong secret, expiry, cross-tenant isolation; a claimed or expired receipt's rent goes back to the relayer (only fees are spent), and neither the customer nor the merchant can redirect it |
| `test_card_nft.rs` | 17 | The card NFT (supply, authorities, soulbound, name and symbol), a wallet can't move it, cashing in burns it and keeps leftover stamps, the next stamp brings a fresh one, cards without an NFT still cash in, a holder who burned their own card can still cash in, nobody can dodge the burn or mint for someone else's card, a pre-funded address doesn't block it, the relayer gets all the rent back, and an NFT idle for 90 days is recycled (anyone can trigger it, rent goes back to the recorded payer, the stamps stay on the card) |
| `test_transfer_stamps.rs` | 10 | Moving stamps between cards at one shop: the move itself, that no stamps are created, that a transfer can't fake a finished reward, and the refusals — more than the card holds, zero, a card at another shop, someone else's card, a card that doesn't exist yet, and a card sending to itself (Anchor's own duplicate-mutable-account constraint catches that one before the handler runs). Plus: pooled stamps really do pass the check `mint_voucher` makes |
| `test_close_dead_card.rs` | 15 | Closing a dead card and getting all its rent back; refusing one with stamps on it, one used within the year, and one whose NFT is still alive (which would strand the cycle-0 mint address); rent can't be redirected; a returning customer starts over cleanly. Also the migrations: an old card is grown and given its payer, only migrated once, refuses a non-card, still works with the idle-NFT sweep afterwards, and — the point of the whole thing — an un-migrated card cannot be used at all. Plus: a shop cannot ask for zero stamps, on the way in or through an edit |
| `test_fair_terms.rs` | 11 | A finished card taking on the shop's current terms for its next round; `rewards_earned` counting up and surviving a change of terms (with the old derivation shown giving the wrong answer); a shop held to its committed offer; the year cap, the past-date refusal and the two-weeks-notice rule; closing a shop that has finished trading and getting its rent back; refusing to close mid-offer, with cards open, or with rewards owed; rent can't be redirected; a new card counting against the shop; and a guard that the new fields fit the space reserved for them |
| `test_vouchers.rs` | 28 | The NFT itself (supply, authorities, metadata), presenting freezes and cancelling thaws, gifting moves the real token, a wallet can't move a presented voucher, redeeming burns it and closes every account (the whole voucher costs the relayer only fees, and the merchant can't redirect the rent), gifting closes the sender's empty account, a gifted voucher redeems without a card, and the failure cases (below the stamp threshold, unpresented, wrong business, redeemed twice, expired). A voucher past its 90 days can no longer be presented, gifted or redeemed, and closing one (presented, gifted-but-never-presented, or plain) sends its rent to whoever paid for it, never to whoever happens to call it. Also: raising the reward threshold never voids a card's already-earned reward — see file for details |
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

**Before real merchants, in this order:**

- **Get this audited, and move the upgrade authority off a single key.** The program
  has never had an outside review, and today one wallet can rewrite its rules — which
  undercuts the claim that a shop's terms are on-chain and cannot be quietly changed.
  A multisig is the normal answer.
- **Fix the web app's search surface.** The public directory has no page per shop
  with its own URL and `LocalBusiness` schema, and its live sitemap is submitting
  test-data categories to Google, which lowers the quality signal rather than raising
  it. Both are small, and both want doing before a real shop is listed rather than
  after. See the passdari-app README, "Before mainnet".
- **Re-measure every rent figure above on mainnet.** They were measured on devnet,
  whose rent rate is about 1.37x lower.
- **Approved merchants only.** The relayer should pay a business's rent only for
  wallets on an approved list, with a daily SOL budget cap on relayer spending.
  Today anyone can register businesses at the relayer's expense (see the
  passdari-app README, "Known limitations"). This is the largest remaining hole.
- **An audited relayer, with the key off the server.** The Solana Foundation's
  [Kora](https://github.com/solana-foundation/kora) does what this project's own
  relayer does, has been audited, and supports remote signers (Turnkey, Privy,
  Openfort) so the fee payer's key need not sit in an environment variable.
- **Staff delegate keys,** so a tablet at the counter can't approve redemptions
  with the owner's own key.

**Worth doing, not urgent:**

- Ed25519 signature verification for receipts.
- Host the NFT pictures somewhere permanent. Today the web app serves them — one
  picture for all vouchers and one for all cards — so they depend on it staying up.
  Ownership itself stays on-chain.
- Let a customer carry a card's progress between shops in a group, or a shop run
  more than one reward at once. Both are real asks that the current one-reward,
  one-shop model does not cover.

**Deliberately not done, and why:**

- **Freezing a reward's *name* per card.** Snapshotting the reward label onto every
  card would cost about 183,000 lamports each, and would force a shop that stops
  selling croissants to owe five hundred cards a croissant. The committed-offer date
  gives the same protection for a fraction of the cost, because it is one fact per
  shop rather than one per card.
- **Compressed NFTs.** They would collapse the card NFT's cost almost entirely, and
  Bubblegum V2 can keep them soulbound. But every burn needs a Merkle proof passed
  in the transaction, and `mint_voucher` is already close to the size limit; the tree
  has a fixed lifetime capacity that burning does not give back; and reads would need
  a DAS-capable RPC. Worth revisiting only if the float genuinely hurts.


**Operational note:** the program's upgrade keypair (`target/deploy/loyalty-keypair.json`) is backed up outside the build directory — losing it would mean any redeploy generates a new program ID, silently invalidating every reference to the old one.
