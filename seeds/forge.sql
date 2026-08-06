-- escrownad/seeds/forge.sql
--
-- Platform seed for a FRESH `escrownad.com` database.
-- The platform ships schema only; this file is the minimum an admin area needs:
-- the admin role, the admin menu, a root user, their links, template structure.
--
-- Run it AFTER the platform schema. The password is never in git:
--
--     psql -h 127.0.0.1 -U html -d "escrownad.com" \
--          -v admin_email="adm@escrownad.com" -v admin_pass="SECRET" \
--          -f seeds/forge.sql
--
-- Convention: the admin email is adm@<domain>. The password comes from a local
-- file that is not in git and is passed as -v admin_pass=...
-- The platform stores md5(password) in users.usr_md5_pass.
-- Idempotent.

-- ── 1. roles ─────────────────────────────────────────────────────────────────
INSERT INTO roles (rol_code, rol_descr, rol_is_enable, rol_is_default, rol_hash)
VALUES
    ('admin',   'Administrator', true, false, md5(random()::text || clock_timestamp()::text)),
    ('manager', 'Manager',       true, false, md5(random()::text || clock_timestamp()::text))
ON CONFLICT (rol_code) DO UPDATE
   SET rol_descr     = EXCLUDED.rol_descr,
       rol_is_enable = EXCLUDED.rol_is_enable;

-- ── 2. menus_groups ──────────────────────────────────────────────────────────
INSERT INTO menus_groups (mng_code, mng_descr, mng_uri, mng_hash)
VALUES ('admin', 'Admin menu', '/admin/', md5(random()::text || clock_timestamp()::text))
ON CONFLICT (mng_code) DO UPDATE
   SET mng_descr = EXCLUDED.mng_descr,
       mng_uri   = EXCLUDED.mng_uri;

-- ── 3. menus — the platform admin pages (forge_admin::pages::all()) ──────────
INSERT INTO menus (mnu_uri, mnu_descr, mnu_order, mnu_is_enable, mnu_hash, mng_id)
SELECT v.uri, v.descr, v.ord, true,
       md5(random()::text || clock_timestamp()::text || v.descr), g.mng_id
FROM (VALUES
    ('/admin/',              'System',           100),
    ('/admin/menus/',        'Menus',            110),
    ('/admin/menus-groups/', 'Menu groups',      120),
    ('/admin/roles/',        'Roles',            130),
    ('/admin/users/',        'Users',            140),
    ('/admin/telegrams/',    'Telegram channels',150),
    ('/admin/templates/',    'Templates',        160),
    ('/admin/constants/',    'Constants',        170),
    ('/admin/styles/',       'Style',           1000)
) AS v(uri, descr, ord)
CROSS JOIN menus_groups g
WHERE g.mng_code = 'admin'
  AND NOT EXISTS (SELECT 1 FROM menus m WHERE m.mnu_uri = v.uri AND m.mng_id = g.mng_id);

-- ── 4. roles2menus: admin sees every entry ───────────────────────────────────
INSERT INTO roles2menus (rol_id, mnu_id, r2m_is_enable)
SELECT r.rol_id, m.mnu_id, true
FROM roles r CROSS JOIN menus m
WHERE r.rol_code = 'admin'
ON CONFLICT (rol_id, mnu_id) DO NOTHING;

-- ── 5. constants ─────────────────────────────────────────────────────────────
-- Branding only. Bot credentials are not in git; they go in via /admin/constants/.
INSERT INTO constants (cnt_code, cnt_descr, cnt_value_json, cnt_is_enable)
VALUES
    ('site_title', 'Project name in the header and footer', '"EscrowNad"'::jsonb, true),
    ('domain',     'Primary domain',                      '"escrownad.com"'::jsonb, true)
ON CONFLICT (cnt_code, cnt_is_enable) DO UPDATE
   SET cnt_descr      = EXCLUDED.cnt_descr,
       cnt_value_json = EXCLUDED.cnt_value_json;

-- ── 6. telegrams — the general / errors channels (chat_id NULL until set) ────
INSERT INTO telegrams (tlg_code, tlg_descr, tlg_external_id, tlg_topic_id, tlg_is_enable, tlg_hash)
VALUES
    ('general',
     'General EscrowNad channel — startup and info',
     NULL, NULL, true,
     md5(random()::text || clock_timestamp()::text)),
    ('errors',
     'EscrowNad error channel — server and render failures',
     NULL, NULL, true,
     md5(random()::text || clock_timestamp()::text))
ON CONFLICT (tlg_code) DO UPDATE
   SET tlg_descr     = EXCLUDED.tlg_descr,
       tlg_is_enable = EXCLUDED.tlg_is_enable,
       tlg_updated   = NOW();

-- ── 7. templates — startup → general, error → errors ─────────────────────────
INSERT INTO templates (tpl_code, tpl_template, tpl_descr, tlg_id, tpl_hash, tpl_is_enable)
VALUES
    ('startup',
     E'*EscrowNad is up*\n{{ now | date(format="%Y-%m-%d %H:%M") }}',
     'Startup notice when notifier comes up',
     (SELECT tlg_id FROM telegrams WHERE tlg_code = 'general'),
     md5(random()::text || clock_timestamp()::text), true),
    ('error',
     E'*Error* {{ source }}\n{{ text }}',
     'Automatic system-error alert (register_error_hooks)',
     (SELECT tlg_id FROM telegrams WHERE tlg_code = 'errors'),
     md5(random()::text || clock_timestamp()::text), true)
ON CONFLICT (tpl_code) DO UPDATE
   SET tpl_template = EXCLUDED.tpl_template,
       tpl_descr    = EXCLUDED.tpl_descr,
       tlg_id       = EXCLUDED.tlg_id,
       tpl_updated  = NOW();

-- ── 8. users — root admin ────────────────────────────────────────────────────
INSERT INTO users (usr_email, usr_md5_pass, usr_is_enable, usr_is_staff, usr_hash, usr_descr)
VALUES (:'admin_email', md5(:'admin_pass'), true, true,
        md5(random()::text || clock_timestamp()::text), 'root admin EscrowNad')
ON CONFLICT (usr_email) DO UPDATE
   SET usr_md5_pass  = EXCLUDED.usr_md5_pass,
       usr_is_enable = true,
       usr_is_staff  = true;

-- ── 9. users2roles — the admin role for the root user ────────────────────────
INSERT INTO users2roles (usr_id, rol_id, u2r_is_enable)
SELECT u.usr_id, r.rol_id, true
FROM users u CROSS JOIN roles r
WHERE u.usr_email = :'admin_email' AND r.rol_code = 'admin'
ON CONFLICT (usr_id, rol_id) DO NOTHING;
