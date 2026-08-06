-- escrownad/seeds/deals_rename_canon.sql
-- Bringing the `deals` table in line with the naming convention: EVERY column
-- carries its entity's prefix (`del_`), the way `usr_*` / `mnu_*` / `tlg_*` do.
--
--     psql -h 127.0.0.1 -U html -d 'escrownad.com' -f seeds/deals_rename_canon.sql
--
-- WARNING: a breaking migration. Apply it TOGETHER with the code release — the
-- old binary will not work after it, and the new one will not work before it.
-- Order on the server: git pull → cargo build --release → this file →
-- supervisorctl restart escrownad:*
--
-- Foreign keys (`seller_usr_id`, `buyer_usr_id`, `broker_usr_id`) are left
-- alone: by convention they carry the target table's prefix plus a role, as in
-- `telegrams.usr_id` and `menus.mng_id`.

BEGIN;

ALTER TABLE deals RENAME COLUMN asset_type      TO del_asset_type;
ALTER TABLE deals RENAME COLUMN resource_kind   TO del_resource_kind;
ALTER TABLE deals RENAME COLUMN prefix          TO del_prefix;
ALTER TABLE deals RENAME COLUMN listing_side    TO del_listing_side;
ALTER TABLE deals RENAME COLUMN from_org        TO del_from_org;
ALTER TABLE deals RENAME COLUMN to_org          TO del_to_org;
ALTER TABLE deals RENAME COLUMN rir             TO del_rir;
ALTER TABLE deals RENAME COLUMN geo             TO del_geo;
ALTER TABLE deals RENAME COLUMN seller_wallet   TO del_seller_wallet;
ALTER TABLE deals RENAME COLUMN buyer_wallet    TO del_buyer_wallet;
ALTER TABLE deals RENAME COLUMN chain_id        TO del_chain_id;
ALTER TABLE deals RENAME COLUMN lock_tx         TO del_lock_tx;
ALTER TABLE deals RENAME COLUMN release_tx      TO del_release_tx;
ALTER TABLE deals RENAME COLUMN deadline_ts     TO del_deadline_ts;
ALTER TABLE deals RENAME COLUMN ripe_match_key  TO del_ripe_match_key;
ALTER TABLE deals RENAME COLUMN checklist_json  TO del_checklist_json;
ALTER TABLE deals RENAME COLUMN soft_verified   TO del_soft_verified;
ALTER TABLE deals RENAME COLUMN contact_email   TO del_contact_email;

COMMENT ON COLUMN deals.del_from_org IS
    'Holder organisation in the registry — the oracle looks for a transfer row by it';
COMMENT ON COLUMN deals.del_to_org IS
    'Receiving organisation in the registry; empty at listing time';
COMMENT ON COLUMN deals.del_rir IS
    'Regional registry: RIPE|ARIN|APNIC|LACNIC|AFRINIC';
COMMENT ON COLUMN deals.del_geo IS
    'Where the block is, for display — takes no part in matching the fact';
COMMENT ON COLUMN deals.del_prefix IS
    'The full network. Only a mask goes out before funding — see may_see_prefix';

COMMIT;
