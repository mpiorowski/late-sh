-- Burn milestones (SHOP.md phase 4): three permanent badges whose only
-- product is the receipt. They fill the price band above the rentals, and
-- the same file brings the shop's ceiling down from ten million to one.
--
-- `item_kind = 'milestone_badge'`, never 'badge': migration 148 ends with
-- `UPDATE marketplace_items SET active = false WHERE item_kind = 'badge'`
-- and its header invites re-running that shape for new badges, so a
-- milestone seeded as a badge would be retired by the next badge added.
--
-- `slot` is NULL. A milestone never goes through the `equipped_slot` path:
-- it is not a badge slot at all, it is a fourth glyph that renders on top of
-- a rented badge and a rented flag. The highest one a user owns is the one
-- that shows, so there is nothing to equip and nothing to choose.
--
-- Every emoji here is unique to this ladder: none of them appears in the
-- badge seeds (056, 061, 083, 141), the flag seeds (069), or on the crown.

INSERT INTO marketplace_items
    (sku, item_kind, slot, name, description, price_chips, payload, active, sort_order)
VALUES
    (
        'milestone_wick',
        'milestone_badge',
        NULL,
        'Wick',
        'Burn 50,000 chips for 🕯️ beside your chat name, permanently.',
        50000,
        '{"emoji":"🕯️"}'::jsonb,
        true,
        2900
    ),
    (
        'milestone_fuse',
        'milestone_badge',
        NULL,
        'Fuse',
        'Burn 150,000 chips for 🧨 beside your chat name, permanently.',
        150000,
        '{"emoji":"🧨"}'::jsonb,
        true,
        2910
    ),
    (
        'milestone_furnace',
        'milestone_badge',
        NULL,
        'Furnace',
        'Burn 500,000 chips for 🌋 beside your chat name, permanently.',
        500000,
        '{"emoji":"🌋"}'::jsonb,
        true,
        2920
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

-- The ultimate spells come down to one million. At ten million neither ever
-- sold, and a ceiling nobody reaches prices nothing: the milestones above
-- ladder up to half of the new one. Nothing else about the spells changes.
UPDATE marketplace_items
SET price_chips = 1000000,
    updated = current_timestamp
WHERE sku IN ('ultimate_wonderland', 'ultimate_thematrix');
