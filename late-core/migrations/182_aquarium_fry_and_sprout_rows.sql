-- Two Shop rows in the Aquarium section for things the tank grows on its
-- own, so the tank is tended in one place. Both are aquarium_fish items
-- (same section, same art preview) that the purchase path refuses
-- (payload.welcome / payload.sprout; marketplace::is_listed_only).
--
-- The fry: the two-cell hatchling every tank comes with (late-ssh
-- fry.kdl) gets a catalog row of its own, so the Shop shows the sprite
-- that actually swims instead of counting it as an MJ. Every tank bought
-- from now on comes with one (marketplace::welcome_aquarium_fry_in_tx);
-- tanks bought before this migration keep the MJ they were given. Priced
-- like the small tier so it breeds and starves at a small fish's weight.
--
-- The sprout: the bud on the tank floor (late-ssh sprout.kdl). It has no
-- purchase rows ever; the row exists so the Shop can show the one on the
-- floor, its clock, and the cut key, which replaces the /aquarium cut
-- command. Its state comes from user_aquarium_care, not from purchases.
-- The price column refuses zero; the Shop never prints this one.
INSERT INTO marketplace_items
    (sku, item_kind, slot, name, description, price_chips, payload, active, sort_order)
VALUES
    (
        'aquarium_fish_fry',
        'aquarium_fish',
        NULL,
        'Fry',
        'The hatchling every Aquarium comes with. It stays small; streak fry grow into their parent''s species and count there. Not for sale.',
        1000,
        '{"creature":"fry","size":"small","width":2,"height":1,"area":2,"welcome":true}'::jsonb,
        true,
        2998
    ),
    (
        'aquarium_sprout',
        'aquarium_fish',
        NULL,
        'Sprout',
        'A bud on the tank floor. One comes up every 14 days; cut it within 7 or it roots as a Wigglewort and takes a place. Not for sale.',
        1000,
        '{"creature":"sprout","sprout":true}'::jsonb,
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
