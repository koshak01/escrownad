-- Assign default product role "user" (never admin).
INSERT INTO users2roles (usr_id, rol_id, u2r_is_enable)
SELECT $1, r.rol_id, true
FROM roles r
WHERE r.rol_code = 'user'
ON CONFLICT (usr_id, rol_id) DO NOTHING
