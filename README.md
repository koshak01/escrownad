# EscrowNad

Buying a block of IP addresses today means sending money to a stranger and trusting them to file the paperwork afterwards. EscrowNad holds the buyer's money in a contract and pays the seller only when the official internet registry publishes the transfer.

If the registry never shows the transfer, the money never moves. Neither the seller, nor the buyer, nor we can release it by simply claiming the deal is done.

## Links

| | |
|---|---|
| Live site | https://escrownad.com |
| Public deal board | https://escrownad.com/deals/ |
| Oracle page | https://escrownad.com/oracle/ |
| Escrow contract — Monad testnet (chain id 10143), source verified | `0x4c9A68831E51853b981EF3e2f1461cdD46430da4` |
| Contract in the explorer | https://testnet.monadexplorer.com/address/0x4c9A68831E51853b981EF3e2f1461cdD46430da4 |
| Insurance fund (separate address, balance readable by anyone) | `0xe8b8C85e929b67C91c42a793670A88c6d563A962` |
| Settlement asset — USDC by Circle, Monad testnet | `0x534b2f3A21130d7a60830c2Df862319e593943A3` |
| What comes next | [ROADMAP.md](ROADMAP.md) |

Native MON is used for gas only. All settlement is in USDC.

## The market

Over the last twelve months the European regional internet registry recorded:

- **4,809 IPv4 transfers**, of which **4,070** were sales rather than mergers or corporate reorganisations;
- **25.5 million addresses** changing holder;
- at a market price around **$35 per address** that is roughly **$900 million a year**;
- plus **631 IPv6 block transfers** and **1,034 autonomous system number transfers**.

These figures are checkable: the registry publishes its transfer tables openly, in machine-readable form, at the same URLs our oracle reads. And this is **one registry out of five** — the other four cover North America, Asia-Pacific, Latin America and Africa.

The infrastructure this market runs on is from the 2000s. Deals are found on aggregator sites and in Telegram channels where holders post their networks and buyers post wanted ads; the transfer itself is arranged by writing letters to a registrar. There is no escrow, no counterparty verification, and no single place where a deal is visible. Price discovery is equally poor: for one and the same block an intermediary offered the holder $25 per address and resold at $45 — not fraud, just one side knowing the market and the other not.

**What is actually being sold.** Address resources are not owned in the ordinary sense. They are administered by the registry, and a participant holds a right of use and registration, recorded by that registry. So we do not attempt to tokenise ownership. We hold the money until the registry shows the right has moved.

**The problem is wider than fraud.** Even between two honest companies deals break: the transfer takes time, documents come back for correction, a resource can be frozen, one side can behave badly mid-process. Meanwhile the money hangs between the parties with no protection at all.

## How it works

1. **Seller lists** the resource. The registry is queried directly for holder, resource type and country, instead of trusting what the seller typed.
2. **Buyer funds the lock.** `approve` on USDC, then `fund(dealId, seller, amount, deadline, conditionHash)` from the buyer's own wallet. The server re-checks the fact on-chain.
3. **The observer watches the registry** — not the seller, not us — through two independent sources:

   | Source | What it answers |
   |---|---|
   | Registry transfer tables (public JSON) | "this block moved from A to B on date X" — authoritative, but a record takes time to appear |
   | RDAP query on the specific network | "who holds this network right now" — immediate, works across all five registries |

4. **The fact appears → the money is released** to the seller in a single call, with the fact key written into the `Released` event. No human step in between.
5. **The fact does not appear → nothing moves.** Nobody is asked to vouch for anything.
6. **The deadline passes → arbitration.** We deliberately do not do an automatic refund on expiry: the seller may already have started the transfer and lost the resource, so returning the money to the buyer is not automatically the fair outcome. The goal is to complete the deal, not to close it at the first delay. As an emergency exit that does not depend on us, the buyer can always call `refundAfterDeadline(dealId)` directly and take the money back if the observer goes silent.

**What goes on chain:** only fingerprints — `dealId` and `conditionHash`. The network, the organisations and the listing text stay off-chain. Amounts and addresses are visible, as on any public chain; hiding those would require zero-knowledge proofs and a different chain.

## What already works

- **Nine settled deals** on the public board, each traceable to a real record in the registry's public data.
- **A full cycle on live chain:** a buyer funded a deal from their own wallet, the observer found the matching registry record, and released the money to the seller with no manual step.
- **Fee: 1% to the platform plus 1% to an insurance fund**, paid by the buyer on top of the price. The seller receives the price in full and pays the platform nothing. Fees are charged only on `release`, i.e. only on a deal that actually completed; a refund returns everything, both fees included. The rates are fixed into the deal at `fund` time, so they cannot be raised retroactively on an already funded lock, and `MAX_TOTAL_BPS = 500` is hardcoded — `setFee` will not accept more even from the owner.
- **The insurance fund is a separate address**, not a number in a database. Its balance is readable by anyone in the explorer: whatever has accrued for disputed cases is what is actually there. Nothing is promised beyond it.
- **`quote(amount)` is the single source of truth** for the total the buyer pays. Rates are not duplicated in the interface or in the database.
- **Every listing is reviewed** by an operator before it goes public.
- **The exact network is hidden until the deal is funded.** Outsiders see size, registry, country and price — enough to decide, not enough to go around the platform.

## Cleanverse integration

We can prove the asset moved. We cannot prove who the parties are. Today a counterparty is a wallet address we know nothing about, and that is exactly where trust still leaks.

**Where it connects.** The product has three layers: proof that the asset moved (built), settlement through a contract (built), and verification of who the parties are (missing). Cleanverse fills the third, and it attaches along boundaries that already exist in the code.

**CVI is mandatory, with no exceptions.** No verified identity, no access to deals. No grey paths and no thresholds: these are deals worth tens and hundreds of thousands of dollars, and on such a market requiring verification is normal and expected. There are no small deals here worth carving out an exception for.

CVI also solves a privacy problem we cannot solve ourselves. Holders here frequently prefer not to be seen — it is ordinary practice for the beneficiary and the holder of record to differ, with resources sitting in a subsidiary. A seller must prove they are entitled to dispose of the resource without announcing publicly that it is theirs. We could only hide that data in our interface, and we would still see it: trust would move from the seller to us. With CVI the personal data stays with its owner and only the fact of verification is exposed. We admit a verified party to a deal while knowing nothing about them and having no way to find out.

**Proof of control, via the registry contact.** This is a separate mechanism, taken from practice, and it runs alongside CVI. The registry record lists a contact address for the resource holder; that address is what the holder uses to file with the registrar, so whoever controls it effectively controls the resource. We require confirmation from that address. Verification runs through the registry, not through the seller's word.

**Where CVI fits on top of that.** The contact check proves control over the resource but says nothing about who the person is. CVI closes the other half: a verified identity matched against the entity the registry names as holder. Together that gives something no intermediary on this market has today — proven control plus proven identity.

**CVA on the asset.** A resource confirmed by a registry record is a verifiable asset with traceable origin. Today we perform that check ourselves: two independent sources, a hash of the deal condition stored in the contract, and a fact key emitted in the release event. Those fields are ready-made attachment points for a CVA representation, replacing proof we hand-built with a standard one carrying programmable rules.

**Travel Rule in settlement.** Settlement already goes through a single contract call where both parties and the amount are known. One integration point; the flow does not need rebuilding.

**CCP on review and arbitration.** We already review every listing before publication, and an expired deal goes to arbitration. Both are asking to become programmable rules: what an operator now checks by eye should be checked by a rule before the transaction.

**Order of work.** First CVI as a mandatory entry condition, plus matching the seller against the holder named in the registry — that closes the largest remaining trust gap and is immediately visible in the interface. Then CVA on the asset and Travel Rule in settlement. After that, compliance rules replacing manual review.

### The identity gate is in the contract, not around it

`EscrowLock.fund` calls `complianceVerify` on the Cleanverse CCP validator for **both** parties before a single token moves — the buyer paying and the seller receiving. An unverified wallet cannot end up on either side of a deal, and no amount of interface trickery gets around it, because the check lives where the money does. See `contracts/EscrowLock.sol` and the interface it calls in `contracts/IAPassComplianceValidator.sol`.

The validator address is set after deployment rather than in the constructor, and that is deliberate: a compliance pool is registered with the validator *by its own address*, which does not exist until the contract is on chain. Wiring it up at construction time would lock the contract out of its own registration. `setValidator(0)` is what a fresh deployment starts from; pointing it at the validator switches the gate on.

## Verifying this yourself

Be aware of one limitation before you clone: **the Rust services cannot be built from this repository.** They depend on an in-house platform library referenced by relative path in `Cargo.toml`, and that library is not public, so `cargo build` fails at dependency resolution. We are not going to dress that up.

Here is what does not depend on it and can be checked independently:

**1. The contracts.** Self-contained Solidity, buildable with Foundry alone:

```bash
forge build     # foundry.toml points src at contracts/
```

- `contracts/EscrowLock.sol` (336 lines) — `fund` / `release` / `refund` / `refundAfterDeadline`, `quote`, the fee cap, the CVI identity gate, the events.
- `contracts/IAPassComplianceValidator.sol` — the Cleanverse CCP validator interface the gate calls into.
- `contracts/IERC20.sol` — minimal token interface.
- `contracts/DEPLOY.md` — deployed addresses, fee model, the recorded live run.

The deployed source is verified on the explorer, so you can compare it against this repository line by line.

**2. The oracle logic.** `src/observer/` depends only on `serde`, `reqwest` and `chrono` — no platform code:

- `src/observer/mod.rs` (171 lines) — the two registry transfer-table endpoints, the transfer record shape, block matching, deal matching, fact key derivation.
- `src/observer/rdap.rs` (183 lines) — the RDAP lookup, holder / resource type / country parsing.

Both files can be read end to end in a few minutes, and the endpoints they read are public — you can fetch the same JSON and check the matching rules against it by hand.

**3. The live system.** https://escrownad.com is running: the deal board, the oracle page, the insurance fund balance. Everything settled is on Monad testnet and visible in the explorer at the contract address above.

## Limits we state up front

- **Metadata is public.** Who paid whom, how much, when. That is what a public chain is.
- **The exact network must remain readable to the oracle.** Encrypt it and automatic proof becomes impossible — verification traded for privacy.
- **The observer key currently lives on the application server.** Until it moves to a separate machine, an operator can read what the observer can read. That move is on the roadmap and is stated as a limit, not as a feature.
- **One observer key is a single point of trust.** A multi-signature oracle with release on quorum is planned, not built.
- **Insurance rules are not defined.** The fund accrues and its balance is on-chain. What counts as a claim and who decides is not settled yet, so nothing is promised.
- **Testnet only.** Mainnet addresses and USDC are known; the move is a configuration change plus an audit.

## Where this goes

The model works wherever three things hold: the asset has a holder, holding can be checked, and the fact of transfer is published by a source both sides trust. Everything else — contract, fee, insurance fund, arbitration, party verification — is already built and does not depend on what is being sold. A new market is a new oracle module, not a new product.

Address resources come first because the registry publishes transfers in machine-readable form for free. Next, in descending order of source readiness: domain names, where registries publish ownership changes; tokenised assets and NFTs, where the transfer is visible on-chain and needs no external confirmation at all; shares and specialised instruments, where the source is a registrar or a depository.

We are not building a universal marketplace. The claim is narrower: where the transfer of a right is authoritatively recorded by someone, settlement can be tied to that record and trust removed from the process. The list of such assets is finite and known.

## Repository layout

```
escrownad/
├── contracts/        # EscrowLock.sol, IERC20.sol, DEPLOY.md — buildable with Foundry
├── src/
│   ├── observer/     # oracle: registry transfer tables + RDAP (no platform dependency)
│   ├── chain/        # contract calls
│   ├── models/       # deals
│   ├── pages/        # site
│   └── bin/          # services (need the private platform library)
├── templates/
├── static/
├── foundry.toml
└── ROADMAP.md
```

## Licence

MIT.
