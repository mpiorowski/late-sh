-- The month tier is a convenience premium, not a bulk discount. The day tier
-- is the habit: flair runs out, the name goes plain, the player logs in and
-- rebuys. Thirty days without that chore is the thing being sold, so the
-- month price moves from 30x to 40x the day price (SHOP.md "Fixed numbers").
--
-- Every month SKU is `<stem>_month` with a `<stem>_day` twin, across the
-- three rental kinds (112/146 username effects, 148 badges and flags, 150 the
-- custom title). Priced from the twin here so a later day-price change can
-- re-run the same shape. Retired rows are left alone: their price is history.

UPDATE marketplace_items AS m
SET price_chips = d.price_chips * 40,
    updated = current_timestamp
FROM marketplace_items AS d
WHERE m.sku LIKE '%\_month'
  AND d.sku = left(m.sku, -6) || '_day'
  AND m.item_kind IN ('username_effect', 'badge_rental', 'title_rental')
  AND m.item_kind = d.item_kind
  AND m.active = true;
