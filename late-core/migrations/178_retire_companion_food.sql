-- Pet and aquarium care are free daily rituals now, paid like the bonsai
-- watering (+100 chips on the first feed of the UTC day). The two food
-- consumables leave the Shop: deactivated, not deleted, so purchase history
-- and any leftover stock rows keep their references.
UPDATE marketplace_items
SET active = false,
    updated = current_timestamp
WHERE sku IN ('pet_food', 'aquarium_food');
