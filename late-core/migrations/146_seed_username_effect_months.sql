-- The month tier for the three username effects seeded by migration 112: the
-- same Name Glow / Name Gradient / Name Shimmer styles, running 30 days
-- instead of 24 hours, at 30x the day price. Only `duration_secs` and the
-- price differ, so the picker, the purchase path, and the flair pipeline all
-- treat a month item exactly like its day twin.
--
-- sort_order sits each month item directly under its day item (4100 -> 4105,
-- 4110 -> 4115, 4120 -> 4125), since the shop lists by sort_order.

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
            'username_glow_month',
            'username_effect',
            'Name Glow Monthly',
            'Paint your username in a bright color of your choice, in chat and the clubhouse, for 30 days.',
            6000,
            '{"category":"identity","effect_kind":"username_effect","variant":"glow","duration_secs":2592000}'::jsonb,
            4105
        ),
        (
            'username_gradient_month',
            'username_effect',
            'Name Gradient Monthly',
            'Fade your username between two colors of your choice, in chat and the clubhouse, for 30 days.',
            15000,
            '{"category":"identity","effect_kind":"username_effect","variant":"gradient","duration_secs":2592000}'::jsonb,
            4115
        ),
        (
            'username_shimmer_month',
            'username_effect',
            'Name Shimmer Monthly',
            'Give your username animated color cycling, in chat and the clubhouse, for 30 days.',
            30000,
            '{"category":"identity","effect_kind":"username_effect","variant":"shimmer","duration_secs":2592000}'::jsonb,
            4125
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
