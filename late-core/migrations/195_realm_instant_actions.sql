-- Realm moves off midnight resolution: actions resolve the moment they are
-- taken, so the hidden per-day queues have nothing left to hold. What a day
-- still governs is the action budget, and each game now refills at its own
-- hour so a US game and an EU game can both sit at a civilised local time.
ALTER TABLE realm_games
    ADD COLUMN reset_hour_utc SMALLINT NOT NULL DEFAULT 0
        CHECK (reset_hour_utc BETWEEN 0 AND 23);

-- The queues are gone; `realm_days` keeps the logs, which now fill in live
-- and are archived when a day rolls over.
DROP TABLE IF EXISTS realm_actions;
