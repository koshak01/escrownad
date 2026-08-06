# EscrowNad — Submission Summary

**Track 01 — RWA: Real-World Assets, Verified** · Live: `escrownad.com`

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

**Already running:** verified escrow contract on live chain; nine settled deals
on the public board, each traceable to a real registry record; 1% platform fee
plus 1% into an insurance fund at a separate address whose balance anyone can
read on-chain (fee cap of 5% hard-wired via `MAX_TOTAL_BPS`); operator review of
every listing; and a full cycle completed on chain — buyer funded, observer
matched the registry record, money released, no human step in between.

## CVI · CVA Integration Points

We built the proof layer and the settlement layer. The identity layer is where
trust still leaked: a counterparty used to be a wallet address we knew nothing
about. That gate now exists, and it lives in the contract rather than around it.

**The identity check is on-chain.** `EscrowLock.fund` calls `complianceVerify`
on the CCP validator for **both** parties — the buyer paying and the seller
receiving — before a single token moves. An unverified wallet cannot end up on
either side of a deal, and no interface trick gets around it, because the check
sits where the money does. Source: `contracts/EscrowLock.sol`, interface in
`contracts/IAPassComplianceValidator.sol`.

The validator address is set after deployment rather than in the constructor,
deliberately: a compliance pool is registered with the validator *by its own
address*, which does not exist until the contract is on chain. Wiring it at
construction time would lock the contract out of its own registration.

Around that gate the application does the courteous half: it asks `query_apass`
at sign-in and, before spending gas, asks the contract itself via
`isCompliant(address)`. Someone without an identity is told so and handed the
link to get one, instead of discovering the requirement through a failed
transaction. Personal data never reaches us — only the fact of verification.

| Primitive | Integration point |
|---|---|
| **CVI — parties** | **Enforced in `EscrowLock.fund` for both sides.** Mandatory, no exceptions and no thresholds: no verified identity, no deal. A verified identity is matched against the entity named in the registry as the holder of record. |
| **CVI — privacy** | A seller must prove entitlement without announcing it publicly. We could only hide data in our own interface — trust would just move from the seller to us. With CVI personal data stays with its owner and only the fact of verification is exposed. |
| **CVI — proof of control** | Complements our existing mechanism: the registry record lists a holder contact address, and we require confirmation from it. That proves control of the resource; CVI proves who the controller is. |
| **CVA — asset** | A resource confirmed by a registry record is a verifiable asset with traceable origin. Existing attachment points: two independent sources, a hash of the deal condition stored in the contract, and a fact key emitted in the release event. |
| **Travel Rule — settlement** | Settlement is a single contract call where both parties and the amount are already known. One integration point; no flow rebuild. |
| **CCP — review & arbitration** | Listing review and expired-deal arbitration are manual today. Both are asking to become programmable pre-transaction rules. |

**Order of work:** CVI as mandatory gate plus seller-to-holder matching first —
largest trust gap, immediately visible in the UI. Then CVA on the asset and
Travel Rule in settlement.

## Deployed Chains

| Chain | Chain ID | Status |
|---|---|---|
| **Monad testnet** | `10143` | **Deployed and live** (2026-08-06) |
| Monad mainnet | `143` | Not yet deployed |

**Monad testnet contracts**

| What | Address |
|---|---|
| **EscrowLock** (verified source) | `0x4c9A68831E51853b981EF3e2f1461cdD46430da4` |
| USDC (Circle, native — 6 decimals) | `0x534b2f3A21130d7a60830c2Df862319e593943A3` |
| Insurance fund | `0xe8b8C85e929b67C91c42a793670A88c6d563A962` |

Settlement asset is native Circle USDC, not a wrapper; native MON is used for
gas only. RPC `https://testnet-rpc.monad.xyz`, explorer
`testnet.monadexplorer.com`.

---

**Where this goes.** The model works wherever three things hold: the asset has a
holder, ownership can be checked, and the transfer is published by a source both
sides trust. Everything else — contract, fee, insurance fund, arbitration, party
verification — is already built and independent of what is sold. A new market is
a new oracle module, not a new product. Next in descending order of source
readiness: domain names, tokenised assets and NFTs, then shares and specialised
instruments held by a registrar or depository.
