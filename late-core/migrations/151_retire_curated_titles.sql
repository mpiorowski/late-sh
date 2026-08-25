-- The Shop sells one title: the one you write. The 36 curated titles from 149
-- (72 rows, a day and a month tier each) are retired here, never deleted:
-- `user_purchases` and `shop_consumable_effects.source_sku` keep pointing at
-- them, and a live curated title runs out on its own clock.
--
-- A curated row is any title rental without the `custom` payload flag; the
-- custom pair from 150 keeps that flag and stays on sale under a warmer name.
-- Prices are unchanged (SHOP.md "Fixed numbers": 2,000 / 60,000).

UPDATE marketplace_items
SET active = false,
    updated = current_timestamp
WHERE item_kind = 'title_rental'
  AND COALESCE((payload->>'custom')::boolean, false) = false;

INSERT INTO marketplace_items
    (sku, item_kind, slot, name, description, price_chips, payload, active, sort_order)
VALUES
    (
        'title_custom_day',
        'title_rental',
        NULL,
        'Your Own Title',
        'A title in your own words, up to 20 characters, worn after your name in every message for 24 hours. Screened before you are charged.',
        2000,
        jsonb_build_object('custom', true, 'duration_secs', 86400),
        true,
        4190
    ),
    (
        'title_custom_month',
        'title_rental',
        NULL,
        'Your Own Title',
        'A title in your own words, up to 20 characters, worn after your name in every message for 30 days. Screened before you are charged.',
        60000,
        jsonb_build_object('custom', true, 'duration_secs', 2592000),
        true,
        4195
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
