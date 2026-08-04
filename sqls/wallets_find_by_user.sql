SELECT u2w_address
FROM users2wallets
WHERE usr_id = $1
ORDER BY u2w_id DESC
LIMIT 1
