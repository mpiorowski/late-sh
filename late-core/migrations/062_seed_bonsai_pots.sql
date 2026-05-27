-- Bonsai pot shop. Slots into the marketplace alongside cat_companion,
-- aquarium, fish, and badges. New `item_kind=bonsai_pot`, `slot=bonsai_pot`.
-- Each row's payload carries a `skin_id` that the bonsai renderer maps to a
-- 7-character pot row replacing the default ` [===] `. The default pot
-- remains free and is the absence of any equipped row (no SKU needed).
--
-- Sort orders start at 4000 to leave room for future bonsai-themed items
-- (saucers, soil, accent stones) below the aquarium fish block (3000-3203).
INSERT INTO marketplace_items
    (sku, item_kind, slot, name, description, price_chips, payload, active, sort_order)
VALUES
    (
        'bonsai_pot_round',
        'bonsai_pot',
        'bonsai_pot',
        'Round Pot',
        'Swap the default rectangular pot for a rounded silhouette.',
        2000,
        '{"skin_id":"round"}'::jsonb,
        true,
        4000
    ),
    (
        'bonsai_pot_footed',
        'bonsai_pot',
        'bonsai_pot',
        'Footed Pot',
        'Tapered pot with angled feet — gives the tree a little lift.',
        2000,
        '{"skin_id":"footed"}'::jsonb,
        true,
        4001
    ),
    (
        'bonsai_pot_drum',
        'bonsai_pot',
        'bonsai_pot',
        'Drum Pot',
        'Tall straight-sided drum pot, popular for cascade styles.',
        3000,
        '{"skin_id":"drum"}'::jsonb,
        true,
        4002
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
