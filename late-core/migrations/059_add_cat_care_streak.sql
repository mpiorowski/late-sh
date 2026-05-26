ALTER TABLE cat_companions
    ADD COLUMN care_streak_days INTEGER NOT NULL DEFAULT 0,
    ADD COLUMN care_streak_last_day DATE;
