-- The Hangover Pill is off the shelf: a night at the bar wears off on its own
-- (late_core::models::drinks::decayed_points) and nothing sold in the Shop
-- shortens it any more. Retired like every other pulled SKU, `active = false`,
-- so past purchases keep their ledger history and the row never lists again.
UPDATE marketplace_items SET active = false WHERE sku = 'hangover_pill';
