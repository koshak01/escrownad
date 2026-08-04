# EscrowNad

Proof-escrow on **Monad**: funds release when an external oracle confirms the
condition. First asset: **IPv4 (RIPE PA + PI)**.

Forge app (skeleton pattern). Peer: **Янус** (`escrownad`). Domain/DB:
`escrownad.com`.

## Layout (forge)

```
escrownad/
├── Cargo.toml
├── src/
│   ├── lib.rs              # APP_NAME, sockets, DbCommand/DbResponse
│   ├── models/             # Demo (эталон) + Deal
│   ├── pages/              # index, deals, oracle + admin/demos
│   ├── observer/           # RIPE PA+PI match
│   ├── chain/              # mock lock txs (live later)
│   └── bin/
│       ├── database.rs
│       ├── notifier.rs
│       ├── ws.rs
│       ├── web.rs
│       └── observer.rs
├── etc/                    # database / notifier / ws / web / redis
├── templates/
├── static/                 # css + brand/
├── seeds/                  # forge.sql, demos.sql, deals.sql
├── contracts/EscrowLock.sol
└── deploy/
```

Side-by-side with forge:

```
~/work/rust/
├── forge/
└── escrownad/
```

## Dev (ports 4100 / 4101)

```bash
# schema once (DB escrownad.com already exists)
psql -h 127.0.0.1 -U html -d 'escrownad.com' -f ../forge/docs/db_schema.sql
psql -h 127.0.0.1 -U html -d 'escrownad.com' \
  -v admin_email="adm@escrownad.com" -v admin_pass="$(tr -d '\n' < pass.txt)" \
  -f seeds/forge.sql
psql -h 127.0.0.1 -U html -d 'escrownad.com' -f seeds/demos.sql -f seeds/deals.sql

cargo build --bins
# order: database → notifier → ws → web
./target/debug/escrownad-database &
./target/debug/escrownad-notifier &
./target/debug/escrownad-ws &
./target/debug/escrownad-web &
# or: mprocs
```

| | |
|---|---|
| Site | http://localhost:4100/ |
| Deals | http://localhost:4100/deals/ |
| Oracle | http://localhost:4100/oracle/ |
| Admin | http://localhost:4100/admin/ |
| Login | `adm@escrownad.com` — password in local `pass.txt` (not in git) |

## Demo path (hackathon / video)

1. Open http://localhost:4100/ — brand + links  
2. http://localhost:4100/oracle/ — what is the oracle  
3. http://localhost:4100/deals/ — list (open + released)  
4. Open a deal card — status / checklist / release_tx  
5. Reset a deal to `awaiting_proof` and run observer:

```bash
psql -h 127.0.0.1 -U html -d 'escrownad.com' -c \
  "UPDATE deals SET del_status='awaiting_proof', release_tx=NULL, ripe_match_key=NULL
   WHERE prefix='176.120.88.0/21';"
OBSERVER_ONCE=1 ./target/debug/escrownad-observer
# reload deal card → status released, release_tx mock:ripe:…
```

6. Admin login → core forge CRUD still works  

## Product flow

1. Seller lists IP block (PA|PI) + from/to condition  
2. Buyer funds on-chain lock with **native MON** (`payable`; mock until deploy). USDC later.  

3. `escrownad-observer` polls RIPE PA + PI JSON  
4. Match → release seller; timeout → refund  

Contract source: `contracts/EscrowLock.sol`. Live deploy needs Foundry + Monad
testnet keys (not required for mock demo video).

See `PROJECT.md` for product context.
