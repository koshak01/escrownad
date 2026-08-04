SELECT *
FROM deals
WHERE del_is_enable
  AND del_status IN (
    'listed', 'requested', 'accepted', 'preparing', 'prepared',
    'funded', 'awaiting_proof', 'released', 'refunded', 'dispute'
  )
ORDER BY del_id DESC
LIMIT 100
