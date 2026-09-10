-- The tank's care grows stakes. The care row remembers the feeding streak
-- (fourteen straight fed days hatch a fry), the starvation deaths already
-- taken since the last meal (one fish per fourteen unfed days, settled at
-- login), and the fry still swimming small. The Aquarium Shield is the
-- Bonsai Decay Shield's shape (migration 130): an auto feeder that minds
-- the tank for two weeks, so no fish starves and the water stays clean;
-- a rebuy while one is live extends it rather than restarting it.

ALTER TABLE user_aquarium_care
    ADD COLUMN IF NOT EXISTS streak INT NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS deaths_settled INT NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS fry_creature TEXT,
    ADD COLUMN IF NOT EXISTS fry_born DATE;

INSERT INTO marketplace_items
    (sku, item_kind, slot, name, description, price_chips, payload, active, sort_order)
VALUES
    (
        'aquarium_shield_two_weeks',
        'aquarium_consumable',
        NULL,
        'Aquarium Shield',
        'An auto feeder minds your tank for two weeks: no fish starves and the water stays clean. Stacks with any remaining shield time. The daily chips are still yours to earn.',
        2000,
        '{"category":"aquarium","effect_kind":"aquarium_shield","duration_secs":1209600}'::jsonb,
        true,
        2999
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
