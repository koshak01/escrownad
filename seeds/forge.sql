-- escrownad/seeds/forge.sql
--
-- Ядерный сид forge для СВЕЖЕЙ БД `escrownad.com`.
-- `forge/docs/db_schema.sql` — schema-only. Этот файл — минимум для админки:
-- роль admin, admin-меню, root-юзер, связки, templates structure.
--
-- Прогон (ПОСЛЕ наката ../forge/docs/db_schema.sql), пароль НЕ в git:
--
--     psql -h 127.0.0.1 -U html -d "escrownad.com" \
--          -v admin_email="adm@escrownad.com" -v admin_pass="СЕКРЕТ" \
--          -f seeds/forge.sql
--
-- Конвенция коллектива: admin-email — adm@escrownad.com (НЕ p.elagin, НЕ admin@).
-- Пароль — из локального pass.txt (не в git), передаётся -v admin_pass=...
-- forge хранит md5(пароля) в users.usr_md5_pass.
-- Idempotent.

-- ── 1. roles ─────────────────────────────────────────────────────────────────
INSERT INTO roles (rol_code, rol_descr, rol_is_enable, rol_is_default, rol_hash)
VALUES
    ('admin',   'Администратор', true, false, md5(random()::text || clock_timestamp()::text)),
    ('manager', 'Менеджер',      true, false, md5(random()::text || clock_timestamp()::text))
ON CONFLICT (rol_code) DO UPDATE
   SET rol_descr     = EXCLUDED.rol_descr,
       rol_is_enable = EXCLUDED.rol_is_enable;

-- ── 2. menus_groups ──────────────────────────────────────────────────────────
INSERT INTO menus_groups (mng_code, mng_descr, mng_uri, mng_hash)
VALUES ('admin', 'Админ-меню', '/admin/', md5(random()::text || clock_timestamp()::text))
ON CONFLICT (mng_code) DO UPDATE
   SET mng_descr = EXCLUDED.mng_descr,
       mng_uri   = EXCLUDED.mng_uri;

-- ── 3. menus — ядерные admin-страницы (forge_admin::pages::all()) ────────────
INSERT INTO menus (mnu_uri, mnu_descr, mnu_order, mnu_is_enable, mnu_hash, mng_id)
SELECT v.uri, v.descr, v.ord, true,
       md5(random()::text || clock_timestamp()::text || v.descr), g.mng_id
FROM (VALUES
    ('/admin/',              'Система',          100),
    ('/admin/menus/',        'Меню',             110),
    ('/admin/menus-groups/', 'Группы меню',      120),
    ('/admin/roles/',        'Роли',             130),
    ('/admin/users/',        'Пользователи',     140),
    ('/admin/telegrams/',    'Telegram-каналы',  150),
    ('/admin/templates/',    'Шаблоны',          160),
    ('/admin/constants/',    'Константы',        170),
    ('/admin/styles/',       'Стиль',           1000)
) AS v(uri, descr, ord)
CROSS JOIN menus_groups g
WHERE g.mng_code = 'admin'
  AND NOT EXISTS (SELECT 1 FROM menus m WHERE m.mnu_uri = v.uri AND m.mng_id = g.mng_id);

-- ── 4. roles2menus: admin видит все пункты ───────────────────────────────────
INSERT INTO roles2menus (rol_id, mnu_id, r2m_is_enable)
SELECT r.rol_id, m.mnu_id, true
FROM roles r CROSS JOIN menus m
WHERE r.rol_code = 'admin'
ON CONFLICT (rol_id, mnu_id) DO NOTHING;

-- ── 5. constants ─────────────────────────────────────────────────────────────
-- Только бренд. Bot-creds — не в git; через /admin/constants/ позже.
INSERT INTO constants (cnt_code, cnt_descr, cnt_value_json, cnt_is_enable)
VALUES
    ('site_title', 'Название проекта в шапке/футере', '"EscrowNad"'::jsonb, true),
    ('domain',     'Основной домен',                  '"escrownad.com"'::jsonb, true)
ON CONFLICT (cnt_code, cnt_is_enable) DO UPDATE
   SET cnt_descr      = EXCLUDED.cnt_descr,
       cnt_value_json = EXCLUDED.cnt_value_json;

-- ── 6. telegrams — каналы general / errors (chat_id = NULL до настройки) ─────
INSERT INTO telegrams (tlg_code, tlg_descr, tlg_external_id, tlg_topic_id, tlg_is_enable, tlg_hash)
VALUES
    ('general',
     'Общий канал EscrowNad — startup и info',
     NULL, NULL, true,
     md5(random()::text || clock_timestamp()::text)),
    ('errors',
     'Канал ошибок EscrowNad — server/render fails',
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
     E'*EscrowNad запущен*\n{{ now | date(format="%Y-%m-%d %H:%M") }}',
     'Стартовое уведомление при подъёме notifier',
     (SELECT tlg_id FROM telegrams WHERE tlg_code = 'general'),
     md5(random()::text || clock_timestamp()::text), true),
    ('error',
     E'*Ошибка* {{ source }}\n{{ text }}',
     'Авто-алерт системной ошибки (register_error_hooks)',
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

-- ── 9. users2roles — admin-роль root-юзеру ───────────────────────────────────
INSERT INTO users2roles (usr_id, rol_id, u2r_is_enable)
SELECT u.usr_id, r.rol_id, true
FROM users u CROSS JOIN roles r
WHERE u.usr_email = :'admin_email' AND r.rol_code = 'admin'
ON CONFLICT (usr_id, rol_id) DO NOTHING;
