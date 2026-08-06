-- escrownad/seeds/deals_demo.sql
-- Demo showcase derived from REAL resource transfers published in the RIPE
-- transfer tables (ipv4 PI+PA, ipv6, asn), snapshot of 2026-08-06.
--
--     psql -h 127.0.0.1 -U html -d 'escrownad.com' -f seeds/deals_demo.sql
--
-- PRIVACY NOTE — THE PARTIES BELOW ARE ANONYMISED ON PURPOSE.
-- The public registry names natural persons and identifiable companies as
-- transfer counterparties. The deals in this file are fabricated demo
-- fixtures: advertising invented escrow deals under the name of a real
-- person or a real company is not acceptable in a public deployment.
-- Therefore every `del_from_org`, `del_to_org` and `del_ripe_match_key`
-- value here is a GENERIC PLACEHOLDER ('LIR Holder A', 'Regional ISP',
-- 'European Telecom Operator', ...) and does NOT identify the registered
-- holder of the resource. No mapping back to the real names is stored.
--
-- Networks, ASNs, country codes, dates and sizes are left untouched: they
-- are public registry facts and not personal data.
--
-- WARNING: `del_ripe_match_key` no longer reproduces the registry row it was
-- taken from, so these demo rows are NOT independently verifiable. That is
-- intentional — a verifiable key would carry the names back in.
--
-- NOTE: this file expects the CANONICAL column names (del_*), i.e. it must
-- be applied AFTER seeds/deals_rename_canon.sql.

BEGIN;

DELETE FROM deals WHERE del_title LIKE '%[demo]%';


INSERT INTO deals (
    del_hash, del_title, del_asset_type, del_resource_kind, del_prefix,
    del_listing_side, del_from_org, del_to_org, del_rir, del_geo,
    del_seller_wallet, del_amount, del_chain_id, del_status,
    del_deadline_ts, del_ripe_match_key, del_release_tx,
    del_soft_verified, del_is_enable, del_dat
) VALUES (
    md5('demo-0-' || clock_timestamp()::text),
    '<p>[demo] IPv4 /23 — 512 addresses, transferred via RIPE policy transfer</p>',
    'ip', 'PI', '195.88.34.0/23',
    'offer', 'European Hosting Provider', 'LIR Holder A',
    'RIPE', 'AT',
    '0xfdbb5fee69f23c378689f99728e28f326edc1a88',
    1792000000000, 'monad', 'released',
    NOW() + INTERVAL '31 days', 'PI|05/08/2026|195.88.34.0/23|195.88.34.0/23|European Hosting Provider|LIR Holder A', '0xb5c11364b40e5c92d6bcf0f463d92d7bfd709a3c4729231e991b1434f9ede30e',
    true, true, NOW() - INTERVAL '3 days'
);

INSERT INTO deals (
    del_hash, del_title, del_asset_type, del_resource_kind, del_prefix,
    del_listing_side, del_from_org, del_to_org, del_rir, del_geo,
    del_seller_wallet, del_amount, del_chain_id, del_status,
    del_deadline_ts, del_ripe_match_key, del_release_tx,
    del_soft_verified, del_is_enable, del_dat
) VALUES (
    md5('demo-1-' || clock_timestamp()::text),
    '<p>[demo] IPv4 /23 — 512 addresses, transferred via RIPE policy transfer</p>',
    'ip', 'PI', '194.246.124.0/23',
    'offer', 'Regional Contact Center Operator', 'Regional Business Services Group',
    'RIPE', 'PL',
    '0xfdbb5fee69f23c378689f99728e28f326edc1a88',
    1792000000000, 'monad', 'released',
    NOW() + INTERVAL '31 days', 'PI|05/08/2026|194.246.124.0/23|194.246.124.0/23|Regional Contact Center Operator|Regional Business Services Group', '0x3f0b9cb333c2556538256e28b80981341713a0f2e088caf130a03db734f5a14c',
    true, true, NOW() - INTERVAL '3 days'
);

INSERT INTO deals (
    del_hash, del_title, del_asset_type, del_resource_kind, del_prefix,
    del_listing_side, del_from_org, del_to_org, del_rir, del_geo,
    del_seller_wallet, del_amount, del_chain_id, del_status,
    del_deadline_ts, del_ripe_match_key, del_release_tx,
    del_soft_verified, del_is_enable, del_dat
) VALUES (
    md5('demo-2-' || clock_timestamp()::text),
    '<p>[demo] IPv4 /21 — 2048 addresses, transferred via RIPE policy transfer</p>',
    'ip', 'PI', '138.16.144.0/21',
    'offer', 'LIR Holder B', 'Satellite Network Operator',
    'RIPE', 'RU',
    '0xfdbb5fee69f23c378689f99728e28f326edc1a88',
    7168000000000, 'monad', 'released',
    NOW() + INTERVAL '31 days', 'PI|05/08/2026|138.16.144.0/21|138.16.144.0/21|LIR Holder B|Satellite Network Operator', '0x8ae7d356bded07a22103a47081a80664c2a51f0afbf87d4444e9092780bac28c',
    true, true, NOW() - INTERVAL '3 days'
);

INSERT INTO deals (
    del_hash, del_title, del_asset_type, del_resource_kind, del_prefix,
    del_listing_side, del_from_org, del_to_org, del_rir, del_geo,
    del_seller_wallet, del_amount, del_chain_id, del_status,
    del_deadline_ts, del_ripe_match_key, del_release_tx,
    del_soft_verified, del_is_enable, del_dat
) VALUES (
    md5('demo-3-' || clock_timestamp()::text),
    '<p>[demo] IPv4 /24 — 256 addresses, transferred via RIPE policy transfer</p>',
    'ip', 'PI', '91.239.23.0/24',
    'offer', 'LIR Holder C', 'Regional Data Center Operator',
    'RIPE', 'RU',
    '0xfdbb5fee69f23c378689f99728e28f326edc1a88',
    896000000000, 'monad', 'released',
    NOW() + INTERVAL '31 days', 'PI|05/08/2026|91.239.23.0/24|91.239.23.0/24|LIR Holder C|Regional Data Center Operator', '0x7c5f17183c0430926ea9039bdb77ab3c867d32a109b0f485d9aadd326fa3f237',
    true, true, NOW() - INTERVAL '3 days'
);

INSERT INTO deals (
    del_hash, del_title, del_asset_type, del_resource_kind, del_prefix,
    del_listing_side, del_from_org, del_to_org, del_rir, del_geo,
    del_seller_wallet, del_amount, del_chain_id, del_status,
    del_deadline_ts, del_ripe_match_key, del_release_tx,
    del_soft_verified, del_is_enable, del_dat
) VALUES (
    md5('demo-4-' || clock_timestamp()::text),
    '<p>[demo] IPv4 /20 — 4096 addresses, transferred via RIPE policy transfer</p>',
    'ip', 'PI', '89.222.48.0/20',
    'offer', 'Regional ISP', 'National Broadband Provider',
    'RIPE', 'GB',
    '0xfdbb5fee69f23c378689f99728e28f326edc1a88',
    14336000000000, 'monad', 'released',
    NOW() + INTERVAL '31 days', 'PI|05/08/2026|89.222.32.0/19|89.222.48.0/20|Regional ISP|National Broadband Provider', '0xfa927457bb9fb879c5ebec419942cb16434ab5689c8100ad17f30e493d2fbaf4',
    true, true, NOW() - INTERVAL '3 days'
);

INSERT INTO deals (
    del_hash, del_title, del_asset_type, del_resource_kind, del_prefix,
    del_listing_side, del_from_org, del_to_org, del_rir, del_geo,
    del_seller_wallet, del_amount, del_chain_id, del_status,
    del_deadline_ts, del_ripe_match_key, del_release_tx,
    del_soft_verified, del_is_enable, del_dat
) VALUES (
    md5('demo-5-' || clock_timestamp()::text),
    '<p>[demo] IPv6 block 2a03:c500::/32 — allocation transfer between LIRs</p>',
    'ipv6', 'PA', '2a03:c500::/32',
    'offer', 'Regional Education Network Trust', 'Education Technology Provider',
    'RIPE', 'GB',
    '0xfdbb5fee69f23c378689f99728e28f326edc1a88',
    4800000000000, 'monad', 'released',
    NOW() + INTERVAL '31 days', 'PA6|04/08/2026|2a03:c500::/32|Regional Education Network Trust|Education Technology Provider', '0x1ecff7a9948c52752018ebb1a0488d41624d67e1a0f6549444911fedaeac5fc2',
    true, true, NOW() - INTERVAL '4 days'
);

INSERT INTO deals (
    del_hash, del_title, del_asset_type, del_resource_kind, del_prefix,
    del_listing_side, del_from_org, del_to_org, del_rir, del_geo,
    del_seller_wallet, del_amount, del_chain_id, del_status,
    del_deadline_ts, del_ripe_match_key, del_release_tx,
    del_soft_verified, del_is_enable, del_dat
) VALUES (
    md5('demo-6-' || clock_timestamp()::text),
    '<p>[demo] IPv6 block 2a04:f140::/29 — allocation transfer between LIRs</p>',
    'ipv6', 'PA', '2a04:f140::/29',
    'offer', 'Offshore Telecom Holding', 'European Telecom Operator',
    'RIPE', 'CY',
    '0xfdbb5fee69f23c378689f99728e28f326edc1a88',
    4800000000000, 'monad', 'released',
    NOW() + INTERVAL '31 days', 'PA6|03/08/2026|2a04:f140::/29|Offshore Telecom Holding|European Telecom Operator', '0x07e78b195a35b240a1dca7d5d03eb6eefa562868e93a230af003fe4a593b90c2',
    true, true, NOW() - INTERVAL '4 days'
);

INSERT INTO deals (
    del_hash, del_title, del_asset_type, del_resource_kind, del_prefix,
    del_listing_side, del_from_org, del_to_org, del_rir, del_geo,
    del_seller_wallet, del_amount, del_chain_id, del_status,
    del_deadline_ts, del_ripe_match_key, del_release_tx,
    del_soft_verified, del_is_enable, del_dat
) VALUES (
    md5('demo-7-' || clock_timestamp()::text),
    '<p>[demo] Autonomous System AS31032 — routing identity transfer</p>',
    'asn', '', 'AS31032',
    'offer', 'Regional Contact Center Operator', 'Regional Business Services Group',
    'RIPE', 'PL',
    '0xfdbb5fee69f23c378689f99728e28f326edc1a88',
    1200000000000, 'monad', 'released',
    NOW() + INTERVAL '31 days', 'ASN|05/08/2026|AS31032|Regional Contact Center Operator|Regional Business Services Group', '0x651993fc704ef7a641fa963c7e69b88fe2743e190c262615de6c481f37c5440e',
    true, true, NOW() - INTERVAL '2 days'
);

INSERT INTO deals (
    del_hash, del_title, del_asset_type, del_resource_kind, del_prefix,
    del_listing_side, del_from_org, del_to_org, del_rir, del_geo,
    del_seller_wallet, del_amount, del_chain_id, del_status,
    del_deadline_ts, del_ripe_match_key, del_release_tx,
    del_soft_verified, del_is_enable, del_dat
) VALUES (
    md5('demo-8-' || clock_timestamp()::text),
    '<p>[demo] Autonomous System AS215730 — routing identity transfer</p>',
    'asn', '', 'AS215730',
    'offer', 'Cloud Infrastructure Holding', 'Cloud Infrastructure Operator',
    'RIPE', 'GB',
    '0xfdbb5fee69f23c378689f99728e28f326edc1a88',
    1200000000000, 'monad', 'released',
    NOW() + INTERVAL '31 days', 'ASN|05/08/2026|AS215730|Cloud Infrastructure Holding|Cloud Infrastructure Operator', '0x17b1f7df608e5b428d212fe3e61acb8700a73aa8c37daa19d553f13101e6cde4',
    true, true, NOW() - INTERVAL '2 days'
);

INSERT INTO deals (
    del_hash, del_title, del_asset_type, del_resource_kind, del_prefix,
    del_listing_side, del_from_org, del_to_org, del_rir, del_geo,
    del_seller_wallet, del_amount, del_chain_id, del_status,
    del_deadline_ts, del_ripe_match_key, del_release_tx,
    del_soft_verified, del_is_enable, del_dat
) VALUES (
    md5('demo-9-' || clock_timestamp()::text),
    '<p>[demo] IPv4 /19 — 8192 clean addresses, direct from the holder</p>',
    'ip', 'PA', '87.58.0.0/19',
    'offer', 'European Cloud Provider', NULL,
    'RIPE', 'NL',
    '0xfdbb5fee69f23c378689f99728e28f326edc1a88',
    28672000000000, 'monad', 'listed',
    NOW() + INTERVAL '31 days', NULL, NULL,
    true, true, NOW() - INTERVAL '0 days'
);

COMMIT;
