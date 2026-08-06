-- escrownad/seeds/wallets_pubkey.sql
-- The wallet's public key — the basis for encrypting data for one person alone.
--
--     psql -h 127.0.0.1 -U html -d 'escrownad.com' -f seeds/wallets_pubkey.sql
--
-- Why: a wallet address is a one-way hash, so nothing can be encrypted "to an
-- address". A public key, however, can be recovered from any signature —
-- including the one a person already gives to sign in. We store it at sign-in
-- so that deal documents can later be handed to a buyer in a form only they can
-- read: not us, not anyone with access to the database.
--
-- Not retroactive: for accounts that signed in earlier the key appears on
-- their next sign-in. Hence the column is nullable.

ALTER TABLE users2wallets ADD COLUMN IF NOT EXISTS u2w_pubkey varchar(70);

COMMENT ON COLUMN users2wallets.u2w_pubkey IS
    'secp256k1 public key, compressed (0x02/0x03 || x), recovered from the sign-in signature';
