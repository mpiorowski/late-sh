-- Three new small-tier aquarium creatures to flesh out the catalog seeded
-- in 059_create_aquarium_shop.sql. Same payload shape (creature, size,
-- width, height, area), same item_kind, same price tier (1000 chips).
-- KDL art for each ships in late-ssh/assets/aquarium/creatures/ and is
-- registered alphabetically in late-ssh/src/app/hub/aquarium/creature.rs.
--
-- Sort orders 3009-3011 pick up directly where the existing small block
-- (3000-3008) left off; medium starts at 3100, large at 3200, mega at
-- 3203 — none of those are affected.
WITH fish_seed(sku, name, size_tier, width, height, area, price_chips, sort_order) AS (
    VALUES
        ('anchovy',    'Anchovy',    'small', 6, 2, 12, 1000, 3009),
        ('clownfish',  'Clownfish',  'small', 7, 3, 21, 1000, 3010),
        ('pufferfish', 'Pufferfish', 'small', 6, 3, 18, 1000, 3011)
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
