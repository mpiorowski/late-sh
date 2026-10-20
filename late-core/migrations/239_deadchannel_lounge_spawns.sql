-- The lounge spawns (GAME.md, "The lounge pass"): a glyph in #lounge that
-- runners put down together. Two tables: the day's budget and its dice,
-- and the spawns themselves.
--
-- One row per UTC day. The first replica to touch a day inserts it with
-- the dice already rolled: a minute inside each eight-hour window for its
-- flicker and a minute anywhere in the day for the howler. Every replica
-- then reads the same moments, and a spawn is a conditional claim on the
-- spent flag (`WHERE NOT flicker_0_spent ...`), so any number of replicas
-- spend each one exactly once. `last_spawn_at` holds the gap between two
-- spawns in the same statement.
CREATE TABLE deadchannel_lounge_days (
    day DATE PRIMARY KEY,
    flicker_0_minute INT NOT NULL CHECK (flicker_0_minute >= 0 AND flicker_0_minute < 480),
    flicker_1_minute INT NOT NULL CHECK (flicker_1_minute >= 480 AND flicker_1_minute < 960),
    flicker_2_minute INT NOT NULL CHECK (flicker_2_minute >= 960 AND flicker_2_minute < 1440),
    howler_minute INT NOT NULL CHECK (howler_minute >= 0 AND howler_minute < 1440),
    flicker_0_spent BOOLEAN NOT NULL DEFAULT false,
    flicker_1_spent BOOLEAN NOT NULL DEFAULT false,
    flicker_2_spent BOOLEAN NOT NULL DEFAULT false,
    howler_spent BOOLEAN NOT NULL DEFAULT false,
    last_spawn_at TIMESTAMPTZ
);

-- One row per spawn. `hits` is who hit it, in order, as JSON the app
-- parses (`[{"user_id", "username", "at_ms"}]`); `hits_needed` is how many
-- put it down. A hit is one conditional update that appends to `hits`
-- and, when that makes the count, ends the spawn as slain in the same
-- statement, so the row decides who landed the last hit. The sweeper ends
-- one past `expires_at` as slipped. `paid_at` is the pay claim: the
-- replica that takes it pays every hitter, and one left unpaid by a
-- replica that died is picked up by any sweeper.
CREATE TABLE deadchannel_spawns (
    id UUID PRIMARY KEY,
    kind TEXT NOT NULL CHECK (kind IN ('flicker', 'howler')),
    hits_needed INT NOT NULL CHECK (hits_needed > 0),
    hits JSONB NOT NULL DEFAULT '[]'::jsonb,
    spawned_at TIMESTAMPTZ NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    ended_at TIMESTAMPTZ,
    outcome TEXT CHECK (outcome IN ('slain', 'slipped')),
    paid_at TIMESTAMPTZ,
    CHECK ((ended_at IS NULL) = (outcome IS NULL))
);

-- One live spawn at a time, whatever any number of replicas try.
CREATE UNIQUE INDEX deadchannel_spawns_one_live
    ON deadchannel_spawns ((true)) WHERE ended_at IS NULL;

-- What the sweeper looks for: live spawns past their time, and slain ones
-- nobody paid.
CREATE INDEX deadchannel_spawns_open
    ON deadchannel_spawns (ended_at) WHERE paid_at IS NULL;
