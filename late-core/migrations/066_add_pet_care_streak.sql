ALTER TABLE pet_companions
    ADD COLUMN care_streak_days INT NOT NULL DEFAULT 0,
    ADD COLUMN care_streak_date DATE;
