-- The pet is a mood indicator now, not a mouth to feed: no meal, no bowl, no
-- chips. The care columns go. What stays is the mood the owner's session
-- last inferred, so a profile can show it while the owner is away (a
-- session writes 'asleep' on its way out).
ALTER TABLE pet_companions
    DROP COLUMN last_fed,
    DROP COLUMN last_watered,
    DROP COLUMN last_played,
    DROP COLUMN last_treated,
    DROP COLUMN care_streak_days,
    DROP COLUMN care_streak_date,
    ADD COLUMN mood TEXT NOT NULL DEFAULT 'asleep',
    ADD COLUMN mood_since TIMESTAMPTZ NOT NULL DEFAULT current_timestamp;

-- The bird joins the cat and the dog. Both columns are closed enums in
-- `late-core/src/models/pet.rs`; the checks keep the rows honest.
ALTER TABLE pet_companions
    ADD CONSTRAINT pet_companions_species_check
        CHECK (species IN ('cat', 'dog', 'bird')),
    ADD CONSTRAINT pet_companions_mood_check
        CHECK (mood IN ('purring', 'proud', 'sulking', 'chatty', 'vibing', 'asleep', 'idle'));

UPDATE marketplace_items
SET description = 'A cat, a dog, or a bird for your Zen page. Nothing to feed: it reads your session (a win, a loss, a message, the music, a long silence) and wears the mood on your profile. Click it to pet it.',
    updated = current_timestamp
WHERE sku = 'pet_companion';
