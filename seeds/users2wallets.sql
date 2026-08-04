-- users2wallets — EVM wallet ↔ forge user (EscrowNad / Monad).
-- Run once on escrownad.com DB (Hyperion or local tunnel).

CREATE TABLE IF NOT EXISTS public.users2wallets (
    u2w_id      bigserial PRIMARY KEY,
    usr_id      bigint NOT NULL REFERENCES public.users(usr_id) ON UPDATE CASCADE ON DELETE CASCADE,
    u2w_address character varying(64) NOT NULL,
    u2w_dat     timestamp without time zone DEFAULT now() NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS u_users2wallets_address
    ON public.users2wallets USING btree (u2w_address);

CREATE INDEX IF NOT EXISTS idx_users2wallets_user
    ON public.users2wallets USING btree (usr_id);
