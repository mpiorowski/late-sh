CREATE TABLE game_payout_claims (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    updated TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    game TEXT NOT NULL CHECK (length(btrim(game)) > 0),
    payout_kind TEXT NOT NULL CHECK (length(btrim(payout_kind)) > 0),
    period_kind TEXT NOT NULL CHECK (length(btrim(period_kind)) > 0),
    period_key TEXT NOT NULL CHECK (length(btrim(period_key)) > 0),
    amount BIGINT NOT NULL CHECK (amount > 0),
    UNIQUE(user_id, game, payout_kind, period_kind, period_key)
);

CREATE INDEX game_payout_claims_user_created_idx
    ON game_payout_claims (user_id, created DESC);

CREATE INDEX game_payout_claims_user_kind_created_idx
    ON game_payout_claims (user_id, game, payout_kind, period_kind, created DESC);

CREATE INDEX game_payout_claims_game_period_idx
    ON game_payout_claims (game, payout_kind, period_kind, period_key);
