-- Badges and flags become rentals: a 24h and a 30-day tier per legacy badge,
-- the same shape the username effects already sell (migrations 112 and 146).
--
-- Every rental is derived from the legacy `item_kind = 'badge'` row it
-- replaces, so this stays correct as new badges land: seed the badge first,
-- then re-run this file's INSERT shape for it.
--
-- Two things differ from the legacy rows on purpose:
--   * `slot` is NULL. A rental must not go through the `equipped_slot` path
--     (that is the permanent equip), so the slot it fills lives in the payload
--     and reaches the chat label query through a `shop_consumable_effects` row.
--   * `sort_order` is the legacy order x10, with the month tier at +5, so each
--     month item lists directly under its day twin and the shop's Badges and
--     Flags tabs keep the order players already know.
--
-- Prices (SHOP.md "Fixed numbers"): basic badge 100 / 3,000, premium badge
-- 250 / 7,500, flags at the basic badge price.
--
-- The legacy permanent SKUs are retired at the bottom (`active = false`, never
-- deleted): `user_purchases` and `shop_consumable_effects.source_sku` keep the
-- history, and the chat label query still renders a permanent badge its owner
-- bought before rentals whenever no rental is live over it.

WITH legacy_badge AS (
    SELECT
        sku,
        slot,
        payload->>'emoji' AS emoji,
        COALESCE(payload->>'tier', 'basic') AS tier,
        sort_order
    FROM marketplace_items
    WHERE item_kind = 'badge'
      AND slot IN ('chat_badge', 'chat_flag')
      AND COALESCE(payload->>'emoji', '') <> ''
),
rental_tier(suffix, duration_secs, label, price_multiplier, sort_offset) AS (
    VALUES
        ('_day', 86400, '24 hours', 1, 0),
        ('_month', 2592000, '30 days', 30, 5)
),
rental_seed AS (
    SELECT
        b.sku || t.suffix AS sku,
        b.emoji,
        b.slot,
        b.tier,
        t.duration_secs,
        t.label,
        CASE
            WHEN b.slot = 'chat_badge' AND b.tier = 'premium' THEN 250
            ELSE 100
        END * t.price_multiplier AS price_chips,
        b.sort_order * 10 + t.sort_offset AS sort_order
    FROM legacy_badge b
    CROSS JOIN rental_tier t
)
INSERT INTO marketplace_items
    (sku, item_kind, slot, name, description, price_chips, payload, active, sort_order)
SELECT
    sku,
    'badge_rental',
    NULL,
    emoji,
    'Display ' || emoji || ' beside your chat name for ' || label || '.',
    price_chips,
    jsonb_build_object(
        'emoji', emoji,
        'slot', slot,
        'tier', tier,
        'duration_secs', duration_secs
    ),
    true,
    sort_order
FROM rental_seed
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

UPDATE marketplace_items
SET active = false,
    updated = current_timestamp
WHERE item_kind = 'badge'
  AND active = true;
