-- offer = sell, request = buy demand (exchange board).
ALTER TABLE deals
  ADD COLUMN IF NOT EXISTS listing_side varchar(16) NOT NULL DEFAULT 'offer';

CREATE INDEX IF NOT EXISTS deals_listing_side_idx
  ON deals (listing_side)
  WHERE del_is_enable;
