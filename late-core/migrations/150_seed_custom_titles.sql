-- Custom title rentals: the buyer writes their own text instead of picking one
-- off the curated list in 149. Same `title_rental` kind, same effect row, same
-- slot; only the payload differs. `custom` marks the SKU as buyer-supplied and
-- there is no `text` key, because the text does not exist until someone types
-- it.
--
-- Prices from SHOP.md "Fixed numbers": custom titles 2,000 / 60,000, ten times
-- the curated tier. The text is capped at 20 characters (`TITLE_MAX_LEN`) and
-- screened by the AI service before the purchase transaction opens, so a
-- refused title is never charged for.
--
-- Sorted just above the curated block (4200+) so the write-your-own tier leads
-- the titles, month directly under day as in 146.

INSERT INTO marketplace_items
    (sku, item_kind, slot, name, description, price_chips, payload, active, sort_order)
VALUES
    (
        'title_custom_day',
        'title_rental',
        NULL,
        'Custom Title',
        'Write your own title, up to 20 characters, and wear it after your name in chat for 24 hours. Screened before you are charged.',
        2000,
        jsonb_build_object('custom', true, 'duration_secs', 86400),
        true,
        4190
    ),
    (
        'title_custom_month',
        'title_rental',
        NULL,
        'Custom Title',
        'Write your own title, up to 20 characters, and wear it after your name in chat for 30 days. Screened before you are charged.',
        60000,
        jsonb_build_object('custom', true, 'duration_secs', 2592000),
        true,
        4195
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
