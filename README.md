# EscrowNad

Proof-escrow / oracle-escrow on **Monad**. Money stays locked until an external
oracle confirms the condition. First asset module: **IPv4 (RIPE PA + PI)**.

| | |
|---|---|
| Peer | `escrownad` / **Янус** |
| Domain / DB | `escrownad.com` |
| Chain (demo) | **Monad only** |
| Stack | forge (4 bins) + Rust observer + Solidity (next) |

## Dev

```bash
# ports: web 4100, ws 4101
cargo build --bins
./target/debug/escrownad-database &
./target/debug/escrownad-notifier &
./target/debug/escrownad-ws &
./target/debug/escrownad-web &

# or: mprocs
```

- Site: <http://localhost:4100/>
- Admin: <http://localhost:4100/admin/>
- DB: `psql -h 127.0.0.1 -U html -d 'escrownad.com'`

### Admin login (dev)

| | |
|---|---|
| email | `adm@escrownad.com` |
| password | local `pass.txt` (not in git) |

## Product (v1)

1. Seller lists an IP block + proof-of-holdership condition (RIPE).
2. Buyer picks a deal and deposits into the on-chain lock.
3. Observer watches RIPE transfer tables (PA + PI JSON).
4. Fact match → release to seller; timeout → refund.

See `PROJECT.md` for full product context.
