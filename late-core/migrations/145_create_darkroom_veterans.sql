-- Which accounts have finished A Dark Room.
--
-- Winning the ascent deletes the user's `darkroom_saves` row (see the door's
-- CONTEXT.md, "The two endings"), so the save itself can never carry anything
-- forward. This table is the one thing that outlives it, and it holds exactly
-- one fact because exactly one thing reads it: an account that has got off
-- this rock once finds the ravaged battleship on every later map.
--
-- Deliberately not a counter and deliberately not a score. A Dark Room pays
-- badges, not standings, and it is a game you finish twice at most: which
-- endings an account has reached is recorded permanently by its `ADE`/`ADB`
-- rows in `profile_awards`, and nothing needs a tally on top of that.
CREATE TABLE darkroom_veterans (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE UNIQUE
);

-- Whoever already escaped holds the ADE badge and nothing else records it.
-- Seed them in, so the battleship is unlocked for them on their next visit
-- rather than making them win a second time to earn what they already have.
INSERT INTO darkroom_veterans (user_id)
SELECT user_id
FROM profile_awards
WHERE category = 'darkroom_escape'
ON CONFLICT (user_id) DO NOTHING;
