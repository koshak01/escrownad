-- Wallet members: role "user" (NOT admin), site menu only, configs schema.
-- Idempotent. Run on escrownad.com after forge.sql + users2wallets.sql.

-- ── 1. Role: user (default for wallet sign-in) ───────────────────────────────
INSERT INTO roles (rol_code, rol_descr, rol_is_enable, rol_is_default, rol_hash)
VALUES (
    'user',
    'Product user — offers / deals only (wallet login)',
    true,
    true,
    md5(random()::text || clock_timestamp()::text)
)
ON CONFLICT (rol_code) DO UPDATE
   SET rol_descr      = EXCLUDED.rol_descr,
       rol_is_enable  = true,
       rol_is_default = true;

-- ── 2. Site menu group (public product chrome, not /admin/) ──────────────────
INSERT INTO menus_groups (mng_code, mng_descr, mng_uri, mng_hash)
VALUES (
    'site',
    'Product menu',
    '/deals/',
    md5(random()::text || clock_timestamp()::text)
)
ON CONFLICT (mng_code) DO UPDATE
   SET mng_descr = EXCLUDED.mng_descr,
       mng_uri   = EXCLUDED.mng_uri;

-- ── 3. Product menus under site ──────────────────────────────────────────────
INSERT INTO menus (mnu_uri, mnu_descr, mnu_order, mnu_is_enable, mnu_hash, mng_id)
SELECT v.uri, v.descr, v.ord, true,
       md5(random()::text || clock_timestamp()::text || v.uri),
       g.mng_id
FROM (VALUES
    ('/deals/',     'Offers',     100),
    ('/deals/new/', 'Add offer',  110),
    ('/cabinet/',   'My deals',   120),
    ('/oracle/',    'Oracle',     130)
) AS v(uri, descr, ord)
CROSS JOIN menus_groups g
WHERE g.mng_code = 'site'
  AND NOT EXISTS (
      SELECT 1 FROM menus m
      WHERE m.mnu_uri = v.uri AND m.mng_id = g.mng_id
  );

-- ── 4. user role → only site product menus ───────────────────────────────────
INSERT INTO roles2menus (rol_id, mnu_id, r2m_is_enable)
SELECT r.rol_id, m.mnu_id, true
FROM roles r
CROSS JOIN menus m
JOIN menus_groups g ON g.mng_id = m.mng_id AND g.mng_code = 'site'
WHERE r.rol_code = 'user'
ON CONFLICT (rol_id, mnu_id) DO NOTHING;

-- ── 5. configs2users schema (wallet_address in operator settings) ────────────
INSERT INTO constants (cnt_code, cnt_descr, cnt_value_json, cnt_is_enable)
VALUES (
    'configs2users',
    'Per-user settings schema (wallet login)',
    '{
      "wallet_address": {
        "type": "string",
        "label": "Wallet address",
        "desc": "EVM address used to sign in (linked via users2wallets + this setting)",
        "ui": "text",
        "readonly": true,
        "order": 1
      }
    }'::jsonb,
    true
)
ON CONFLICT (cnt_code, cnt_is_enable) DO UPDATE
   SET cnt_descr      = EXCLUDED.cnt_descr,
       cnt_value_json = EXCLUDED.cnt_value_json;

-- ── 6. Backfill: any wallet-linked user without roles gets "user" ────────────
INSERT INTO users2roles (usr_id, rol_id, u2r_is_enable)
SELECT DISTINCT u2w.usr_id, r.rol_id, true
FROM users2wallets u2w
CROSS JOIN roles r
WHERE r.rol_code = 'user'
  AND NOT EXISTS (
      SELECT 1 FROM users2roles u2r
      WHERE u2r.usr_id = u2w.usr_id AND u2r.rol_id = r.rol_id
  )
ON CONFLICT (usr_id, rol_id) DO NOTHING;

-- ── 7. Backfill configs2users.wallet_address from users2wallets ───────────────
INSERT INTO configs2users (usr_id, c2u_values)
SELECT u2w.usr_id, jsonb_build_object('wallet_address', u2w.u2w_address)
FROM users2wallets u2w
ON CONFLICT (usr_id) DO UPDATE
SET c2u_values = configs2users.c2u_values || EXCLUDED.c2u_values,
    c2u_updated = now();
