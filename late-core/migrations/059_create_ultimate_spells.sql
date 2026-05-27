CREATE TABLE ultimate_cast_cooldowns (
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    ultimate_id TEXT NOT NULL CHECK (length(btrim(ultimate_id)) > 0),
    last_cast_at TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    PRIMARY KEY (user_id, ultimate_id)
);

CREATE INDEX ultimate_cast_cooldowns_user_idx
    ON ultimate_cast_cooldowns (user_id, last_cast_at DESC);

INSERT INTO marketplace_items
    (sku, item_kind, slot, name, description, price_chips, payload, active, sort_order)
VALUES
    (
        'ultimate_wonderland',
        'ultimate_spell',
        NULL,
        'Wonderland',
        'Cast a server-wide psychedelic theme. Use /ultimate in chat to cast this spell (24h cooldown).',
        10000000,
        '{"ultimate":"wonderland","duration_ms":10000}'::jsonb,
        true,
        3000
    ),
    (
        'ultimate_thematrix',
        'ultimate_spell',
        NULL,
        'The Matrix',
        '"Follow the White Rabbit." Use /ultimate in chat to cast this spell (24h cooldown).',
        10000000,
        '{"ultimate":"thematrix","duration_ms":13000}'::jsonb,
        true,
        3010
    )
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
