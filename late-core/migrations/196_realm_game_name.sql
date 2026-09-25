-- Every realm game gets a name the creator picks, so the Lobby lists several
-- running games as something other than three rows of "standard · earth".
-- Empty means the service fills in "<creator>'s realm".
ALTER TABLE realm_games
    ADD COLUMN name TEXT NOT NULL DEFAULT '';
