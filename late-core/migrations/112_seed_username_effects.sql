-- 24h username effects: user-scoped fg styling on the buyer's own name in
-- chat author headers and the clubhouse label, visible to everyone. Buyer
-- picks the color (glow) or preset pair (gradient) at purchase; the choice
-- lands in the effect row payload. Rebuying replaces the live effect and
-- resets the 24h clock, so there is no daily_limit.
--
-- Migration 104 dropped the user-scoped partial index when the last
-- user-scoped effect retired; these items bring user-scoped rows back.

CREATE INDEX shop_consumable_effects_active_user_idx
    ON shop_consumable_effects (user_id, effect_kind, ends_at DESC)
    WHERE active = true;

WITH effect_seed(
    sku,
    item_kind,
    name,
    description,
    price_chips,
    payload,
    sort_order
) AS (
    VALUES
        (
            'username_glow_day',
            'username_effect',
            'Name Glow',
            'Paint your username in a bright color of your choice, in chat and the clubhouse, for 24 hours.',
            200,
            '{"category":"identity","effect_kind":"username_effect","variant":"glow","duration_secs":86400}'::jsonb,
            4100
        ),
        (
            'username_gradient_day',
            'username_effect',
            'Name Gradient',
            'Fade your username between two colors of your choice, in chat and the clubhouse, for 24 hours.',
            500,
            '{"category":"identity","effect_kind":"username_effect","variant":"gradient","duration_secs":86400}'::jsonb,
            4110
        ),
        (
            'username_shimmer_day',
            'username_effect',
            'Name Shimmer',
            'Give your username animated color cycling, in chat and the clubhouse, for 24 hours.',
            1000,
            '{"category":"identity","effect_kind":"username_effect","variant":"shimmer","duration_secs":86400}'::jsonb,
            4120
        )
)
INSERT INTO marketplace_items
    (sku, item_kind, slot, name, description, price_chips, payload, active, sort_order)
SELECT
    sku,
    item_kind,
    NULL,
    name,
    description,
    price_chips,
    payload,
    true,
    sort_order
FROM effect_seed
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
