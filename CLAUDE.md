# Loyalty Receipt dApp — context for Claude Code

- Anchor version: 1.0.2, pinned. Do not suggest syntax from older Anchor versions (0.29/0.30) — they are incompatible.
- Environment: WSL Ubuntu. Working localhost-first — no devnet or mainnet commands unless explicitly asked.
- Handler function naming convention: every instruction's handler function is named `<instruction_name>_handler`, never the bare word `handler` — this avoids ambiguous glob re-export collisions in instructions.rs.
- See Plan.md in this repo for the full project design and stage-by-stage build order.
cat > CLAUDE.md << 'EOF'
# Loyalty Receipt dApp — context for Claude Code

- Anchor version: 1.0.2, pinned. Do not suggest syntax from older Anchor versions (0.29/0.30) — they are incompatible.
- Environment: WSL Ubuntu. Working localhost-first — no devnet or mainnet commands unless explicitly asked.
- Handler function naming convention: every instruction's handler function is named `<instruction_name>_handler`, never the bare word `handler` — this avoids ambiguous glob re-export collisions in instructions.rs.
- See Plan.md in this repo for the full project design and stage-by-stage build order.
