-- escrownad/seeds/moderation_template.sql
-- Шаблон карточки заявки для оператора + кнопки Approve / Decline.
--
--     psql -h 127.0.0.1 -U html -d 'escrownad.com' -f seeds/moderation_template.sql
--
-- Текст ТОЛЬКО английский: продукт англоязычный, оператор читает то же,
-- что видят пользователи и жюри.
--
-- parse_mode = RichMarkdown (Bot API 10.2): заголовки и таблицы.
-- callback_data кнопок: `dm:<16 символов хэша>:<a|d>` — Telegram ограничивает
-- это поле 64 байтами, полный хэш туда не влезает.

INSERT INTO templates (tpl_code, tpl_descr, tpl_template, tpl_reply_markup_template, tpl_param, tlg_id, tpl_hash, tpl_is_enable)
VALUES (
    'deal_moderation',
    'New listing awaiting approval — sent to the moderation topic',
    E'# New listing {{ deal_no }}\n'
    '\n'
    '{{ description }}\n'
    '\n'
    '| Field | Value |\n'
    '| --- | --- |\n'
    '| Asset | {{ asset_type }} {{ resource_kind }} · {{ rir }} |\n'
    '| Network | `{{ prefix }}` |\n'
    '| Addresses | {{ addresses }} |\n'
    '| Region | {{ geo }} |\n'
    '| Price | {{ price }} USDC |\n'
    '| Side | {{ side }} |\n'
    '\n'
    '## Registry says\n'
    '\n'
    '| Field | Value |\n'
    '| --- | --- |\n'
    '| Holder | {{ registry_holder }} |\n'
    '| Type | {{ registry_type }} |\n'
    '| Country | {{ registry_country }} |\n'
    '| Claimed by seller | {{ from_org }} |\n'
    '| Match | {% if registry_matches %}yes{% else %}NO — check manually{% endif %} |\n'
    '\n'
    '## Applicant wallet\n'
    '\n'
    '| Field | Value |\n'
    '| --- | --- |\n'
    '| Address | `{{ wallet }}` |\n'
    '| USDC | {{ wallet_usdc }} |\n'
    '| Gas (MON) | {{ wallet_native }} |\n'
    '| History | {% if wallet_is_new %}first listing — new wallet{% else %}{{ wallet_deals }} deals before{% endif %} |\n'
    '\n'
    '{% if terms %}**Terms:** {{ terms }}\n{% endif %}'
    '\n'
    '{{ url }}',
    E'{"inline_keyboard":[[{"text":"✅ Approve","callback_data":"dm:{{ deal_short }}:a"},{"text":"⛔ Decline","callback_data":"dm:{{ deal_short }}:d"}],[{"text":"🔎 Open listing","url":"{{ url }}"}]]}',
    '{"parse_mode":"RichMarkdown"}'::jsonb,
    (SELECT tlg_id FROM telegrams WHERE tlg_code = 'moderation'),
    md5(random()::text || clock_timestamp()::text),
    true
)
ON CONFLICT (tpl_code) DO UPDATE
   SET tpl_template = EXCLUDED.tpl_template,
       tpl_reply_markup_template = EXCLUDED.tpl_reply_markup_template,
       tpl_param = EXCLUDED.tpl_param,
       tpl_descr = EXCLUDED.tpl_descr,
       tlg_id = EXCLUDED.tlg_id,
       tpl_is_enable = true,
       tpl_updated = NOW();
