-- A lot, represented on chain as a verified asset (Cleanverse A-Token).
--
-- One lot is one token: it stands for the right to a specific block, and it is
-- issued the moment an operator approves the listing — before the lot reaches
-- the board. Every transfer of that token is gated by the identity check built
-- into the token itself, so the asset cannot move to an unverified wallet even
-- outside this platform.
--
-- Issuance is not instant: the request is submitted, then reviewed and minted
-- on their side. So two columns — the request while it is in flight, and the
-- address once it exists.
--
-- Both are nullable and not retroactive: lots approved before this exist
-- without a token, and nothing breaks because of it.
--
--   psql -h 127.0.0.1 -U html -d 'escrownad.com' -f seeds/deals_asset_token.sql

BEGIN;

ALTER TABLE deals ADD COLUMN IF NOT EXISTS del_asset_request varchar(128);
ALTER TABLE deals ADD COLUMN IF NOT EXISTS del_asset_token   varchar(64);

COMMENT ON COLUMN deals.del_asset_request IS
    'Issuance request id while the asset is being minted; cleared once issued';
COMMENT ON COLUMN deals.del_asset_token IS
    'Address of the verified asset standing for this lot, once issued';

COMMIT;
