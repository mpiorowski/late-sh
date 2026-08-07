-- Roguelike-door log ingestion (devdocs/PLAN-ROGUELIKE-BOARDS.md, Phase 1:
-- DCSS). The door hosts append spoof-proof xlog files on their PVCs; late-ssh
-- tails them over a stats SSH session and lands facts here. Idempotency is
-- structural: the source files are append-only and single-writer, so
-- (game, source_file, source_offset) uniquely names a line, and re-ingest
-- after a cursor reset is a no-op.

-- One row per finished game. `result` is a text key from the closed Rust
-- enum DoorRunResult (death/quit/leaving/win/mastery); `raw` keeps the full
-- parsed line for boards not invented yet.
CREATE TABLE door_runs (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    updated TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    game TEXT NOT NULL,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    ended_at TIMESTAMPTZ NOT NULL,
    result TEXT NOT NULL,
    score BIGINT,
    depth INT,
    turns BIGINT,
    raw JSONB NOT NULL,
    source_file TEXT NOT NULL,
    source_offset BIGINT NOT NULL,
    UNIQUE (game, source_file, source_offset)
);

-- The leaderboard pass groups by (game, user_id) and windows on ended_at.
CREATE INDEX door_runs_game_user_idx ON door_runs (game, user_id);
CREATE INDEX door_runs_game_ended_at_idx ON door_runs (game, ended_at);

-- Live mid-run milestone stream (DCSS's milestones file; NetHack's livelog
-- if Phase 2 finds it). `kind` is a text key from the closed Rust enum
-- DoorMilestoneKind. Orb-pickup badges grant from here so they land at
-- pickup, not at game end.
CREATE TABLE door_milestones (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    updated TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    game TEXT NOT NULL,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    kind TEXT NOT NULL,
    occurred_at TIMESTAMPTZ NOT NULL,
    raw JSONB NOT NULL,
    source_file TEXT NOT NULL,
    source_offset BIGINT NOT NULL,
    UNIQUE (game, source_file, source_offset)
);

CREATE INDEX door_milestones_game_user_idx ON door_milestones (game, user_id);

-- Where ingestion resumed reading each host file: the byte offset of the
-- first unread line, updated in the same transaction as the fact insert. A
-- missing row means start at 0, which ingests the history already on the
-- PVC, so boards launch non-empty.
CREATE TABLE door_log_cursors (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    updated TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    game TEXT NOT NULL,
    file TEXT NOT NULL,
    next_offset BIGINT NOT NULL,
    UNIQUE (game, file)
);

-- Lifetime badge payouts for the DCSS pair, the exact twins of the NetHack
-- pair (090): Orb pickup mirrors the Amulet pickup, the win mirrors
-- ascension. Once per account, enforced by the lifetime game_payout_claims
-- row the credit path takes.
INSERT INTO reward_templates
    (key, title, description, cadence, bucket, domain, difficulty, kind, params, target, reward_chips, weight, is_quest, claim_policy, cooldown_seconds)
VALUES
    (
        'dcss_orb',
        'Find the Orb of Zot',
        'Descend through the Realm of Zot and pick up the Orb in Dungeon Crawl Stone Soup. Awards chips once per account.',
        NULL,
        NULL,
        'dcss',
        'hard',
        'game_win',
        '{"game":"dcss","payout_kind":"orb_found"}'::jsonb,
        1,
        10000,
        100,
        false,
        'per_event',
        NULL
    ),
    (
        'dcss_win',
        'Escape with the Orb',
        'Carry the Orb of Zot back up and out of the dungeon in Dungeon Crawl Stone Soup. Awards chips once per account.',
        NULL,
        NULL,
        'dcss',
        'hard',
        'game_win',
        '{"game":"dcss","payout_kind":"orb_escape"}'::jsonb,
        1,
        20000,
        100,
        false,
        'per_event',
        NULL
    )
ON CONFLICT (key) DO UPDATE SET
    title = EXCLUDED.title,
    description = EXCLUDED.description,
    cadence = EXCLUDED.cadence,
    bucket = EXCLUDED.bucket,
    domain = EXCLUDED.domain,
    difficulty = EXCLUDED.difficulty,
    kind = EXCLUDED.kind,
    params = EXCLUDED.params,
    target = EXCLUDED.target,
    reward_chips = EXCLUDED.reward_chips,
    weight = EXCLUDED.weight,
    is_quest = EXCLUDED.is_quest,
    claim_policy = EXCLUDED.claim_policy,
    cooldown_seconds = EXCLUDED.cooldown_seconds,
    active = true,
    updated = current_timestamp;
