DELETE FROM profile_awards
WHERE rank > 3;

ALTER TABLE profile_awards
    DROP CONSTRAINT IF EXISTS profile_awards_rank_top_three;

ALTER TABLE profile_awards
    ADD CONSTRAINT profile_awards_rank_top_three
    CHECK (rank BETWEEN 1 AND 3);
