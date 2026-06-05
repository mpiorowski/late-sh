CREATE TABLE shop_consumable_effects (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    updated TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    room_id UUID REFERENCES chat_rooms(id) ON DELETE CASCADE,
    effect_kind TEXT NOT NULL CHECK (length(btrim(effect_kind)) > 0),
    source_sku TEXT NOT NULL REFERENCES marketplace_items(sku) ON DELETE CASCADE,
    payload JSONB NOT NULL DEFAULT '{}'::jsonb,
    starts_at TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    ends_at TIMESTAMPTZ NOT NULL,
    active BOOLEAN NOT NULL DEFAULT true
);

CREATE INDEX shop_consumable_effects_active_room_idx
    ON shop_consumable_effects (effect_kind, room_id, ends_at DESC)
    WHERE active = true;

CREATE INDEX shop_consumable_effects_active_user_idx
    ON shop_consumable_effects (user_id, effect_kind, ends_at DESC)
    WHERE active = true;

CREATE TABLE user_aquarium_care (
    user_id UUID PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    updated TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    last_fed TIMESTAMPTZ NOT NULL DEFAULT current_timestamp
);

WITH consumable_seed(
    sku,
    item_kind,
    name,
    description,
    price_chips,
    payload,
    sort_order
) AS (
    VALUES
        (
            'chat_bot_username_color_day',
            'chat_consumable',
            'Bot Username Color',
            'Give bot, graybeard, and dealer a highlighted username color for one day.',
            1000,
            '{"category":"chat","effect_kind":"bot_username_color","duration_secs":86400,"daily_limit":true}'::jsonb,
            4000
        ),
        (
            'chat_room_spark',
            'chat_consumable',
            'Room Spark',
            'Trigger a short room-wide spark effect once per day.',
            2000,
            '{"category":"chat","effect_kind":"room_spark","target":"room","duration_secs":10,"daily_limit":true}'::jsonb,
            4010
        ),
        (
            'chat_room_highlight',
            'chat_consumable',
            'Room Highlight',
            'Lift the current room above favorites for one hour.',
            2500,
            '{"category":"chat","effect_kind":"room_highlight","target":"room","duration_secs":3600,"daily_limit":true}'::jsonb,
            4020
        ),
        (
            'chat_room_glow',
            'chat_consumable',
            'Room Glow',
            'Add a temporary glow state to the current room.',
            1000,
            '{"category":"chat","effect_kind":"room_glow","target":"room","duration_secs":900,"daily_limit":true}'::jsonb,
            4030
        ),
        (
            'chat_pinned_vibe',
            'chat_consumable',
            'Pinned Vibe',
            'Set a curated room vibe marker for a short session.',
            1500,
            '{"category":"chat","effect_kind":"pinned_vibe","target":"room","duration_secs":3600,"daily_limit":true,"vibe":"hacking"}'::jsonb,
            4040
        ),
        (
            'chat_message_accent',
            'chat_consumable',
            'Message Accent',
            'Accent your next chat message.',
            500,
            '{"category":"chat","effect_kind":"message_accent","duration_secs":86400}'::jsonb,
            4050
        ),
        (
            'chat_room_bump',
            'chat_consumable',
            'Room Bump',
            'Bump the current room in the room list for a short time.',
            1000,
            '{"category":"chat","effect_kind":"room_bump","target":"room","duration_secs":300}'::jsonb,
            4070
        ),
        (
            'pet_food',
            'companion_consumable',
            'Cat/Dog Food',
            'Buy one treat for your cat or dog. Open the pet modal with c, then press t to use it.',
            150,
            '{"category":"companion","effect_kind":"pet_food"}'::jsonb,
            11
        ),
        (
            'aquarium_food',
            'companion_consumable',
            'Aquarium Food',
            'Buy one aquarium food pinch. Open the Aquarium tray with Ctrl+Q, then press Ctrl+F to feed.',
            100,
            '{"category":"companion","effect_kind":"aquarium_food"}'::jsonb,
            21
        )
)
INSERT INTO marketplace_items
    (sku, item_kind, slot, name, description, price_chips, payload, active, sort_order)
SELECT
    sku,
    item_kind,
    NULL,
    name,
    description,
    price_chips,
    payload,
    true,
    sort_order
FROM consumable_seed
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

UPDATE marketplace_items
SET sort_order = CASE sku
        WHEN 'dynamic_bonsai' THEN 5
        WHEN 'pet_companion' THEN 10
        WHEN 'pet_food' THEN 11
        WHEN 'aquarium' THEN 20
        WHEN 'aquarium_food' THEN 21
        ELSE sort_order
    END,
    updated = current_timestamp
WHERE sku IN (
    'dynamic_bonsai',
    'pet_companion',
    'pet_food',
    'aquarium',
    'aquarium_food'
);
