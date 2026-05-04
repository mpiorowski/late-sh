ALTER TABLE work_profiles
    ADD COLUMN IF NOT EXISTS contact TEXT NOT NULL DEFAULT '' CHECK (length(contact) <= 200);
