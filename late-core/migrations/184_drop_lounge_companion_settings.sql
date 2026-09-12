-- The Home Lounge tank tray and pet strip are gone: the pet and the tank
-- live on the Zen page only. Nothing reads these two settings keys any
-- more, so the rows stop carrying them.
UPDATE users
SET settings = settings - 'show_aquarium_tray' - 'show_pet_strip',
    updated = current_timestamp
WHERE settings ? 'show_aquarium_tray'
   OR settings ? 'show_pet_strip';
