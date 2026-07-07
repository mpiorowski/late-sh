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

WITH fish_seed(sku, name, size_tier, width, height, area, price_chips, sort_order) AS (
    VALUES
        ('mj', 'MJ', 'small', 3, 1, 3, 1000, 3000),
        ('seahorse', 'Seahorse', 'small', 5, 2, 10, 1000, 3001),
        ('finnegan', 'Finnegan', 'small', 8, 2, 16, 1000, 3002),
        ('bee', 'Bee', 'small', 6, 3, 18, 1000, 3003),
        ('boxfish', 'Boxfish', 'small', 6, 3, 18, 1000, 3004),
        ('tiger', 'Tiger', 'small', 6, 3, 18, 1000, 3005),
        ('diamondfish', 'Diamondfish', 'small', 7, 3, 21, 1000, 3006),
        ('bumble', 'Bumble', 'small', 8, 3, 24, 1000, 3007),
        ('wingfish', 'Wingfish', 'small', 8, 3, 24, 1000, 3008),
        ('floata', 'Floata', 'medium', 9, 3, 27, 2500, 3100),
        ('squeeb', 'Squeeb', 'medium', 7, 4, 28, 2500, 3101),
        ('wigglewort', 'Wigglewort', 'medium', 5, 6, 30, 2500, 3102),
        ('rugbert', 'Rugbert', 'medium', 11, 3, 33, 2500, 3103),
        ('squigs', 'Squigs', 'medium', 9, 4, 36, 2500, 3104),
        ('jellybean', 'Jellybean', 'large', 13, 4, 52, 5000, 3200),
        ('oldskool', 'Oldskool', 'large', 15, 4, 60, 5000, 3201),
        ('bertrand', 'Bertrand', 'large', 17, 4, 68, 5000, 3202),
        ('bigbert', 'Bigbert', 'large', 29, 9, 261, 10000, 3203)
)
INSERT INTO marketplace_items
    (sku, item_kind, slot, name, description, price_chips, payload, active, sort_order)
SELECT
    'aquarium_fish_' || sku,
    'aquarium_fish',
    NULL,
    name,
    'Add one ' || size_tier || ' ' || name || ' to your aquarium.',
    price_chips,
    jsonb_build_object(
        'creature', sku,
        'size', size_tier,
        'width', width,
        'height', height,
        'area', area
    ),
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
