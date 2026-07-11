CREATE TABLE daily_matches (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    updated TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    game_kind TEXT NOT NULL DEFAULT 'chess',
    status TEXT NOT NULL DEFAULT 'open'
        CHECK (status IN ('open', 'active', 'finished', 'cancelled')),
    challenger_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    opponent_id UUID REFERENCES users(id) ON DELETE CASCADE,
    target_user_id UUID REFERENCES users(id) ON DELETE CASCADE,
    turn_user_id UUID REFERENCES users(id) ON DELETE CASCADE,
    turn_deadline_at TIMESTAMPTZ,
    winner_user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    result TEXT NOT NULL DEFAULT '',
    state JSONB NOT NULL DEFAULT '{}'::jsonb
);

CREATE INDEX idx_daily_matches_status ON daily_matches(status);
CREATE INDEX idx_daily_matches_turn_user
    ON daily_matches(turn_user_id) WHERE status = 'active';

INSERT INTO reward_templates
    (key, title, description, cadence, bucket, domain, difficulty, kind, params, target, reward_chips, weight, is_quest, claim_policy, cooldown_seconds)
VALUES
    ('daily_chess_win_payout', 'Win Daily Chess', 'Win a decisive daily chess match.', NULL, NULL, 'strategy', 'hard', 'game_win', '{"game":"daily_chess","payout_kind":"win"}'::jsonb, 1, 500, 100, false, 'per_event', NULL);
