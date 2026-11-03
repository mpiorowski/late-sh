-- The owner's switch for the pet: off, it stays bought but is put away, with
-- only a note in its tile, no sidebar panel, no profile portrait and no daily
-- reminder.
ALTER TABLE pet_companions
    ADD COLUMN IF NOT EXISTS enabled BOOLEAN NOT NULL DEFAULT true;
