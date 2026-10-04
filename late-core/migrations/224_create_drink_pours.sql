-- One row per drink a patron took: every pour that lands buzz through
-- late_core::models::drinks::UserDrinks::record_pour (a paid drink, a round
-- credit cashed, the round buyer's own drink), written in the same statement
-- as the user_drinks upsert, so the two cannot disagree. The newcomer's
-- welcome pour is not a drink anybody took and is not logged.
--
-- Two readers. The Leaderboards page's Top Drinkers board sums `points`
-- per user over the UTC month and the UTC year. The Nightcap's tab board
-- counts the drinks poured at the Nightcap per user, all time.
--
-- `points` is the buzz the drink was worth, before the user_drinks cap: a
-- drink taken while already wasted still counts what it poured.
CREATE TABLE drink_pours (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    bar TEXT NOT NULL CHECK (bar IN ('tavern', 'nightcap')),
    points BIGINT NOT NULL CHECK (points > 0),
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp
);

CREATE INDEX drink_pours_created_idx ON drink_pours (created);
CREATE INDEX drink_pours_bar_user_idx ON drink_pours (bar, user_id);
