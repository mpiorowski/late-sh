-- The job feed (late-ssh `app/jobs`): remote postings pulled once a day
-- from feeds published to be read (Ask HN Who is hiring, We Work Remotely,
-- Jobicy), read by the model into a card, and released onto the Jobs
-- shelf of the Profiles page.
--
-- A row is born `pending` at fetch time with the source text in `raw`,
-- becomes `queued` (HN, waiting for its day in the drip) or `active`
-- (the feeds are already spread over the week) once read, and leaves the
-- shelf as `expired`. `dropped` is a tombstone: the posting was not remote,
-- or failed the source-side filter, and the pair is kept so the next fetch
-- never reads it again. `dead` is a posting whose link answered 404.
-- Nothing beyond the excerpt is stored once a row is read: `raw` is
-- cleared on settle.
CREATE TABLE job_postings (
    id UUID PRIMARY KEY,
    source TEXT NOT NULL CHECK (source IN ('hn', 'wwr', 'jobicy')),
    external_id TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('pending', 'queued', 'active', 'expired', 'dead', 'dropped')),
    -- Model reads spent on the row; the press gives up at its cap.
    read_attempts INTEGER NOT NULL DEFAULT 0,
    url TEXT NOT NULL DEFAULT '',
    company TEXT NOT NULL DEFAULT '',
    title TEXT NOT NULL DEFAULT '',
    -- Set at fetch time when the feed says so (WWR, Jobicy), by the read
    -- for HN. A row on the shelf always has one.
    remote_kind TEXT CHECK (remote_kind IN ('worldwide', 'regions', 'hybrid')),
    regions TEXT[] NOT NULL DEFAULT ARRAY[]::TEXT[],
    tags TEXT[] NOT NULL DEFAULT ARRAY[]::TEXT[],
    pay TEXT NOT NULL DEFAULT '',
    excerpt TEXT NOT NULL DEFAULT '',
    raw TEXT NOT NULL DEFAULT '',
    posted_at TIMESTAMPTZ NOT NULL,
    -- The UTC day the row went active; the paper's NEW WORK reads by it.
    released_on DATE,
    first_seen TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    last_seen TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    UNIQUE (source, external_id),
    CHECK (status NOT IN ('queued', 'active', 'expired') OR remote_kind IS NOT NULL),
    CHECK (status NOT IN ('active', 'expired') OR released_on IS NOT NULL)
);

CREATE INDEX job_postings_shelf_idx ON job_postings (released_on DESC, posted_at DESC)
    WHERE status = 'active';
CREATE INDEX job_postings_press_idx ON job_postings (source, posted_at)
    WHERE status IN ('pending', 'queued');

-- One press run per UTC day, due at 23:30 so the day's releases are rows
-- before the paper prints at midnight. The row is the claim (root
-- CONTEXT.md, multi-replica rule): every replica checks, one runs. A
-- `running` row older than the stale bound is a dead replica's and is
-- taken over; a `failed` row is claimed again until the attempt cap.
CREATE TABLE job_press_runs (
    run_on DATE PRIMARY KEY,
    status TEXT NOT NULL CHECK (status IN ('running', 'done', 'failed')),
    attempts INTEGER NOT NULL,
    claimed_at TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    finished_at TIMESTAMPTZ,
    fetched INTEGER NOT NULL DEFAULT 0,
    read INTEGER NOT NULL DEFAULT 0,
    released INTEGER NOT NULL DEFAULT 0,
    expired INTEGER NOT NULL DEFAULT 0,
    CHECK ((status = 'done') = (finished_at IS NOT NULL))
);

-- The kill switch starts on: the press runs from the first night.
INSERT INTO app_flags (key, enabled) VALUES ('jobs_enabled', true);
