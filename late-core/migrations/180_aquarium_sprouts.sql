-- Sprouts: the tank's buds. Every fourteen days a sprout comes up on the
-- floor; for seven days the owner can cut it, and left alone it roots as a
-- wigglewort (one more owned, swimming when the tank is under the cap).
-- Neither clock cares about feeding: plants come up whether the fish eat
-- or not. Murky water is gone with this migration, so the shield's copy
-- stops promising clean water.

ALTER TABLE user_aquarium_care
    ADD COLUMN IF NOT EXISTS sprout_born DATE,
    ADD COLUMN IF NOT EXISTS next_sprout DATE NOT NULL DEFAULT (current_date + 14);

-- Every tank comes with its first sprout: a purchase plants one from now
-- on, and every tank that exists today gets one at deploy, whether or not
-- its owner has a care row yet (a tank never fed gets the same row a first
-- connect would have made, plus the sprout).
INSERT INTO user_aquarium_care (user_id, last_fed, sprout_born, next_sprout)
SELECT p.user_id, current_timestamp - interval '1 day', current_date, current_date + 14
FROM user_purchases p
JOIN marketplace_items i ON i.id = p.item_id
WHERE i.sku = 'aquarium'
ON CONFLICT (user_id) DO UPDATE
SET sprout_born = current_date,
    next_sprout = current_date + 14,
    updated = current_timestamp;

UPDATE marketplace_items
SET description = 'An auto feeder minds your tank for two weeks: no fish starves. Stacks with any remaining shield time. The daily chips are still yours to earn.',
    updated = current_timestamp
WHERE sku = 'aquarium_shield_two_weeks';
