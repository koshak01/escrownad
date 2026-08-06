-- forge/skeleton/seeds/demos.sql
--
-- Reference: the demo entity's table plus sample rows.
-- NOT a platform table — a teaching exhibit (see src/models/demo.rs). It lives
-- in the reference database so that /admin/demos/ shows admin CRUD for real.
-- It is deliberately absent from the platform schema, to keep every project's
-- schema from swelling.
--
-- Applied by hand, idempotently (CREATE TABLE IF NOT EXISTS + ON CONFLICT):
--
--     psql-18 -h 127.0.0.1 -U html -d 'escrownad.com' \
--          -f forge/skeleton/seeds/demos.sql
--
-- To remove the demo entity:  DROP TABLE IF EXISTS demos;
-- PLUS delete its whole Rust slice — the complete list of wiring points is in
-- the header of src/models/demo.rs, and a partial removal will not build.

-- ──────────────────────────────────────────────────────────────────────────
-- The table is a reference for the platform's canonical SQL types, including
-- where the platform goes against the industry default (see demo.rs, "Type
-- conventions"): money = bigint, NOT numeric or float; time = timestamp in UTC,
-- NOT timestamptz; PK = bigserial, never composite; logical uniqueness lives in
-- a hash column.
-- Housekeeping timestamps (_dat/_updated) are set by the database; the Rust model has none.
-- ──────────────────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS demos (
    dmo_id        bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,       -- PK: bigserial, ALWAYS
    dmo_hash      varchar(64) NOT NULL,                                  -- logical uniqueness = sha256(business key)
    dmo_code      varchar(512),
    dmo_title     varchar(1024),
    dmo_note      text,
    dmo_amount    bigint NOT NULL DEFAULT 0,                             -- MONEY: bigint (Price=FixedN<8>, raw i64). NOT numeric/float
    dmo_event_ts  timestamp without time zone NOT NULL DEFAULT now(),    -- event time: timestamp in UTC. NOT timestamptz
    dmo_is_enable boolean DEFAULT true NOT NULL,
    dmo_dat       timestamp without time zone DEFAULT now() NOT NULL,
    dmo_updated   timestamp without time zone DEFAULT now() NOT NULL
);

-- Code uniqueness (#[db(unique)] in the model) and the anchor for ON CONFLICT below.
CREATE UNIQUE INDEX IF NOT EXISTS demos_dmo_code_key ON demos (dmo_code);
-- Hash uniqueness (#[db(unique, hash)] → find_by_hash/delete_by_hash, plus dedup).
CREATE UNIQUE INDEX IF NOT EXISTS demos_dmo_hash_key ON demos (dmo_hash);

-- ──────────────────────────────────────────────────────────────────────────
-- A couple of sample rows, idempotent on dmo_code.
-- ──────────────────────────────────────────────────────────────────────────
-- pgcrypto: sha256 in the seed yields the SAME hash as Rust's `sha256_hex(dmo_code)`.
CREATE EXTENSION IF NOT EXISTS pgcrypto;

-- dmo_amount — raw Price (× 10^8): 10050000000 = 100.50. dmo_event_ts — now() UTC.
INSERT INTO demos (dmo_hash, dmo_code, dmo_title, dmo_note, dmo_amount, dmo_event_ts, dmo_is_enable)
VALUES
    (encode(digest('alpha', 'sha256'), 'hex'), 'alpha', 'First example', 'An active demo row',    10050000000, now(), true),
    (encode(digest('beta',  'sha256'), 'hex'), 'beta',  'Second example', 'A disabled demo row', 0,           now(), false)
ON CONFLICT (dmo_code) DO UPDATE
   SET dmo_title     = EXCLUDED.dmo_title,
       dmo_note      = EXCLUDED.dmo_note,
       dmo_amount    = EXCLUDED.dmo_amount,
       dmo_is_enable = EXCLUDED.dmo_is_enable,
       dmo_updated   = now();

-- ──────────────────────────────────────────────────────────────────────────
-- A menu entry for /admin/demos/, so the demo is clickable in the sidebar.
-- The page opens at /admin/demos/ regardless; the menu entry is optional.
--
-- The RELIABLE way is to create it through the admin area itself, at
-- /admin/menus/new/ — which doubles as a demonstration of working CRUD, and
-- lets you pick a real group from the dropdown.
--
-- The SQL below is only for when you know your mng_id for certain: on a fresh
-- database the 'admin' group may not exist, the subquery would return NULL, and
-- the entry would land outside any group and never render. Substitute the
-- mng_id / mng_code of your own database.
-- ──────────────────────────────────────────────────────────────────────────
-- INSERT INTO menus (mnu_descr, mnu_uri, mng_id, mnu_is_enable, mnu_order)
-- VALUES ('Demo (reference)', '/admin/demos/',
--         (SELECT mng_id FROM menus_groups WHERE mng_code = '<YOUR_GROUP>' LIMIT 1),
--         true, 100);
