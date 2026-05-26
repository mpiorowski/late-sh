ALTER TABLE user_purchases
    ADD COLUMN IF NOT EXISTS active_quantity INT NOT NULL DEFAULT 0;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'user_purchases_active_quantity_bounds'
    ) THEN
        ALTER TABLE user_purchases
            ADD CONSTRAINT user_purchases_active_quantity_bounds
            CHECK (active_quantity >= 0 AND active_quantity <= quantity);
    END IF;
END $$;

INSERT INTO marketplace_items
    (sku, item_kind, slot, name, description, price_chips, payload, active, sort_order)
VALUES
    (
        'aquarium',
        'feature_unlock',
        NULL,
        'Aquarium',
        'Unlock the ambient bottom aquarium and fish catalog.',
        10000,
        '{"feature":"aquarium"}'::jsonb,
        true,
        20
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

WITH fish_seed(sku, name, sort_order) AS (
    VALUES
        ('bee', 'Bee', 3000),
        ('bertrand', 'Bertrand', 3001),
        ('bigbert', 'Bigbert', 3002),
        ('boxfish', 'Boxfish', 3003),
        ('bumble', 'Bumble', 3004),
        ('diamondfish', 'Diamondfish', 3005),
        ('finnegan', 'Finnegan', 3006),
        ('floata', 'Floata', 3007),
        ('jellybean', 'Jellybean', 3008),
        ('mj', 'MJ', 3009),
        ('oldskool', 'Oldskool', 3010),
        ('rugbert', 'Rugbert', 3011),
        ('seahorse', 'Seahorse', 3012),
        ('squeeb', 'Squeeb', 3013),
        ('squigs', 'Squigs', 3014),
        ('tiger', 'Tiger', 3015),
        ('wigglewort', 'Wigglewort', 3016),
        ('wingfish', 'Wingfish', 3017)
)
INSERT INTO marketplace_items
    (sku, item_kind, slot, name, description, price_chips, payload, active, sort_order)
SELECT
    'aquarium_fish_' || sku,
    'aquarium_fish',
    NULL,
    name,
    'Add one ' || name || ' to your aquarium.',
    1000,
    jsonb_build_object('creature', sku),
    true,
    sort_order
FROM fish_seed
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
