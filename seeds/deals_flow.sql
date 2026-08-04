-- Flow fields for deal lifecycle (cabinet / want / prepare / fund).
--     psql -h 127.0.0.1 -U html -d 'escrownad.com' -f seeds/deals_flow.sql

ALTER TABLE deals ADD COLUMN IF NOT EXISTS asset_type varchar(32) NOT NULL DEFAULT 'ip';
-- ip | domain | work  (v1 product path = ip)

ALTER TABLE deals ADD COLUMN IF NOT EXISTS soft_verified boolean NOT NULL DEFAULT false;
-- IP soft pre-check (email from mailbox) before list

ALTER TABLE deals ADD COLUMN IF NOT EXISTS contact_email varchar(512);
-- mailbox used for soft verify

ALTER TABLE deals ADD COLUMN IF NOT EXISTS broker_usr_id bigint;
-- optional introducer (later fee share)

COMMENT ON COLUMN deals.del_status IS
  'draft|verified|listed|requested|accepted|preparing|prepared|funded|awaiting_proof|released|refunded|dispute|cancelled';

-- reset demo fixtures to happy-path starting points for UI
UPDATE deals SET del_status = 'listed', soft_verified = true,
    release_tx = NULL, ripe_match_key = NULL, lock_tx = NULL,
    buyer_usr_id = NULL, buyer_wallet = NULL
 WHERE prefix = '176.120.88.0/21';

UPDATE deals SET del_status = 'listed', soft_verified = true,
    release_tx = NULL, ripe_match_key = NULL, lock_tx = NULL
 WHERE prefix = '91.224.58.0/23';
