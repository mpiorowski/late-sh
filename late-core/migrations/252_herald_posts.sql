-- The herald (late-ssh `app/herald`): one picture a day, delivered to every
-- network the account is on (Mastodon, Bluesky).
--
-- `herald_posts` is one row per UTC day, the claim that makes several
-- replicas post exactly once (root CONTEXT.md, multi-replica rule). `id`
-- is minted at the first claim and never changes: the Mastodon status call
-- carries it as its Idempotency-Key and the Bluesky record key is derived
-- from it, so a retry after a lost answer lands on the post the network
-- already has instead of a second one. `posting` is held while the calls
-- run (a stale one, older than the sweeper's reclaim window, is taken
-- over); `picture` is the chosen picture, written before the first call so
-- a retry delivers the same one; `posted` means every configured network
-- delivered; `skipped` records a day with nothing worth posting; `failed`
-- keeps the attempt count for the next check to claim again until the
-- sweeper's cap, after which the day rests.
CREATE TABLE herald_posts (
    day DATE PRIMARY KEY,
    id UUID NOT NULL UNIQUE,
    status TEXT NOT NULL CHECK (status IN ('posting', 'posted', 'skipped', 'failed')),
    -- The pick: its kind, its subject (`daily:YYYY-MM-DD` for the Artboard,
    -- the runner's user id for a poster, so a poster goes out once and the
    -- rotation can read what ran the day before), and the picture itself
    -- as the app serializes it.
    kind TEXT CHECK (kind IN ('artboard', 'poster')),
    subject TEXT,
    picture JSONB,
    -- Claims taken on this row, counting the first.
    attempts INTEGER NOT NULL,
    claimed_at TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    finished_at TIMESTAMPTZ,
    CHECK ((kind IS NULL) = (subject IS NULL) AND (kind IS NULL) = (picture IS NULL)),
    CHECK (status <> 'posted' OR kind IS NOT NULL),
    CHECK ((status IN ('posted', 'skipped')) = (finished_at IS NOT NULL))
);

CREATE INDEX herald_posts_kind_subject_idx ON herald_posts (kind, subject);

-- One row per network a day's picture landed on, written as each call
-- succeeds: a retry reads these and sends only to the networks still
-- missing.
CREATE TABLE herald_deliveries (
    day DATE NOT NULL REFERENCES herald_posts(day) ON DELETE CASCADE,
    network TEXT NOT NULL CHECK (network IN ('mastodon', 'bluesky')),
    url TEXT NOT NULL,
    delivered_at TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    PRIMARY KEY (day, network)
);

-- The operator's switch, a herald-owned row like `greendragon_settings`
-- (the process-wide `app_flags` table went in migration 219). It starts
-- off: the code ships dark, and the operator flips it once an account
-- exists and the first hour holds (AUDIENCE.md, the herald). `false`
-- stops the next check without a deploy.
CREATE TABLE herald_settings (
    key TEXT PRIMARY KEY,
    enabled BOOLEAN NOT NULL,
    updated TIMESTAMPTZ NOT NULL DEFAULT current_timestamp
);

INSERT INTO herald_settings (key, enabled) VALUES ('posting', false);
