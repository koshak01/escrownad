INSERT INTO users2wallets (usr_id, u2w_address, u2w_pubkey)
VALUES ($1, $2, NULLIF($3, ''))
ON CONFLICT (u2w_address) DO UPDATE
   SET u2w_pubkey = COALESCE(NULLIF(EXCLUDED.u2w_pubkey, ''), users2wallets.u2w_pubkey)
RETURNING usr_id
