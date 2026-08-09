# EscrowNad — Submission Summary

**Track 01 — RWA: Real-World Assets, Verified** · Live: [escrownad.com](https://escrownad.com)

## Problem

The transfer market for internet number resources — IPv4 and IPv6 blocks, ASNs
— runs on 2000s infrastructure. Over the last twelve months the European
registry alone recorded 4,809 IPv4 transfers, 4,070 of them actual sales,
moving 25.5 million addresses: roughly $900M a year at market price, and that
is one registry out of five.

Yet deals are found on aggregator sites and in Telegram channels, and executed
by writing letters to a registrar. There is no escrow, no counterparty
verification, and no single place where a deal is visible. Prices are published
nowhere: for one and the same block an intermediary offered the holder $25 per
address and resold at $45.

The asset is unusual: address resources are administered by a regional
registry, so what changes hands is a **right of use recorded by the registry**,
not a thing that can be tokenised as ownership. Transfers take time, run through
a registrar, and can be frozen or returned for correction. Meanwhile the money
hangs between the parties with no protection at all. Holders also routinely
require privacy — the beneficiary and the holder of record are often different
entities on purpose.

## Solution

**Escrow that settles on external proof instead of counterparty trust.**

The buyer deposits USDC into a lock contract. The transfer is confirmed neither
by the seller nor by us, but by the official registry: an observer watches the
specific resource and waits for the transfer to appear in the registry's public
data. It appears — the money is released to the seller. It does not — the deal
does not close. Nobody is asked to vouch for anything.

The oracle reads two independent sources: registry transfer tables and an RDAP
query against the network itself (RDAP also covers ARIN, APNIC, LACNIC,
AFRINIC). Because the process is slow by nature, an expired deal goes to
arbitration rather than an automatic refund — the seller may already have
started the transfer and lost the resource.

**Already running:** verified escrow contract on live chain; ten settled deals
on the public board (tab **Settled**), each tied to a registry fact; 1% platform
fee plus 1% into an insurance fund at a separate address whose balance anyone
can read on-chain (fee cap of 5% hard-wired via `MAX_TOTAL_BPS`); operator review
of every listing; Cleanverse identity on both parties and verified assets from
issuance; and a full cycle completed on chain — buyer funded, observer matched
the registry record, money released, no human step in between. On each deal
card, Lock / Release / Contract open the Monad explorer so the chain state is
one click away.

## CVI · CVA Integration Points

We built the proof layer and the settlement layer. The identity layer is where
trust still leaked: a counterparty used to be a wallet address we knew nothing
about. That gate now exists, and it lives **in the contract rather than around
it** — Cleanverse CVI is not a UI badge.

### CVI — identity gate on-chain (source)

`EscrowLock.fund` calls `complianceVerify` on the Cleanverse CCP validator for
**both** parties before a single token moves:

```solidity
// contracts/EscrowLock.sol — function fund(...)
if (address(validator) != address(0)) {
    if (!validator.complianceVerify(address(this), msg.sender)) {
        revert NotCompliant(msg.sender);   // buyer
    }
    if (!validator.complianceVerify(address(this), seller)) {
        revert NotCompliant(seller);       // seller
    }
}
```

- Contract: `contracts/EscrowLock.sol`
- Validator interface: `contracts/IAPassComplianceValidator.sol`
- An unverified wallet cannot sit on either side of a deal; no interface trick
  gets around it, because the check sits where the money does.
- The validator address is set after deployment (`setValidator`), not in the
  constructor: a compliance pool is registered with the validator *by its own
  address*, which does not exist until the contract is on chain.

Around that hard gate the application does the courteous half: `query_apass` at
sign-in and `isCompliant(address)` before spending gas
(`src/cleanverse/core.rs`). Someone without an identity is told so and handed
the Cleanverse magic link. Personal data never reaches us — only the fact of
verification.

### CVA — verified asset from issuance (source)

When an operator approves a listing, the lot is issued as a Cleanverse verified
asset (`launch_asset` → `/atoken/launch` in `src/cleanverse/core.rs`) with a
transfer rule built into the token (`AssetRule` in `src/cleanverse/types.rs`).
Default rule: any valid identity (this market is international; tighter tiers
and country lists are available per issuance when a deal needs them).

A transfer to a wallet that fails the rule reverts on-chain — on this platform
or off it. The rule is shown on the deal card next to the asset address. We also
support wrapping the settlement token (`launch_wrapped_asset` /
`/atoken/launch_wrapped_atoken`) so compliance can follow the money after
release, not only entry into the deal.

| Primitive | Integration point | State |
|---|---|---|
| **CVI — parties** | `EscrowLock.fund` → `complianceVerify` for buyer **and** seller | **In contract, live** |
| **CVI — UX** | `query_apass` / `isCompliant` before gas; magic link if missing | **Live** |
| **CVI — privacy** | Personal data stays with the owner; we only see verification fact | **By design (CVI)** |
| **CVI — proof of control** | Registry holder contact confirms control of the resource; CVI confirms who the controller is | Complements existing check |
| **CVA — asset** | Issue on approve with `AssetRule`; rule on deal card; optional wrapped USDC | **Live (issue + card); wrap API wired** |
| **Travel Rule — settlement** | Single contract call; both parties and amount known | Ready attachment point |
| **CCP — review & arbitration** | Listing review and expired-deal arbitration | Manual today; natural for programmable rules |

## Deployed Chains

| Chain | Chain ID | Status |
|---|---|---|
| **Monad testnet** | `10143` | **Deployed and live** (2026-08-06) |
| Monad mainnet | `143` | Not yet deployed |

**Monad testnet contracts**

| What | Address |
|---|---|
| **EscrowLock** (verified source) | [`0x4c9A68831E51853b981EF3e2f1461cdD46430da4`](https://testnet.monadexplorer.com/address/0x4c9A68831E51853b981EF3e2f1461cdD46430da4) |
| USDC (Circle, 6 decimals) | [`0x534b2f3A21130d7a60830c2Df862319e593943A3`](https://testnet.monadexplorer.com/address/0x534b2f3A21130d7a60830c2Df862319e593943A3) |
| Insurance fund | [`0xe8b8C85e929b67C91c42a793670A88c6d563A962`](https://testnet.monadexplorer.com/address/0xe8b8C85e929b67C91c42a793670A88c6d563A962) |

Settlement asset is Circle USDC on Monad; native MON is gas only. RPC
`https://testnet-rpc.monad.xyz`, explorer `testnet.monadexplorer.com`.

**Live demo:** [escrownad.com](https://escrownad.com) · board
[escrownad.com/deals/?scope=settled](https://escrownad.com/deals/?scope=settled) ·
oracle [escrownad.com/oracle/](https://escrownad.com/oracle/)

---

**Where this goes.** The model works wherever three things hold: the asset has a
holder, holding can be checked, and the transfer is published by a source both
sides trust. Everything else — contract, fee, insurance fund, arbitration, party
verification — is already built and independent of what is sold. A new market is
a new oracle module, not a new product. Next in descending order of source
readiness: domain names, tokenised assets and NFTs, then shares and specialised
instruments held by a registrar or depository.
