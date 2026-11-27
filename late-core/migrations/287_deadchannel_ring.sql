-- The ring (`app/deadchannel/ring`, GAME.md "The arena pass"): a live 1v1
-- card duel between two runners, called out by one, accepted by the
-- other, fought round by round while the rest of the street bets bits on
-- a side. Three tables: the bouts, the bets on them, and the deck each
-- runner brings to the ring, and one column on the runner.
--
-- One row per call to the ring. A call is open until the called runner
-- accepts it, either runner declines it, or `expires_at` passes (the
-- sweeper ends it as expired). Accepting snapshots both decks into the
-- row, rings the bell at `bell_at` (bets close then) and starts round 1,
-- due by `deadline_at`; every round after rewrites `deadline_at` and
-- `state`, the duel as JSON only the app reads (the round is in it, and
-- nowhere else). `accepted_at` and
-- `bell_at` are set together, so either one says the bout was fought.
-- `pool_caller` and `pool_called` are the bits bet on each side, added
-- before the bell only. `outcome` is the side that won, a draw, or a call
-- declined or left to expire. Once a fought bout ends, `caller_settled_at`
-- and `called_settled_at` are each fighter's settle claim: the replica
-- that takes one pays that fighter, and one left untaken by a replica that
-- died is picked up by any sweeper.
--
-- The three unique indexes are the rules: one fought bout in the ring at
-- a time, one open call or bout per caller, and two runners fight once a
-- day. The first and the last are partial on `accepted_at`, so an open
-- call holds neither the ring nor the pair's day: accepting is what
-- claims them, and an accept that loses one is the rule refusing.
CREATE TABLE deadchannel_bouts (
    id UUID PRIMARY KEY,
    day DATE NOT NULL,
    caller_user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    called_user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    caller_username TEXT NOT NULL,
    called_username TEXT NOT NULL,
    caller_deck JSONB,
    called_deck JSONB,
    called_at TIMESTAMPTZ NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    accepted_at TIMESTAMPTZ,
    bell_at TIMESTAMPTZ,
    deadline_at TIMESTAMPTZ,
    state JSONB,
    pool_caller BIGINT NOT NULL DEFAULT 0 CHECK (pool_caller >= 0),
    pool_called BIGINT NOT NULL DEFAULT 0 CHECK (pool_called >= 0),
    ended_at TIMESTAMPTZ,
    outcome TEXT CHECK (outcome IN ('caller', 'called', 'draw', 'declined', 'expired')),
    caller_settled_at TIMESTAMPTZ,
    called_settled_at TIMESTAMPTZ,
    CHECK ((ended_at IS NULL) = (outcome IS NULL)),
    CHECK (caller_user_id <> called_user_id),
    CHECK ((accepted_at IS NULL) = (bell_at IS NULL))
);

-- One accepted, unfinished bout at a time, whatever any number of
-- replicas try.
CREATE UNIQUE INDEX deadchannel_bouts_one_ring
    ON deadchannel_bouts ((true)) WHERE accepted_at IS NOT NULL AND ended_at IS NULL;

-- One open call or bout per caller: a runner calls one rival at a time.
CREATE UNIQUE INDEX deadchannel_bouts_one_call
    ON deadchannel_bouts (caller_user_id) WHERE ended_at IS NULL;

-- Two runners fight once a day, whichever of them called.
CREATE UNIQUE INDEX deadchannel_bouts_one_per_pair
    ON deadchannel_bouts (
        LEAST(caller_user_id, called_user_id),
        GREATEST(caller_user_id, called_user_id),
        day
    ) WHERE accepted_at IS NOT NULL;

-- What the sweeper reads: the open rows, calls waiting on an answer and
-- the bout in the ring, a handful at any time.
CREATE INDEX deadchannel_bouts_open
    ON deadchannel_bouts (expires_at) WHERE ended_at IS NULL;

-- Every change to a bout tells every replica, from the write itself, so
-- no write path can forget to. The payload is the bout's id.
CREATE FUNCTION notify_deadchannel_bout_changed() RETURNS trigger AS $$
BEGIN
    PERFORM pg_notify('deadchannel_bout', NEW.id::text);
    RETURN NULL;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER deadchannel_bouts_changed
    AFTER INSERT OR UPDATE ON deadchannel_bouts
    FOR EACH ROW EXECUTE FUNCTION notify_deadchannel_bout_changed();

-- One row per runner betting on a bout: one side, pressed again to add
-- bits (`presses` counts the bets folded into the row). `paid_at` is the
-- pay claim once the bout ends: the replica that takes it pays
-- `paid_bits` (nothing for a losing side), and one left unpaid by a
-- replica that died is picked up by any sweeper.
CREATE TABLE deadchannel_bout_bets (
    bout_id UUID NOT NULL REFERENCES deadchannel_bouts(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    username TEXT NOT NULL,
    side TEXT NOT NULL CHECK (side IN ('caller', 'called')),
    bits BIGINT NOT NULL CHECK (bits > 0),
    presses INT NOT NULL DEFAULT 1 CHECK (presses > 0),
    placed_at TIMESTAMPTZ NOT NULL,
    paid_at TIMESTAMPTZ,
    paid_bits BIGINT,
    PRIMARY KEY (bout_id, user_id)
);

-- What the ledge asks before it lets a runner go: a bet of theirs the
-- ring has not paid yet.
CREATE INDEX deadchannel_bout_bets_unpaid
    ON deadchannel_bout_bets (user_id) WHERE paid_at IS NULL;

-- A bet placed or paid is a change to its bout: the same channel, the
-- bout's id.
CREATE FUNCTION notify_deadchannel_bout_bet_changed() RETURNS trigger AS $$
BEGIN
    PERFORM pg_notify('deadchannel_bout', NEW.bout_id::text);
    RETURN NULL;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER deadchannel_bout_bets_changed
    AFTER INSERT OR UPDATE ON deadchannel_bout_bets
    FOR EACH ROW EXECUTE FUNCTION notify_deadchannel_bout_bet_changed();

-- The deck a runner brings to the ring, as JSON the app reads: set
-- whenever they change it, copied into the bout when one is accepted.
CREATE TABLE deadchannel_ring_decks (
    user_id UUID PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    cards JSONB NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL
);

-- The highest level whose ring gate this climb has cleared.
ALTER TABLE deadchannel_runners
    ADD COLUMN ring_cleared INT NOT NULL DEFAULT 0 CHECK (ring_cleared >= 0);

-- The nuke (migration 248) with the ring in it. The sheets go back to
-- fresh ones, so nothing at the ring may pay onto them afterwards: every
-- bet still unpaid is void, every call still open lapses, and a bout
-- being fought ends a draw with both fighters settled, so no sweeper has
-- anything left to hand out. The gates go back with the marks. The deck a
-- runner brings stays, as the look does.
CREATE OR REPLACE FUNCTION deadchannel_nuke_runners() RETURNS void AS $$
    UPDATE deadchannel_ambushes
    SET ended_at = current_timestamp,
        outcome = 'dropped',
        bits_moved = 0
    WHERE ended_at IS NULL;

    UPDATE deadchannel_bout_bets
    SET paid_at = current_timestamp,
        paid_bits = 0
    WHERE paid_at IS NULL;

    UPDATE deadchannel_bouts
    SET ended_at = current_timestamp,
        outcome = 'expired'
    WHERE accepted_at IS NULL AND ended_at IS NULL;

    UPDATE deadchannel_bouts
    SET ended_at = COALESCE(ended_at, current_timestamp),
        outcome = COALESCE(outcome, 'draw'),
        caller_settled_at = COALESCE(caller_settled_at, current_timestamp),
        called_settled_at = COALESCE(called_settled_at, current_timestamp)
    WHERE accepted_at IS NOT NULL
      AND (ended_at IS NULL OR caller_settled_at IS NULL OR called_settled_at IS NULL);

    UPDATE deadchannel_runners
    SET level = 1,
        peak_level = 1,
        exp = 0,
        signal = 10,
        weapon_tier = 0,
        armor_tier = 0,
        bits = 50,
        rations_left = 10,
        day = (now() AT TIME ZONE 'utc')::date,
        fight = NULL,
        road = NULL,
        cards = NULL,
        inks = NULL,
        ring_cleared = 0,
        kills = 0,
        kills_today = 0,
        runs_today = 0,
        marks = 0,
        stash = 0,
        debt = 0,
        crystals = 0,
        drink = NULL,
        reset_generation = reset_generation + 1,
        updated = current_timestamp;
$$ LANGUAGE sql;
