# EscrowNad

Proof-escrow on **Monad**: **USDC** unlocks when an external oracle confirms the
condition. First asset: **IPv4 (RIPE PA + PI)**.

Forge app (skeleton pattern). Peer: **Янус** (`escrownad`). Domain/DB:
`escrownad.com`.

> Settlement asset: **USDC** (ERC-20). Native MON = gas only.  
> RIPE proof unlocks USDC to the seller. (Lesson: not native-only — tidex6.)

## Layout (forge)

```
escrownad/
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── models/          # Demo (эталон) + Deal
│   ├── pages/           # index, deals, oracle + admin/demos
│   ├── observer/        # RIPE PA+PI match
│   ├── chain/           # mock USDC lock txs
│   └── bin/             # database, notifier, ws, web, observer
├── etc/
├── templates/
├── static/
├── seeds/
├── contracts/           # EscrowLock (USDC) + MockUSDC + DEPLOY.md
└── deploy/
```

```
~/work/rust/
├── forge/
└── escrownad/
```

## Dev (ports 4100 / 4101)

```bash
psql -h 127.0.0.1 -U html -d 'escrownad.com' -f ../forge/docs/db_schema.sql
psql -h 127.0.0.1 -U html -d 'escrownad.com' \
  -v admin_email="adm@escrownad.com" -v admin_pass="$(tr -d '\n' < pass.txt)" \
  -f seeds/forge.sql
psql -h 127.0.0.1 -U html -d 'escrownad.com' -f seeds/demos.sql -f seeds/deals.sql

cargo build --bins
./target/debug/escrownad-database &
./target/debug/escrownad-notifier &
./target/debug/escrownad-ws &
./target/debug/escrownad-web &
```

| | |
|---|---|
| Site | http://localhost:4100/ |
| Deals | http://localhost:4100/deals/ |
| Oracle | http://localhost:4100/oracle/ |
| Admin | http://localhost:4100/admin/ |
| Login | `adm@escrownad.com` — `pass.txt` (not in git) |

## Demo path (product skeleton, mock USDC)

1. http://localhost:4100/cabinet/ — one cabinet  
2. http://localhost:4100/deals/new/ — create draft lot  
3. Deal card actions (buttons):  
   `soft_verify` → `list` → `request` → `accept` → `start_prepare` →  
   `mark_prepared` → `fund` (mock USDC lock) → status `awaiting_proof`  
4. Observer:

```bash
OBSERVER_ONCE=1 ./target/debug/escrownad-observer
# → released, release_tx mock:usdc:ripe:…
```

5. Search: http://localhost:4100/deals/  
6. Oracle: http://localhost:4100/oracle/  

Fixtures: two `listed` IP lots after `seeds/deals_flow.sql`.

## Product flow

1. Seller lists IP (PA|PI) + from/to  
2. Buyer `approve` USDC → `EscrowLock.fund` (amount in USDC base units, 6 dec)  
3. Observer polls RIPE PA + PI  
4. Match → `release` USDC to seller; timeout → `refund` USDC  

### Contracts

| | |
|---|---|
| Lock | `contracts/EscrowLock.sol` — USDC fund/release/refund |
| Dev token | `contracts/MockUSDC.sol` — if no official Monad testnet USDC |
| Notes | `contracts/DEPLOY.md` |

Live deploy needs Foundry + Monad RPC; mock path is honest for video until then.

See `PROJECT.md` for product context.
