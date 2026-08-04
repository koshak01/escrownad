-- escrownad/seeds/deals.sql
-- Domain: proof-escrow deals. Asset v1 = IPv4 (RIPE PA|PI).
--
--     psql -h 127.0.0.1 -U html -d 'escrownad.com' -f seeds/deals.sql

CREATE TABLE IF NOT EXISTS deals (
    del_id            bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    del_hash          varchar(64) NOT NULL,
    del_title         varchar(1024),
    del_note          text,
    -- asset (IP)
    resource_kind     varchar(8) NOT NULL DEFAULT 'PI',   -- PA | PI
    prefix            varchar(64) NOT NULL,               -- e.g. 176.120.88.0/21
    from_org          varchar(512),
    to_org            varchar(512),
    -- parties (wallet strings for demo; user_id later)
    seller_wallet     varchar(128),
    buyer_wallet      varchar(128),
    seller_usr_id     bigint,
    buyer_usr_id      bigint,
    -- money + chain
    del_amount        bigint NOT NULL DEFAULT 0,          -- FixedN<8> / Price raw
    chain_id          varchar(64) NOT NULL DEFAULT 'monad',
    lock_tx           varchar(128),
    release_tx        varchar(128),
    -- lifecycle
    del_status        varchar(32) NOT NULL DEFAULT 'draft',
    -- draft|listed|funded|awaiting_proof|released|refunded|dispute|cancelled
    deadline_ts       timestamp without time zone,
    ripe_match_key    varchar(512),
    checklist_json    text,
    del_is_enable     boolean NOT NULL DEFAULT true,
    del_dat           timestamp without time zone NOT NULL DEFAULT now(),
    del_updated       timestamp without time zone NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX IF NOT EXISTS deals_del_hash_key ON deals (del_hash);
CREATE INDEX IF NOT EXISTS deals_status_idx ON deals (del_status) WHERE del_is_enable;
CREATE INDEX IF NOT EXISTS deals_prefix_idx ON deals (prefix);

CREATE EXTENSION IF NOT EXISTS pgcrypto;

-- demo row for list smoke
INSERT INTO deals (
    del_hash, del_title, del_note, resource_kind, prefix,
    from_org, to_org, seller_wallet, del_amount, del_status, deadline_ts
)
VALUES (
    encode(digest('demo-pi-176.120.88.0/21', 'sha256'), 'hex'),
    'IPv4 /21 PI transfer (demo)',
    'Fixture deal for UI smoke. RIPE PI assignment example.',
    'PI',
    '176.120.88.0/21',
    'Tochka Opory LLC',
    'IT PARK JSC',
    '0xSellerDemo0000000000000000000000000001',
    150000000000,  -- 1500.00 × 1e8
    'listed',
    now() + interval '7 days'
)
ON CONFLICT (del_hash) DO UPDATE
   SET del_title   = EXCLUDED.del_title,
       del_status  = EXCLUDED.del_status,
       del_updated = now();

-- menu item under admin group (optional sidebar)
INSERT INTO menus (mnu_uri, mnu_descr, mnu_order, mnu_is_enable, mnu_hash, mng_id)
SELECT '/deals/', 'Deals', 50, true,
       md5(random()::text || clock_timestamp()::text), g.mng_id
FROM menus_groups g
WHERE g.mng_code = 'admin'
  AND NOT EXISTS (SELECT 1 FROM menus m WHERE m.mnu_uri = '/deals/' AND m.mng_id = g.mng_id);

INSERT INTO roles2menus (rol_id, mnu_id, r2m_is_enable)
SELECT r.rol_id, m.mnu_id, true
FROM roles r CROSS JOIN menus m
WHERE r.rol_code = 'admin' AND m.mnu_uri = '/deals/'
ON CONFLICT (rol_id, mnu_id) DO NOTHING;
