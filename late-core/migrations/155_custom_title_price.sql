-- Your Own Title comes down from 2,000 to 1,000 a day (SHOP.md "Fixed
-- numbers"). At 2,000 the title alone ate a whole completionist arcade day
-- (~2,000 chips), so the fully dressed name (title plus the top username
-- effect) cost more than the best day pays. At 1,000 it prices like the top
-- effect: both together are one full day, a casual player wears a title every
-- other day, which is the daily-login habit the day tier exists to create.
--
-- The month tier follows the 40x rule from 153, re-run in that migration's
-- shape from the day twin, so the rule stays the one place the multiplier
-- lives: 40,000, the same as the top effect's month.

UPDATE marketplace_items
SET price_chips = 1000,
    updated = current_timestamp
WHERE sku = 'title_custom_day'
  AND item_kind = 'title_rental'
  AND active = true;

UPDATE marketplace_items AS m
SET price_chips = d.price_chips * 40,
    updated = current_timestamp
FROM marketplace_items AS d
WHERE m.sku = 'title_custom_month'
  AND d.sku = 'title_custom_day'
  AND m.item_kind = 'title_rental'
  AND d.item_kind = 'title_rental'
  AND m.active = true;
