SELECT *
FROM deals
WHERE del_is_enable
  AND del_status IN ('funded', 'awaiting_proof')
ORDER BY del_id DESC
LIMIT 100
