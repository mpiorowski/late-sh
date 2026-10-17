-- Hangover Cure: a one-shot chat consumable sold in the Shop that immediately
-- purges all drunk points and drunk effects on the user, restoring sober typing
-- and clearing drunk labels and passed-out postures.
--
-- Listed in the Chat tab under Consumables (sort_order 4015, between Room Bump
-- at 4005 and Room Spark at 4020).

INSERT INTO marketplace_items
    (sku, item_kind, slot, name, description, price_chips, payload, active, sort_order)
VALUES
    (
        'hangover_cure',
        'chat_consumable',
        NULL,
        'Hangover Cure',
        'Immediately purge all alcohol and drunk effects, returning your typing and speech to normal.',
        500,
        '{"category":"chat","effect_kind":"hangover_cure","target":"user"}'::jsonb,
        true,
        4015
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
