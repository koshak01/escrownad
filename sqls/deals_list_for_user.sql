SELECT *
FROM deals
WHERE del_is_enable
  AND (seller_usr_id = $1 OR buyer_usr_id = $1)
ORDER BY del_id DESC
LIMIT 100
