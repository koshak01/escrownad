# EscrowNad — PROJECT

## One line
Escrow that settles on **external proof** (oracle), not on counterparty trust. Demo chain: **Monad**. First oracle module: **RIPE** (PA + PI transfer JSON).

## Product
- Parties: preferably Cleanverse-verified identity (sandbox on hackathon).
- Buyer deposits funds into on-chain **lock** (escrow contract on Monad).
- Release only when observer confirms external condition matched.
- Self-check **before** open deal (seller still holds resource; type PA/PI matches).
- Dispute: only if no registry fact / grey zone; if RIPE line matches → auto release (no dispute against fact).
- Timeout (~up to 30d product; hackathon shorter): refund.
- Site = thin shell over contract + observer (Rust + template, wallet login).
- Not marketing site; not multi-chain deploy on day 1 (architecture multi-chain later).

## Why not generic escrow
Monad already has tutorial-style escrow demos (e.g. BluOwn monadescrow: buyer/seller/arbiter). We are **proof-escrow / oracle-escrow** — first real module IP transfers via RIPE public tables.

## RIPE fact (sold)
Sources:
- PI: `https://www-static.ripe.net/dynamic/table-of-transfers/ipv4/transfers-assignments.json`
- PA: `https://www-static.ripe.net/dynamic/table-of-transfers/ipv4/transfers-allocations.json`

Match deal fields: date/block/from/to/type. Existing poller reference: `ip_sale_check` (work/rust/ip_sale_check).

Example line shape:
`21/05/2024  176.120.84.0-176.120.95.255 → 176.120.88.0/21`
`Tochka Opory LLC → IT PARK JSC [POLICY]`

Whois/BGP = optional strengthen; RIPE table = 99% for auto-release.

## Observer
v1: **single** Rust service (one key). Later: multi-sig / multi-poller. Signs/calls release on Monad when match.

## Stack
| Layer | Tech |
|---|---|
| Chain | **Monad** (EVM contracts, usually Solidity) |
| Observer / API / self-check | **Rust** |
| UI shell | Rust + templates, one domain |
| Identity | Cleanverse sandbox when keys |

## Hackathon (Cleanverse Build Trusted Assets)
- Reg until Aug 7 23:59 UTC; build Aug 8–9; results Aug 14
- Tracks: RWA / Compliant DeFi — product fits RWA-ish + verified parties
- Deliverable: public repo + demo video + working deploy (contract + UI)
- Depth of integration weighted high — wire Cleanverse CVI when available

## 48h scope
IN: contract lock on Monad testnet; observer PA+PI (or one live + other stub); self-check minimal; thin UI; README; oracle explainer page; video path
OUT: all 7 chains, insurance 100k, domains module, complex arbiter UI, pretty brand

## Name
**EscrowNad** / `escrownad` — free (unlike safetron, safescrow, helix8). Domains: consider escrownad.com / .xyz.

## Peer
Separate collective peer `escrownad` (not Nike). Nike = hunt/coord only.

## Operator
Petr. Builder peer after Hyperion seeds git.
