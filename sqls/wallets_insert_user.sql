INSERT INTO users (usr_hash, usr_pass, usr_descr, usr_is_enable, usr_is_staff)
VALUES ($1, '', $2, true, false)
RETURNING usr_id, usr_hash, usr_is_staff, usr_is_enable
