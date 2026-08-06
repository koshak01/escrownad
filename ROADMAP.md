# EscrowNad — roadmap

Where the product is today and what comes next. Written 6 August 2026.

## Today

Escrow that settles on **external proof**, not on trust between the parties.
First asset class: IPv4 blocks transferred through RIPE.

| Piece | State |
|---|---|
| Escrow contract on Monad (USDC) | live on testnet, `0xe5289D23829ABE1Aa882c3355A65001de7294a46`, source verified |
| Oracle: RIPE transfer tables | live — watches the network, waits for a **new** transfer record |
| Oracle: registry lookup over RDAP | live — reads holder, resource type, country straight from the registry |
| Buyer pays from own wallet | live — approve + fund from Phantom, server re-checks the fact on-chain |
| Fee 1% + 1% to insurance fund | live, fund balance shown on the board and readable by anyone in the explorer |
| Deadline refund without us | live — `refundAfterDeadline`, buyer takes the money back if the observer goes silent |
| Network hidden until funded | live — outsiders see size, registry, country and price, not the exact block |

## Next

**Encryption under wallets.** Envelope scheme: listing details are encrypted with
a random key; that key is sealed separately for each reader — seller, oracle,
and the buyer once the deal is funded. Adding a reader means adding an envelope,
not re-encrypting the data.

Wallets no longer decrypt (`eth_decrypt` was removed from MetaMask, Phantom EVM
never had it), so the reader's key is derived from a deterministic signature:
sign a fixed message, get the same key every time. No wallet — nothing to read,
because the database holds ciphertext.

Honest limits of this, stated up front:

- metadata stays public: who paid whom, how much, when. That is what a public
  chain is;
- the exact network must stay readable to the oracle, otherwise automatic proof
  is impossible. Encrypt it and you trade verification for privacy;
- while the observer's private key sits in the same database, an operator can
  open the observer's envelopes. The scheme becomes real only when that key
  moves to a separate machine.

Public keys are already being collected: every wallet sign-in recovers one from
the signature, no extra step for the user.

**Moderation.** New listings go to a review queue; a Telegram message carries
only a link, the decision is made in the admin panel where the whole listing is
visible — text, network, organisation, documents.

**Block-list checks.** «Clean, no spam» is a seller's claim today. Spamhaus DROP
and similar lists are public and can be pulled the same way RIPE tables are,
turning the claim into a checkable fact with a date.

**More registries.** RDAP already answers for ARIN, APNIC, LACNIC and AFRINIC —
tested. Transfer tables in machine-readable form exist only at RIPE, so
elsewhere the holder change is the proof.

**More resource types.** IPv6 blocks and ASN transfers are published by RIPE in
the same shape and are already in the demo board.

## Later

- **Multi-signature oracle.** One key is a single point of trust. Several
  independent watchers, release on quorum.
- **Observer key off the application server.** See the honest limit above.
- **Insurance rules.** The fund accrues and is visible on-chain. What counts as
  a claim, who decides, and where the money comes from before the fund grows —
  none of that is defined yet, so nothing is promised.
- **Mainnet.** Contract addresses and USDC for Monad mainnet are known; the move
  is a configuration change plus an audit.
- **Tokenised rights.** An NFT standing for a block makes the asset transferable
  without touching the registry every time; the oracle then ties the token to
  reality.
- **Private settlement track.** Hiding amounts and parties needs zero-knowledge
  proofs, which is a different chain and a different project — our team is
  building that on Aleo separately.

## Not doing

**Chat and haggling between parties.** The model stays fixed-price: see it, buy
it. Negotiation belongs outside the escrow.
