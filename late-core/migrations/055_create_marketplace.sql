CREATE TABLE marketplace_items (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    updated TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    sku TEXT NOT NULL UNIQUE,
    item_kind TEXT NOT NULL,
    slot TEXT,
    name TEXT NOT NULL,
    description TEXT NOT NULL,
    price_chips BIGINT NOT NULL CHECK (price_chips >= 0),
    payload JSONB NOT NULL DEFAULT '{}'::jsonb,
    active BOOLEAN NOT NULL DEFAULT true,
    starts_at TIMESTAMPTZ,
    ends_at TIMESTAMPTZ,
    sort_order INT NOT NULL DEFAULT 0
);

CREATE INDEX marketplace_items_visible_idx
    ON marketplace_items (active, sort_order, created);

CREATE TABLE user_purchases (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    updated TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    item_id UUID NOT NULL REFERENCES marketplace_items(id) ON DELETE CASCADE,
    quantity INT NOT NULL DEFAULT 1 CHECK (quantity >= 0),
    remaining_uses INT CHECK (remaining_uses IS NULL OR remaining_uses >= 0),
    equipped_slot TEXT,
    equipped_at TIMESTAMPTZ,
    purchased_price_chips BIGINT NOT NULL CHECK (purchased_price_chips >= 0),
    UNIQUE (user_id, item_id)
);

CREATE INDEX user_purchases_user_idx
    ON user_purchases (user_id, created DESC);

CREATE UNIQUE INDEX user_purchases_equipped_slot_idx
    ON user_purchases (user_id, equipped_slot)
    WHERE equipped_slot IS NOT NULL;

INSERT INTO marketplace_items
    (sku, item_kind, slot, name, description, price_chips, payload, active, sort_order)
VALUES
    (
        'cat_companion',
        'feature_unlock',
        NULL,
        'Cat Companion',
        'Unlock the sidebar cat and care modal.',
        3000,
        '{"feature":"cat_companion"}'::jsonb,
        true,
        10
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
