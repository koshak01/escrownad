-- Lots whose verified asset was requested but has not been minted yet.
--
-- Issuance is asynchronous: the request goes out when an operator approves the
-- listing, and the address appears once their side has reviewed and minted it.
-- Something has to close that gap, so the observer picks these up on its round
-- and asks where the request has got to.
SELECT *
FROM deals
WHERE del_is_enable
  AND del_asset_request IS NOT NULL
  AND del_asset_request <> ''
  AND (del_asset_token IS NULL OR del_asset_token = '')
ORDER BY del_id
LIMIT 50
