# Loyalty Receipt dApp — context for Claude Code

- Anchor version: 1.0.2, pinned. Do not suggest syntax from older Anchor versions (0.29/0.30) — they are incompatible.
- Environment: WSL Ubuntu. Working localhost-first — no devnet or mainnet commands unless explicitly asked.
- Handler function naming convention: every instruction's handler function is named `<instruction_name>_handler`, never the bare word `handler` — this avoids ambiguous glob re-export collisions in instructions.rs.
- Run `anchor build` after every program change, before `cargo test`. The tests load the already-compiled `target/deploy/loyalty.so` with `include_bytes!`, so testing without rebuilding silently runs the old binary.
- Adding a field to an account is a breaking change. Put it at the **end** of the struct — anywhere else shifts every field after it and makes accounts already on chain unreadable — and add a migration, because the program cannot deserialize a shorter account at all. `migrate_card` and `migrate_business` are the worked examples, and they must run immediately after the upgrade, not at leisure. Note that a `String` field is Borsh length-prefixed and stores only the characters used, so accounts containing one cannot be read at fixed byte offsets; declare the old struct and deserialize it instead.
- See README.md in this repo for the project design: account model and PDA seeds, the voucher and card NFTs, what a merchant may change, closing accounts, fee and rent parameters, trust model, instructions, and how to build and test.
