-- The companion shop copy still described surfaces that no longer exist: a
-- sidebar pet with a care modal reached by `c`/`t`, and an aquarium tray on
-- Ctrl+Q/Ctrl+F. Both are composer commands now, and the pet treat action was
-- folded into /feed. Restate each item in terms of the commands that run it.

UPDATE marketplace_items
SET description = 'Unlock the pet strip above the Lounge chat, in cat or dog ASCII. Use /pet to show or hide it, /feed and /water to care for it, and /petname to name it.',
    updated = current_timestamp
WHERE sku = 'pet_companion';

UPDATE marketplace_items
SET description = 'Buy one meal for your cat or dog. Use /feed, or click the food bowl, to spend one once per day and send the pet strolling across your screen.',
    updated = current_timestamp
WHERE sku = 'pet_food';

UPDATE marketplace_items
SET description = 'Unlock the ambient bottom aquarium and its fish catalog. Use /aquarium, or /aq, to show or hide the tray.',
    updated = current_timestamp
WHERE sku = 'aquarium';

UPDATE marketplace_items
SET description = 'Buy one pinch of fish food. Open the tray with /aquarium, then use /aquarium feed to scatter a pinch.',
    updated = current_timestamp
WHERE sku = 'aquarium_food';
