-- Curated title rentals: a short text printed after the username in chat and
-- on the clubhouse floor, rented for 24h or 30 days. Its own slot, so it
-- stacks with a username color effect rather than replacing it.
--
-- Same shape as the badge rentals in 148: `slot` stays NULL (nothing is
-- equipped), the payload carries what the effect row needs, and the month tier
-- sits directly under its day twin by `sort_order`. Prices from SHOP.md
-- "Fixed numbers": curated titles 200 / 6,000.
--
-- The list is written in the Blade Runner / noir register of GAME.md's theme
-- section and every entry passes the screenshot test: legible from the screen,
-- no dev jargon. They are lowercase on purpose, because they render as an
-- aside on a name (`mira, the night clerk`), not as a second name. Nothing
-- here is longer than 20 characters, the title cap the renderers enforce.

WITH title_seed(slug, text, sort_index) AS (
    VALUES
        ('the_insufferable',    'the insufferable',    0),
        ('the_night_clerk',     'the night clerk',     1),
        ('signal_runner',       'signal runner',       2),
        ('static_merchant',     'static merchant',     3),
        ('dead_channel',        'dead channel',        4),
        ('the_fixer',           'the fixer',           5),
        ('nightside',           'nightside',           6),
        ('the_quiet_one',       'the quiet one',       7),
        ('off_the_record',      'off the record',      8),
        ('the_ghost_line',      'the ghost line',      9),
        ('no_fixed_address',    'no fixed address',    10),
        ('the_slow_blade',      'the slow blade',      11),
        ('unlicensed',          'unlicensed',          12),
        ('the_pale_hour',       'the pale hour',       13),
        ('third_shift',         'third shift',         14),
        ('the_rainmaker',       'the rainmaker',       15),
        ('still_broadcasting',  'still broadcasting',  16),
        ('the_burned_wire',     'the burned wire',     17),
        ('nobodys_problem',     'nobody''s problem',   18),
        ('the_low_signal',      'the low signal',      19),
        ('afterglow',           'afterglow',           20),
        ('the_long_way_down',   'the long way down',   21),
        ('out_of_frame',        'out of frame',        22),
        ('the_patient_sort',    'the patient sort',    23),
        ('cheap_and_certain',   'cheap and certain',   24),
        ('the_wrong_number',    'the wrong number',    25),
        ('last_call',           'last call',           26),
        ('the_tourist',         'the tourist',         27),
        ('unmetered',           'unmetered',           28),
        ('the_borrowed_name',   'the borrowed name',   29),
        ('smoke_and_static',    'smoke and static',    30),
        ('the_night_mayor',     'the night mayor',     31),
        ('louder_than_most',    'louder than most',    32),
        ('the_good_bad_idea',   'the good bad idea',   33),
        ('the_last_honest',     'the last honest',     34),
        ('neon_apologist',      'neon apologist',      35)
),
rental_tier(suffix, duration_secs, label, price_chips, sort_offset) AS (
    VALUES
        ('_day', 86400, '24 hours', 200, 0),
        ('_month', 2592000, '30 days', 6000, 5)
)
INSERT INTO marketplace_items
    (sku, item_kind, slot, name, description, price_chips, payload, active, sort_order)
SELECT
    'title_' || s.slug || t.suffix,
    'title_rental',
    NULL,
    s.text,
    'Wear "' || s.text || '" after your name in chat for ' || t.label || '.',
    t.price_chips,
    jsonb_build_object('text', s.text, 'duration_secs', t.duration_secs),
    true,
    4200 + s.sort_index * 10 + t.sort_offset
FROM title_seed s
CROSS JOIN rental_tier t
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
