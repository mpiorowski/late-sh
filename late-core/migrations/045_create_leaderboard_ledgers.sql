CREATE TABLE chip_ledger (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    delta BIGINT NOT NULL,
    reason TEXT NOT NULL,
    source_kind TEXT,
    source_ref TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT current_timestamp
);

CREATE INDEX chip_ledger_user_created_idx ON chip_ledger (user_id, created_at DESC);
CREATE INDEX chip_ledger_positive_created_idx ON chip_ledger (created_at DESC, user_id)
    WHERE delta > 0;

CREATE TABLE game_score_events (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    game TEXT NOT NULL,
    score INT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT current_timestamp
);

CREATE INDEX game_score_events_game_created_idx ON game_score_events (game, created_at DESC);
CREATE INDEX game_score_events_user_game_created_idx ON game_score_events (user_id, game, created_at DESC);
