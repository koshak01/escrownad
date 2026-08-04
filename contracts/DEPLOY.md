# Deploy notes — EscrowLock (USDC)

Settlement asset: **USDC (ERC-20, 6 decimals)**. Native MON = gas only.

## Contracts

| File | Role |
|---|---|
| `MockUSDC.sol` | Dev/testnet faucet token if no official USDC |
| `EscrowLock.sol` | fund / release / refund USDC |
| `IERC20.sol` | minimal interface |

## Addresses (fill after deploy)

| Network | USDC | EscrowLock | Observer EOA |
|---|---|---|---|
| Monad testnet | _TBD — use MockUSDC if none_ | _TBD_ | _TBD_ |
| Local anvil | MockUSDC deploy | EscrowLock(usdc, observer) | anvil key |

```text
# Foundry (when installed)
forge create contracts/MockUSDC.sol:MockUSDC --rpc-url $MONAD_RPC --private-key $PK
forge create contracts/EscrowLock.sol:EscrowLock \
  --constructor-args $USDC $OBSERVER \
  --rpc-url $MONAD_RPC --private-key $PK
# mint demo USDC
cast send $USDC "mint(address,uint256)" $BUYER 1000000000  # 1000 USDC
```

## Buyer flow

1. `usdc.approve(escrowLock, amount)`
2. `escrowLock.fund(dealId, seller, amount, deadline, conditionHash)`
3. Observer: RIPE match → `release(dealId, ripeKey)` moves USDC to seller
