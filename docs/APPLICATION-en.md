# Cleanverse Build — application (English, ready to paste)

Соответствует русской версии `APPLICATION-ru.md`. Нумерация та же — если
правишь пункт там, говори номер, поправлю здесь.

---

## Project Name

```
EscrowNad
```

## Track

```
RWA — Real-World Assets, Verified
```

## Team / Project Icon

`static/images/icon-512.png` — 512×512 PNG.

---

## Team Background

```
Thirty years in software, more than five hundred projects shipped to
production. Today we work as a small team with heavy automation: engineering
roles — from infrastructure to code review — are distributed across
specialised AI agents, each with its own area of responsibility. That lets a
small team move at the speed of a much larger one.

The project grew out of a real case of selling IP address blocks. Once you
get into it, transferring such an asset turns out to be a hard problem — and
the hard parts are not technical. They are trust and procedure.

The service platform is our own and already runs several products; the
contracts on Monad sit on top of it. Everything described below is running,
not planned.
```

---

## Project Description

```
EscrowNad is escrow that settles on external proof instead of counterparty
trust.

THE MARKET

Over the last twelve months the European registry recorded 4,809 IPv4
transfers — 4,070 of them actual sales rather than corporate mergers —
moving 25.5 million addresses. At a market price around $35 per address that
is roughly $900 million a year, and that is one registry out of five. Add 631
IPv6 block transfers and 1,034 autonomous system number transfers. These
numbers are verifiable: the registry publishes its transfer tables openly.

Yet this market runs on 2000s infrastructure. Deals are found through obscure
aggregator sites and Telegram channels where holders post their networks and
buyers post wanted ads. Our own case went exactly that way: a weekly message
in a channel and correspondence with whoever replied. The transfer itself is
done by writing letters to a registrar. A near-billion-dollar market operates
on email and classifieds, with no escrow, no counterparty verification, and
no single place where a deal is visible.

WHAT IS ACTUALLY BEING SOLD

Address resources are not owned in the ordinary sense: they are administered
by a regional registry, and participants hold a right of use and
registration. The holder of record is a legal entity, and all procedures run
through a local registrar. So "selling addresses" means transferring a right,
recorded by the registry — not selling a thing. Our design follows that: we
do not try to tokenise ownership, we hold the money until the registry shows
the right has moved.

Holders also frequently prefer not to be seen. It is normal here for the
beneficiary and the holder of record to be different: resources sit with a
subsidiary or an entity that hides the ultimate owner. A seller is willing to
transfer the right but not to publicly announce it is theirs. That is
ordinary business practice, and any solution for this market has to live
with it.

And many holders do not know what they own. Resources were obtained years ago
for a project, the project ended, the blocks stayed. Our own case started
exactly like that — addresses registered for a work task, and the existence
of an entire market discovered by accident. Prices are published nowhere and
the spread is wide: for one and the same block an intermediary offered the
holder $25 per address and resold at $45. Formally that is not fraud — one
side knows the market and the other does not. In practice the holder loses
half the value for having nothing to compare against.

THE PROBLEM IS WIDER THAN FRAUD

Even when both sides are honest legal entities, deals break for other
reasons: the transfer takes time, documents contain errors, one side may
behave badly mid-process. Meanwhile the money hangs between the parties with
no protection at all.

HOW IT WORKS

The buyer deposits USDC into a lock contract. From there the transfer is
confirmed neither by the seller nor by us, but by the official registry: an
observer watches the specific resource and waits for the transfer to appear
in the registry's public data. It appears — the money goes to the seller. It
does not — the deal does not close. Nobody is asked to vouch for anything.

Changing a holder is not an instant operation. It runs through a registrar:
the holder files a request, the registrar performs legal actions, and that
takes time. We do not interfere in those actions and cannot influence them —
a resource can be frozen, documents can come back for correction. So the
settlement is built to keep the money protected for as long as the process
runs.

When the deadline passes we deliberately do not do a simple automatic refund.
Returning the money to the buyer is not always fair: the seller may already
have started the transfer and lost the resource. So an expired deal goes to
arbitration, where it is established at what stage the process stopped. The
platform's goal is to get the deal completed, not to close it with a refund
at the first delay.

WHAT ALREADY WORKS

An escrow contract on Monad testnet with verified source
(0xe5289D23829ABE1Aa882c3355A65001de7294a46). Nine settled deals on the
public board — each one traceable to a real record in the registry's public
data. A 1% platform fee plus 1% accruing to an insurance fund whose balance
anyone can read on-chain. Operator review of every listing before it goes
public. And a full cycle completed on live chain: a buyer funded a deal, the
observer found the matching transfer record in the registry, and released the
money to the seller — with no human step in between.

WHY CLEANVERSE IS ESSENTIAL

We can prove the asset moved. We cannot prove who the parties are. Today a
counterparty is a wallet address we know nothing about, and that is exactly
where trust still leaks.

CVI closes it: an identity bound to a wallet, so a seller can demonstrate
they are the entity the resource is registered to, and access to deals is
limited to verified participants.

CVI also solves the privacy problem better than we can. A seller must prove
they are entitled to dispose of the resource without announcing it publicly.
We could only hide the data in our interface — but we would still see it, and
trust would simply move from the seller to us. With CVI the personal data
stays with its owner and only the fact of verification is exposed. We admit a
verified party to a deal while knowing nothing about them, and having no way
to find out.

CVA gives the asset itself a standard verified representation with
programmable rules, replacing the proof we hand-built for ourselves. And
since settlement already runs through a single contract call, Travel Rule
data has exactly one place to live.

We built the proof layer. Cleanverse is the identity and compliance layer
that turns it into a market institutions can actually use.

WHERE THIS GOES

The model works wherever three things hold: the asset has a holder,
ownership can be checked, and the fact of transfer is published by a source
both sides trust. Everything else — contract, fee, insurance fund,
arbitration, party verification — is already built and does not depend on
what is being sold. A new market is a new oracle module for us, not a new
product.

Address resources are the first entry because the registry publishes
transfers in machine-readable form for free. Next, in descending order of
source readiness: domain names, where registries publish ownership changes;
tokenised assets and NFTs, where the transfer is visible on-chain and needs
no confirmation at all — it is enough to tie settlement to the fact of
delivery; shares and specialised instruments, where the source is a registrar
or a depository. Different sources, the same settlement mechanics.

We are not building a universal marketplace for everything. We claim
something narrower: where the transfer of a right is authoritatively
recorded by someone, settlement can be tied to that record and trust removed
from the process. The list of such assets is finite and known.
```

---

## Cleanverse Integration Plan

```
WHERE IT CONNECTS

The product has three layers: proof that the asset moved (built), settlement
through a contract (built), and verification of who the parties are (missing).
Cleanverse fills the third, and it connects along boundaries that already
exist in the code.

CVI IS MANDATORY, NO EXCEPTIONS

No verified identity, no access to deals. No grey paths and no thresholds:
these are deals worth tens and hundreds of thousands of dollars, and on such a
market requiring verification is normal and expected. There are no small
deals here worth carving out an exception for.

PROOF OF CONTROL — VIA THE HOLDER CONTACT IN THE REGISTRY

This is a separate mechanism, taken from practice. The registry record lists
a contact address for the resource holder; that address is what the holder
uses to file with the registrar, and whoever controls it effectively controls
the resource. We require confirmation from that address: the seller must prove
they hold it. Verification runs through the registry contact, not through the
seller's word.

WHERE CVI FITS ON TOP OF THAT

The contact check proves control over the resource but says nothing about who
the person is. CVI closes the other half: a verified identity matched against
the entity named in the registry as the holder. Together that gives something
no intermediary on this market has today — proven control plus proven
identity.

CVA ON THE ASSET

A resource confirmed by a registry record is a verifiable asset with traceable
origin. Today we do that check ourselves: two independent sources, a hash of
the deal condition stored in the contract, and a fact key emitted in the
release event. Those fields are ready-made attachment points for a CVA
representation.

TRAVEL RULE IN SETTLEMENT

Settlement goes through a single contract call where both parties and the
amount are already known. One integration point; the flow does not need
rebuilding.

CCP ON REVIEW AND ARBITRATION

We already review every listing before publication, and an expired deal goes
to arbitration. Both places are asking to become programmable rules: what an
operator now checks by eye should be checked by a rule before the transaction.

ORDER OF WORK DURING THE HACKATHON

First CVI as a mandatory entry condition and matching the seller against the
holder named in the registry — that closes the largest trust gap and is
immediately visible in the interface. Then CVA on the asset and Travel Rule
in settlement.

AFTER THE HACKATHON

Compliance rules instead of manual review; moving the observer key off the
application server, so that encrypting party documents stops depending on
trusting us as the operator.
```

---

## Что заполнить самому

- **Team / Company** — название команды или юрлица.
- **Contact Email** — рабочая почта, на неё придёт доступ к API.
- **Business Plan / Deck** — необязательно, у нас нет; пропускаем.
