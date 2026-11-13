-- Hedgehog joins the Basic badges, with the same day/month rental tiers.
INSERT INTO marketplace_items
    (sku, item_kind, slot, name, description, price_chips, payload, active, sort_order)
VALUES
    ('badge_hedgehog_day', 'badge_rental', NULL, '🦔',
     'Display 🦔 beside your chat name for 24 hours.',
     100, '{"emoji": "🦔", "slot": "chat_badge", "tier": "basic", "duration_secs": 86400}', true, 14020),
    ('badge_hedgehog_month', 'badge_rental', NULL, '🦔',
     'Display 🦔 beside your chat name for 30 days.',
     4000, '{"emoji": "🦔", "slot": "chat_badge", "tier": "basic", "duration_secs": 2592000}', true, 14025)
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
