# Compliance model — how CVI and CVA sit in EscrowNad

EscrowNad settles the sale of an internet number resource (an IPv4/IPv6 block or
an ASN) on external proof from the registry, not on trust between the parties.
This document describes where Cleanverse's primitives are wired into that flow —
the part a reviewer most wants to see, and the part that is easy to fake with a
badge and hard to fake in the contract.

The short version: **the identity gate is inside the settlement contract, not
around it, and the asset carries its transfer rule on-chain.** Everything below
is a pointer to real code, not a description of intent.

## 1. Three layers, one missing piece filled

| Layer | What it proves | State |
|---|---|---|
| Proof | the resource actually moved (registry) | built — `src/observer/` |
| Settlement | the money moves against that proof | built — `contracts/EscrowLock.sol` |
| Identity | who the two parties are | **filled by CVI, in the contract** |

Before Cleanverse, a counterparty was a wallet address we knew nothing about —
that is exactly where trust still leaked. CVI closes it, and CVA turns the
verified resource into a programmable asset.

## 2. CVI — the identity gate is on-chain

`EscrowLock.fund` calls `complianceVerify` on the Cleanverse CCP validator for
**both** parties — the buyer paying and the seller receiving — before a single
token moves. An unverified wallet cannot end up on either side of a deal, and no
interface trick gets around it, because the check sits where the money does.

- Contract: `contracts/EscrowLock.sol`
- Validator interface it calls: `contracts/IAPassComplianceValidator.sol`
- The validator address is set after deployment (`setValidator`), not in the
  constructor: a compliance pool is registered with the validator *by its own
  address*, which does not exist until the contract is on chain. Wiring it at
  construction time would lock the contract out of its own registration.

Around that hard gate the application does the courteous half so a user is never
surprised by a failed transaction:

- at sign-in it asks `query_apass` whether the wallet holds an identity;
- before spending gas it asks the contract itself via `isCompliant(address)`;
- someone without an identity is told so and handed the link to obtain one.

Personal data never reaches us — only the fact of verification. Client code:
`src/cleanverse/core.rs` (`query_apass`, `is_verified`, `verify_apass`).

## 3. CVA — the asset carries its rule

When an operator approves a listing, the lot is issued as a verified asset
(`launch_asset` → `/atoken/launch`), with a transfer rule built into the token
itself. This is compliance embedded *from the issuance stage*, which is what
Track 01 asks for: the asset exists with its rules attached before it is visible
to anyone.

The rule is `AssetRule` (`src/cleanverse/types.rs`):

| Field | Meaning |
|---|---|
| `min_tier` | minimum identity tier; their check is *greater than*, so `1` means "any valid identity" |
| `allowed_group` / `allowed_sub_group` | institutional whitelisting, when a deal calls for it |
| `countries` | ISO codes; empty means no jurisdiction restriction |
| `is_black_list` | deprecated on their side; always false |

A transfer to a wallet that does not satisfy the rule reverts on-chain — on this
platform or off it. That rule is now shown on the deal card next to the token,
so the restriction is legible, not buried.

### Why the default is "any valid identity" and not a tighter tier

This is a deliberate compliance decision, not a gap. The market is
international and the resource itself carries no nationality; gating by tier or
country would exclude legitimate holders for no gain. So the default rule is a
valid identity and nothing more.

The capability to tighten is there in the same `AssetRule` shape — a higher
`min_tier` for accredited-only holding, `allowed_group` for institutional
whitelisting, `countries` for a jurisdiction limit — and is applied per issuance
when a specific deal warrants it, rather than imposed blanket where it does not
fit. Restriction where it is justified, not restriction for show.

## 4. CCP, Travel Rule, and where they attach next

- **Travel Rule — settlement.** Settlement is already a single contract call
  where both parties and the amount are known. One integration point; the flow
  does not need rebuilding.
- **CCP — review and arbitration.** Every listing is reviewed by an operator
  before publication, and an expired deal goes to arbitration rather than an
  automatic refund. Both are manual today and are the natural place for
  programmable pre-transaction rules.
- **CVA on the proof.** The oracle already produces the attachment points a CVA
  representation needs: two independent registry sources, a hash of the deal
  condition stored in the contract, and a fact key emitted in the `Released`
  event.

## 5. Verify it yourself

- Contract source is verified on the Monad testnet explorer — compare it against
  `contracts/EscrowLock.sol` line by line.
- `src/observer/` depends only on `serde`, `reqwest` and `chrono`; the registry
  endpoints it reads are public.
- The deployed addresses and the recorded live run are in `contracts/DEPLOY.md`.
