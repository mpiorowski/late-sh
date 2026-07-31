-- Bonsai Decay Shield: a repeatable user-scoped consumable that keeps a
-- bonsai (classic or Dynamic) from decaying for two weeks. Follows the same
-- shop_consumable_effects flow as the username effects (migration 112):
-- purchase activates a user-scoped row (room_id NULL), and rebuying while one
-- is still live extends the existing expiry by another 14 days rather than
-- restarting the clock, so a player never loses paid-for time.

INSERT INTO marketplace_items
    (sku, item_kind, slot, name, description, price_chips, payload, active, sort_order)
VALUES
    (
        'bonsai_decay_shield_two_weeks',
        'bonsai_consumable',
        NULL,
        'Bonsai Decay Shield',
        'Protect your bonsai from decay for two weeks. Stacks with any remaining time on your last shield.',
        2000,
        '{"category":"bonsai","effect_kind":"bonsai_decay_protection","duration_secs":1209600}'::jsonb,
        true,
        16
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
