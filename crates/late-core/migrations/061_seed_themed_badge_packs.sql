-- Three themed badge packs that slot into the existing chat-badge shop seeded
-- in 056_seed_badge_shop.sql. Same shape (item_kind=badge, slot=chat_badge,
-- payload={emoji, tier}, basic=1000 chips, premium=5000 chips). All SKUs are
-- prefixed `badge_` and use ON CONFLICT DO UPDATE so this migration is safe
-- to re-run.
--
-- Sort orders pick up where 056 left off (basics ran 1000-1068, premiums
-- 2000-2014). New packs start at 1100 (music), 1200 (plants), 1300 (weather),
-- with premium tier rows at 2100/2200/2300.
WITH badge_seed(sku, tier, emoji, price_chips, sort_order) AS (
    VALUES
        -- Music pack
        ('piano',        'basic',   '🎹',  1000, 1100),
        ('guitar',       'basic',   '🎸',  1000, 1101),
        ('drum',         'basic',   '🥁',  1000, 1102),
        ('saxophone',    'basic',   '🎷',  1000, 1103),
        ('trumpet',      'basic',   '🎺',  1000, 1104),
        ('violin',       'basic',   '🎻',  1000, 1105),
        ('microphone',   'basic',   '🎤',  1000, 1106),
        ('radio',        'basic',   '📻',  1000, 1107),
        ('disc',         'premium', '💿',  5000, 2100),

        -- Plants pack
        ('herb',         'basic',   '🌿',  1000, 1200),
        ('cactus',       'basic',   '🌵',  1000, 1201),
        ('potted_plant', 'basic',   '🪴',  1000, 1202),
        ('seedling',     'basic',   '🌱',  1000, 1203),
        ('fallen_leaf',  'basic',   '🍂',  1000, 1204),
        ('maple_leaf',   'basic',   '🍁',  1000, 1205),
        ('tulip',        'basic',   '🌷',  1000, 1206),
        ('rose',         'basic',   '🌹',  1000, 1207),
        ('blossom',      'premium', '🌺',  5000, 2200),

        -- Weather pack
        ('umbrella',     'basic',   '☔',  1000, 1300),
        ('snow',         'basic',   '🌨',  1000, 1301),
        ('fog',          'basic',   '🌫',  1000, 1302),
        ('cloud_rain',   'basic',   '🌧',  1000, 1303),
        ('sun_cloud',    'basic',   '🌤',  1000, 1304),
        ('thunder',      'basic',   '⛈',  1000, 1305),
        ('comet',        'premium', '☄️',  5000, 2300),
        ('tornado',      'premium', '🌪',  5000, 2301)
)
INSERT INTO marketplace_items
    (sku, item_kind, slot, name, description, price_chips, payload, active, sort_order)
SELECT
    'badge_' || sku,
    'badge',
    'chat_badge',
    emoji,
    'Display ' || emoji || ' beside your chat name.',
    price_chips,
    jsonb_build_object('emoji', emoji, 'tier', tier),
    true,
    sort_order
FROM badge_seed
ON CONFLICT (sku) DO UPDATE SET
    item_kind = EXCLUDED.item_kind,
    slot = EXCLUDED.slot,
    name = EXCLUDED.name,
    description = EXCLUDED.description,
    price_chips = EXCLUDED.price_chips,
    payload = EXCLUDED.payload,
    active = EXCLUDED.active,
    sort_order = EXCLUDED.sort_order,
    updated = current_timestamp;
