-- Verified parties: completed a released deal as seller or buyer.
SELECT DISTINCT usr_id FROM (
  SELECT seller_usr_id AS usr_id FROM deals
  WHERE del_is_enable AND del_status = 'released' AND seller_usr_id IS NOT NULL
  UNION
  SELECT buyer_usr_id AS usr_id FROM deals
  WHERE del_is_enable AND del_status = 'released' AND buyer_usr_id IS NOT NULL
) t
