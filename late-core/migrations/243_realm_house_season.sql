-- The house runs the realm: one standing world at a time, opened by the
-- server as a numbered season, the next one rising when the last one falls.
-- `season` numbers the worlds in order; rows from before the house took over
-- keep 0.
ALTER TABLE realm_games
    ADD COLUMN season INTEGER NOT NULL DEFAULT 0;

-- At most one realm is ever open or active. Player-made realms from before
-- this are closed, the newest kept as the standing world until it ends.
UPDATE realm_games
   SET status = 'cancelled',
       updated = current_timestamp
 WHERE status IN ('open', 'active')
   AND id <> (
       SELECT id FROM realm_games
        WHERE status IN ('open', 'active')
        ORDER BY created DESC, id DESC
        LIMIT 1
   );

-- The invariant the sweeper leans on: two replicas opening the next season
-- at once both insert, and the second insert fails here instead of leaving
-- two worlds standing.
CREATE UNIQUE INDEX realm_games_one_live
    ON realm_games ((true))
    WHERE status IN ('open', 'active');
