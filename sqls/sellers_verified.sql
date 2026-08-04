SELECT DISTINCT seller_usr_id
FROM deals
WHERE del_is_enable
  AND del_status = 'released'
  AND seller_usr_id IS NOT NULL
