INSERT INTO marketplace_items
    (sku, item_kind, slot, name, description, price_chips, payload, active, sort_order)
VALUES
    (
        'dynamic_bonsai',
        'feature_unlock',
        'bonsai_variant',
        'Dynamic Bonsai',
        'Switch your bonsai care modal to the living branch graph.',
        1000,
        '{"feature":"dynamic_bonsai","variant":"dynamic"}'::jsonb,
        true,
        15
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
