# Deploy notes — EscrowLock (USDC)

The settlement asset is **USDC (ERC-20, 6 decimals)**. Native MON is for gas only.

## Contracts

| File | Role |
|---|---|
| `EscrowLock.sol` | the lock: fund / release / refund USDC |
| `IERC20.sol` | minimal token interface |
| `MockUSDC.sol` | unused — Monad has the real USDC from Circle |

## Addresses

### Monad testnet (chain id 10143) — deployed 2026-08-06

| What | Address |
|---|---|
| USDC (Circle) | `0x534b2f3A21130d7a60830c2Df862319e593943A3` |
| **EscrowLock** | `0x4c9A68831E51853b981EF3e2f1461cdD46430da4` |
| Observer (also the owner) | `0xC1B8d6B5CbB542e0c8Ae89AA5aBa43518a3282d0` |
| Treasury (platform fee) | `0xf04f664095cdc9d1121ee938db89282fcd616cdb` |
| **Insurance fund** | `0xe8b8C85e929b67C91c42a793670A88c6d563A962` |

Deployment transaction:
`0x87cb2f7d35622096eaef2f3620e4d83442dae87e40ab4bd819ba7af143337900`

Previous versions — `0x3CB2C5EA…80be5`, `0x90CB5117…6972A`, `0x5491c3De…81f46`;
not in use.

## Fees

**2% is paid by the buyer only**, on top of the price. The asset owner receives
their price in full and pays the platform nothing.

| Charge | Rate | Destination |
|---|---|---|
| `feeBps` | `100` = 1% | platform treasury |
| `insuranceBps` | `100` = 1% | insurance fund |

The cap on the combined charges is `MAX_TOTAL_BPS` = 500 (5%), hardcoded into
the contract. `setFee` physically will not let anyone set more — not even the owner.

| | |
|---|---|
| who pays | the buyer, on top of the price |
| when it is taken | only on `release`, i.e. only when the deal has gone through |
| on a refund | everything goes back to the buyer, both charges included |

Both amounts are pinned down at the moment of `fund` and written into the deal.
Raising the rate after the fact for an already funded lock is impossible — that
is protection against ourselves.

**The insurance fund is a separate address**, not a tally in a database. Anyone
can see its balance in the explorer: whatever has been accumulated for disputed
cases is exactly what sits there. No promises of payouts beyond what actually
exists.

The client asks the contract for the full amount due: `quote(amount)` →
`(total, fee, insuranceFee)`. The rates are not duplicated in the interface or
in the database — one source of truth.

Changing rates and recipients: `setFee(feeBps, insuranceBps, treasury, insurance)`,
owner only.

### Monad mainnet (chain id 143) — not deployed yet

| What | Address |
|---|---|
| USDC (Circle) | `0x754704Bc059F8C67012fEd69BC8A327a5aafb603` |
| EscrowLock | — |

## Verified run (testnet, 2026-08-06)

The full money cycle worked end to end:

Price 1 USDC, charges 1% + 1%:

| Step | Result |
|---|---|
| `quote(1 USDC)` | total `1.020000`, fee `0.010000`, insurance `0.010000` |
| `approve(lock, 1.02)` + `fund` | `1.020000` in the lock |
| `release(dealId, ripeKey)` | seller `1.000000`, treasury `0.010000`, fund `0.010000`, lock empty |

The seller got the price in full — the charges never touched them.
Insurance fund balance after the run: `0.010000 USDC`.

A note on the run: on testnet the treasury and the seller are **the same
address**, so their balances add up together. The fund is separate, and it can
be seen cleanly.

## Deployment

```bash
export PATH="$HOME/.foundry/bin:$PATH"
set -a; source .env; set +a

forge create contracts/EscrowLock.sol:EscrowLock \
  --rpc-url "$MONAD_RPC" \
  --private-key "$OBSERVER_PRIVATE_KEY" \
  --broadcast \
  --constructor-args "$USDC_ADDRESS" "$OBSERVER_ADDRESS"
```

**Careful:** `--broadcast` must come BEFORE `--constructor-args` — otherwise the
parser swallows the flag as a third constructor argument.

Building the contracts: `forge build` (this is forge from Foundry, not our own
forge). `foundry.toml` points at `src = "contracts"`, because `src/` holds Rust.

## The buyer's money flow

1. `usdc.approve(escrowLock, amount)`
2. `escrowLock.fund(dealId, seller, amount, deadline, conditionHash)`
3. The observer, on the fact of RIPE: `release(dealId, ripeKey)` → USDC to the seller

Emergency exit: if the observer stays silent and the deadline has passed, the
buyer calls `refundAfterDeadline(dealId)` themselves and takes the money back.

## What goes on-chain

Fingerprints only: `dealId` (hash of the deal) and `conditionHash` (hash of the
condition). Neither the network, nor the organizations, nor the description of
the lot is on-chain — that stays in the database. Amounts and addresses are
always visible on a public chain; hiding them without zk is not possible.

## Environment variables

| Variable | Purpose |
|---|---|
| `CHAIN_MODE` | `live` — work against the chain, otherwise mock |
| `MONAD_RPC` | `https://testnet-rpc.monad.xyz` |
| `MONAD_CHAIN_ID` | `10143` |
| `USDC_ADDRESS` | USDC address from the table above |
| `ESCROW_LOCK_ADDRESS` | lock address from the table above |
| `OBSERVER_PRIVATE_KEY` | observer key — **environment only**, never in git |

Live mode switches on only when both the lock address and the key are set.
Otherwise the service stays in mock and logs a warning about it — so that an
incomplete configuration does not take production down.
