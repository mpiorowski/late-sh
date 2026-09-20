-- Petting the pet pays: the first pet of the UTC day credits 100 chips,
-- the way the tank's first feed does. `last_petted` is the daily gate the
-- credit reads, written by the same conditional update that decides it.
ALTER TABLE pet_companions
    ADD COLUMN IF NOT EXISTS last_petted DATE;

UPDATE marketplace_items
SET description = 'A cat, a dog, or a bird for your Zen page. Nothing to feed: it reads your session (a win, a loss, a message, the music, a long silence) and wears the mood on your profile. Click it to pet it: the first pet of the day pays 100 chips.',
    updated = current_timestamp
WHERE sku = 'pet_companion';
