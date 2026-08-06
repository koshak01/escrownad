-- escrownad/seeds/chain.sql
-- Settlement layer settings (Monad + USDC) — the `chain` constant.
--
--     psql -h 127.0.0.1 -U html -d 'escrownad.com' -f seeds/chain.sql
--
-- Project convention: parameters and keys live in the `constants` table and
-- are edited through /admin/constants/. No environment variables, no files.
--
-- IMPORTANT: `observer_key` is EMPTY here and stays empty in the repository.
-- The private key is entered only through the admin area or by a direct UPDATE
-- on the server. Re-running this file does NOT overwrite a key already in
-- place — see the COALESCE below.

INSERT INTO constants (cnt_code, cnt_descr, cnt_value_json, cnt_is_enable)
VALUES (
    'chain',
    'On-chain settlement: the Monad network, the USDC and EscrowLock addresses, '
    'and the observer key. mode=live works against the real chain; anything else is mock.',
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
       -- update addresses and network, but keep a key already entered
       cnt_value_json = EXCLUDED.cnt_value_json || jsonb_build_object(
           'observer_key',
           COALESCE(NULLIF(constants.cnt_value_json ->> 'observer_key', ''), '')
       );
