-- Polls now take up to four options (two required, two optional).
ALTER TABLE chat_poll_options
    DROP CONSTRAINT chat_poll_options_position_check,
    ADD CONSTRAINT chat_poll_options_position_check CHECK (position BETWEEN 1 AND 4);
