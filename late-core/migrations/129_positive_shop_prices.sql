-- Free shop items are not a thing: UserChips::apply rejects zero-amount
-- moves, so a zero-price item would fail at purchase time anyway. Tighten
-- the schema (and the admin editor, in the same change) so one cannot be
-- created. purchased_price_chips keeps its >= 0 check: it is a historical
-- record, not a gate.
ALTER TABLE marketplace_items
    DROP CONSTRAINT marketplace_items_price_chips_check;
ALTER TABLE marketplace_items
    ADD CONSTRAINT marketplace_items_price_chips_check CHECK (price_chips > 0);
