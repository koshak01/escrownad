-- forge/skeleton/seeds/demos.sql
--
-- ЭТАЛОН: таблица доменной demo-сущности + демо-строки.
-- НЕ ядерная таблица — учебный экспонат (см. skeleton/src/models/demo.rs).
-- Живёт в эталонной forge-БД, чтобы /admin/demos/ показывал admin-CRUD вживую.
-- В core docs/db_schema.sql её НЕТ намеренно — чтобы не пухла схема всех проектов.
--
-- Накат руками (идемпотентно: CREATE TABLE IF NOT EXISTS + ON CONFLICT):
--
--     psql-18 -h 127.0.0.1 -U html -d 'forge.foothold.me' \
--          -f forge/skeleton/seeds/demos.sql
--
-- Убрать demo, если не нужна:  DROP TABLE IF EXISTS demos;
-- ПЛЮС снести весь Rust-срез — ПОЛНЫЙ список точек проводки (иначе build не
-- соберётся) в шапке skeleton/src/models/demo.rs.

-- ──────────────────────────────────────────────────────────────────────────
-- Таблица — ЭТАЛОН каноничных SQL-типов forge. ⚠️ Где forge идёт против
-- индустриального дефолта (см. модель demo.rs, раздел «КАНОН ТИПОВ»):
--   деньги = bigint (НЕ numeric/float); время = timestamp UTC (НЕ timestamptz);
--   PK = bigserial (НЕ составной); логический уникум = hash-колонка.
-- Служебные таймстампы (_dat/_updated) проставляет БД, в Rust-модели их нет.
-- ──────────────────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS demos (
    dmo_id        bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,       -- PK: bigserial, ВСЕГДА
    dmo_hash      varchar(64) NOT NULL,                                  -- логич. уникум = sha256(бизнес-ключ)
    dmo_code      varchar(512),
    dmo_title     varchar(1024),
    dmo_note      text,
    dmo_amount    bigint NOT NULL DEFAULT 0,                             -- ДЕНЬГИ: bigint (Price=FixedN<8>, raw i64). ⚠️ НЕ numeric/float
    dmo_event_ts  timestamp without time zone NOT NULL DEFAULT now(),    -- время события: timestamp UTC. ⚠️ НЕ timestamptz
    dmo_is_enable boolean DEFAULT true NOT NULL,
    dmo_dat       timestamp without time zone DEFAULT now() NOT NULL,
    dmo_updated   timestamp without time zone DEFAULT now() NOT NULL
);

-- Уникальность кода (#[db(unique)] в модели) + опора для ON CONFLICT ниже.
CREATE UNIQUE INDEX IF NOT EXISTS demos_dmo_code_key ON demos (dmo_code);
-- Уникальность hash (#[db(unique, hash)] → find_by_hash/delete_by_hash + дедуп).
CREATE UNIQUE INDEX IF NOT EXISTS demos_dmo_hash_key ON demos (dmo_hash);

-- ──────────────────────────────────────────────────────────────────────────
-- Пара демонстрационных строк (idempotent по dmo_code).
-- ──────────────────────────────────────────────────────────────────────────
-- pgcrypto — sha256 в seed даёт ТОТ ЖЕ hash, что Rust `sha256_hex(dmo_code)`.
CREATE EXTENSION IF NOT EXISTS pgcrypto;

-- dmo_amount — raw Price (× 10^8): 10050000000 = 100.50. dmo_event_ts — now() UTC.
INSERT INTO demos (dmo_hash, dmo_code, dmo_title, dmo_note, dmo_amount, dmo_event_ts, dmo_is_enable)
VALUES
    (encode(digest('alpha', 'sha256'), 'hex'), 'alpha', 'Первый пример', 'Активная demo-запись',    10050000000, now(), true),
    (encode(digest('beta',  'sha256'), 'hex'), 'beta',  'Второй пример', 'Выключенная demo-запись', 0,           now(), false)
ON CONFLICT (dmo_code) DO UPDATE
   SET dmo_title     = EXCLUDED.dmo_title,
       dmo_note      = EXCLUDED.dmo_note,
       dmo_amount    = EXCLUDED.dmo_amount,
       dmo_is_enable = EXCLUDED.dmo_is_enable,
       dmo_updated   = now();

-- ──────────────────────────────────────────────────────────────────────────
-- Пункт меню на /admin/demos/ (чтобы demo был кликабелен в сайдбаре).
-- Страница и так открывается по URL /admin/demos/ — пункт меню опционален.
--
-- НАДЁЖНЫЙ способ: завести через саму админку — /admin/menus/new/ (это заодно
-- демонстрация работающего CRUD). Выберешь реальную группу из выпадающего.
--
-- SQL-вариант ниже — только если ТОЧНО знаешь свой mng_id: на свежей БД группы
-- 'admin' может не быть, тогда подзапрос вернёт NULL и пункт окажется вне
-- группы (в сайдбаре не отрисуется). Подставь mng_id/mng_code своей БД.
-- ──────────────────────────────────────────────────────────────────────────
-- INSERT INTO menus (mnu_descr, mnu_uri, mng_id, mnu_is_enable, mnu_order)
-- VALUES ('Demo (эталон)', '/admin/demos/',
--         (SELECT mng_id FROM menus_groups WHERE mng_code = '<ТВОЯ_ГРУППА>' LIMIT 1),
--         true, 100);
