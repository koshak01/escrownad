INSERT INTO users2wallets (usr_id, u2w_address)
VALUES ($1, $2)
ON CONFLICT (u2w_address) DO UPDATE SET u2w_address = EXCLUDED.u2w_address
RETURNING usr_id
