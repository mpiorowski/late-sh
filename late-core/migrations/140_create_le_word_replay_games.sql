CREATE TABLE le_word_replay_games (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    updated TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    user_id UUID NOT NULL UNIQUE REFERENCES users(id) ON DELETE CASCADE,
    answer_word TEXT NOT NULL CHECK (answer_word ~ '^[a-z]{5}$'),
    guesses JSONB NOT NULL DEFAULT '[]'::jsonb,
    current_guess TEXT NOT NULL DEFAULT '',
    is_game_over BOOLEAN NOT NULL DEFAULT false,
    won BOOLEAN NOT NULL DEFAULT false
);
