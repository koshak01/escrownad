-- escrownad/seeds/chain.sql
-- Настройки расчётного слоя (Monad + USDC) — константа `chain`.
--
--     psql -h 127.0.0.1 -U html -d 'escrownad.com' -f seeds/chain.sql
--
-- Канон проекта: параметры и ключи живут в таблице `constants` и правятся
-- через /admin/constants/. Ни окружения, ни файлов на диске.
--
-- ВАЖНО: `observer_key` здесь ПУСТОЙ и таким остаётся в репозитории.
-- Приватный ключ вписывается только через админку или прямым UPDATE на
-- сервере. Повторный прогон этого файла НЕ затирает уже введённый ключ —
-- см. COALESCE ниже.

INSERT INTO constants (cnt_code, cnt_descr, cnt_value_json, cnt_is_enable)
VALUES (
    'chain',
    'Расчёты в цепи: сеть Monad, адреса USDC и EscrowLock, ключ наблюдателя. '
    'mode=live включает работу с настоящей цепью, любое другое значение — mock.',
    jsonb_build_object(
        'mode',         'live',
        'rpc',          'https://testnet-rpc.monad.xyz',
        'chain_id',     10143,
        'usdc',         '0x534b2f3A21130d7a60830c2Df862319e593943A3',
        'lock',         '0x3CB2C5EA954C7711EfF621A784CD096E4E580be5',
        'observer_key', ''
    ),
    true
)
ON CONFLICT (cnt_code, cnt_is_enable) DO UPDATE
   SET cnt_descr = EXCLUDED.cnt_descr,
       -- обновляем адреса и сеть, но сохраняем уже введённый ключ
       cnt_value_json = EXCLUDED.cnt_value_json || jsonb_build_object(
           'observer_key',
           COALESCE(NULLIF(constants.cnt_value_json ->> 'observer_key', ''), '')
       );
