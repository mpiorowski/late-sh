-- Fish and plants part ways. Until now the tank had one kind of stock,
-- aquarium_fish, and the wigglewort (a plant) sat in it: a fry could hatch
-- from it and starvation could take it. From here the plants are their own
-- item kind, aquarium_plant: bought like fish, active-capped apart from
-- them (marketplace::AQUARIUM_MAX_PLANTS), and what a sprout roots as
-- (one of the catalog's plants, picked evenly). Plants never starve and
-- never parent a fry; fish never root. The Shop's Companions tab lists
-- them under section rows of their own, the plants before the fish.
-- Purchase rows key on the item id, so every wigglewort anyone owns or
-- rooted follows the row.
UPDATE marketplace_items
SET sku = 'aquarium_plant_wigglewort',
    item_kind = 'aquarium_plant',
    description = 'Add one medium Wigglewort to your aquarium: a swaying stalk on the floor. Plants never starve; a sprout left to root can become one.',
    sort_order = 3301,
    updated = current_timestamp
WHERE sku = 'aquarium_fish_wigglewort';

-- The second plant, so a rooting sprout has something to choose between
-- (late-ssh seatuft.kdl).
INSERT INTO marketplace_items
    (sku, item_kind, slot, name, description, price_chips, payload, active, sort_order)
VALUES
    (
        'aquarium_plant_seatuft',
        'aquarium_plant',
        NULL,
        'Seatuft',
        'Add one small Seatuft to your aquarium: a tuft of grass on the floor. Plants never starve; a sprout left to root can become one.',
        1000,
        '{"creature":"seatuft","size":"small","width":5,"height":3,"area":15}'::jsonb,
        true,
        3300
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

-- The sprout row moves to the Plants tab, and stops naming one plant.
UPDATE marketplace_items
SET item_kind = 'aquarium_plant',
    description = 'A bud on the tank floor. One comes up every 14 days whether or not the fish eat; cut it within 7 or it roots as one of the plants and takes a plant place. Plants never die. Not for sale.',
    sort_order = 3299,
    updated = current_timestamp
WHERE sku = 'aquarium_sprout';

-- The fry's copy stops implying it could grow into a plant.
UPDATE marketplace_items
SET description = 'The hatchling every Aquarium comes with. It stays small; a streak fry grows into one of your fish and counts there. Not for sale.',
    updated = current_timestamp
WHERE sku = 'aquarium_fish_fry';
