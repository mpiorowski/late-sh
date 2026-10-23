-- The herald (late-ssh `app/herald`): one picture a day on the Fediverse.
--
-- One row per UTC day, the claim that makes several replicas post exactly
-- once (root CONTEXT.md, multi-replica rule). `id` is minted at the first
-- claim and never changes: it is the Idempotency-Key on the status call,
-- so a retry after a lost response gets the status the instance already
-- created instead of a second one. `posting` is held while the calls run
-- (a stale one, older than the sweeper's reclaim window, is taken over);
-- `posted` carries what went out and where; `skipped` records that the
-- day had nothing worth posting; `failed` keeps the attempt count for the
-- next check to claim again until the sweeper's cap, after which the day
-- rests.
CREATE TABLE herald_posts (
    day DATE PRIMARY KEY,
    id UUID NOT NULL UNIQUE,
    status TEXT NOT NULL CHECK (status IN ('posting', 'posted', 'skipped', 'failed')),
    -- What went out: the slot kind and its subject (`daily:YYYY-MM-DD` for
    -- the Artboard, the runner's user id for a poster), so a poster goes
    -- out once and the rotation can read what ran the day before.
    kind TEXT CHECK (kind IN ('artboard', 'poster')),
    subject TEXT,
    status_url TEXT,
    -- Claims taken on this row, counting the first.
    attempts INTEGER NOT NULL,
    claimed_at TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    finished_at TIMESTAMPTZ,
    CHECK ((status = 'posted') = (kind IS NOT NULL AND subject IS NOT NULL AND status_url IS NOT NULL)),
    CHECK ((status IN ('posted', 'skipped')) = (finished_at IS NOT NULL))
);

CREATE INDEX herald_posts_kind_subject_idx ON herald_posts (kind, subject);

-- The switch starts off: the code ships dark, and the operator flips it
-- once the account exists and the first hour holds (AUDIENCE.md, the
-- herald). The first switch in this table with a reader
-- (`models/app_flag.rs`).
INSERT INTO app_flags (key, enabled) VALUES ('herald_enabled', false);
