-- escrownad/seeds/deals_rename_canon.sql
-- Приведение таблицы `deals` к канону именования кузницы: КАЖДАЯ колонка
-- несёт префикс своей сущности (`del_`), как `usr_*` / `mnu_*` / `tlg_*`.
--
--     psql -h 127.0.0.1 -U html -d 'escrownad.com' -f seeds/deals_rename_canon.sql
--
-- ВНИМАНИЕ: ломающая миграция. Накатывать ОДНОВРЕМЕННО с выкатом кода —
-- старый бинарь после неё работать не будет, новый до неё тоже.
-- Порядок на сервере: git pull → cargo build --release → этот файл →
-- supervisorctl restart escrownad:*
--
-- Внешние ключи (`seller_usr_id`, `buyer_usr_id`, `broker_usr_id`) НЕ трогаем:
-- по канону они носят префикс целевой таблицы плюс роль — как `telegrams.usr_id`
-- и `menus.mng_id`.

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
    'Организация-владелец в реестре — по ней оракул ищет строку перехода';
COMMENT ON COLUMN deals.del_to_org IS
    'Организация-получатель в реестре; на публикации пуста';
COMMENT ON COLUMN deals.del_rir IS
    'Региональный реестр: RIPE|ARIN|APNIC|LACNIC|AFRINIC';
COMMENT ON COLUMN deals.del_geo IS
    'Гео блока для витрины — в поиске факта не участвует';
COMMENT ON COLUMN deals.del_prefix IS
    'Сеть целиком. Наружу до оплаты уходит только маска — см. may_see_prefix';

COMMIT;
