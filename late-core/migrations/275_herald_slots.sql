-- The herald's two posts a day (late-ssh `app/herald`): the words slot (the
-- operator's announcements, a member's project, the paper, the week's
-- scoreboard) and the art slot (the Artboard or a gallery piece). A claim is
-- now one slot of one UTC day, so `herald_posts` and `herald_deliveries` key on
-- `(day, slot)`. Each row keeps its own `id`, so each post has its own
-- idempotency key on Mastodon and its own record key on Bluesky.
ALTER TABLE herald_deliveries DROP CONSTRAINT herald_deliveries_pkey;
ALTER TABLE herald_deliveries DROP CONSTRAINT herald_deliveries_day_fkey;
ALTER TABLE herald_posts DROP CONSTRAINT herald_posts_pkey;

ALTER TABLE herald_posts ADD COLUMN slot TEXT CHECK (slot IN ('words', 'art'));
UPDATE herald_posts
SET slot = CASE WHEN kind IN ('artboard', 'piece') THEN 'art' ELSE 'words' END;
ALTER TABLE herald_posts ALTER COLUMN slot SET NOT NULL;
ALTER TABLE herald_posts ADD PRIMARY KEY (day, slot);

ALTER TABLE herald_deliveries ADD COLUMN slot TEXT;
UPDATE herald_deliveries AS delivery
SET slot = post.slot
FROM herald_posts AS post
WHERE post.day = delivery.day;
ALTER TABLE herald_deliveries ALTER COLUMN slot SET NOT NULL;
ALTER TABLE herald_deliveries ADD PRIMARY KEY (day, slot, network);
ALTER TABLE herald_deliveries
    ADD FOREIGN KEY (day, slot) REFERENCES herald_posts (day, slot) ON DELETE CASCADE;

-- A member's project (subject `project:{showcase id}`).
ALTER TABLE herald_posts DROP CONSTRAINT herald_posts_kind_check;
ALTER TABLE herald_posts ADD CONSTRAINT herald_posts_kind_check
    CHECK (kind IN ('artboard', 'piece', 'scoreboard', 'paper', 'announcement', 'project'));

-- The days the checker could still owe (it owes today or yesterday) already
-- had their one post: their other slot is settled unposted, or the first check
-- after this migration would catch it up beside a post that already went out.
INSERT INTO herald_posts
    (day, slot, id, status, attempts, claimed_at, first_claimed_at, finished_at)
SELECT
    day,
    CASE slot WHEN 'art' THEN 'words' ELSE 'art' END,
    uuidv7(),
    'skipped',
    0,
    current_timestamp,
    current_timestamp,
    current_timestamp
FROM herald_posts
WHERE day >= (current_timestamp AT TIME ZONE 'UTC')::date - 1;
