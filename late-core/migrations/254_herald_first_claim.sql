-- The herald's claim window (late-ssh `app/herald/svc.rs`,
-- HERALD_CLAIM_WINDOW): Mastodon keeps an Idempotency-Key for about an
-- hour, so a day is never claimed again once its first claim is older than
-- that, and a stale `posting` row past it rests as `failed`. `claimed_at`
-- moves on every reclaim; this is the first one, written by the app at the
-- first claim and never again. Existing rows take the migration's moment,
-- then the default goes: the app always writes it.
ALTER TABLE herald_posts ADD COLUMN first_claimed_at TIMESTAMPTZ NOT NULL DEFAULT current_timestamp;
ALTER TABLE herald_posts ALTER COLUMN first_claimed_at DROP DEFAULT;
