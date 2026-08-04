-- Per-user settings: store wallet_address (merge jsonb).
INSERT INTO configs2users (usr_id, c2u_values)
VALUES ($1, jsonb_build_object('wallet_address', $2::text))
ON CONFLICT (usr_id) DO UPDATE
SET c2u_values = configs2users.c2u_values || EXCLUDED.c2u_values,
    c2u_updated = now()
