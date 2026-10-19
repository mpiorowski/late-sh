ALTER TABLE user_avatars
    DROP CONSTRAINT user_avatars_size_check,
    ADD CONSTRAINT user_avatars_size_check CHECK (size BETWEEN 1 AND 32);
