-- Green Dragon shared game state: a tiny key/value store for the few values
-- that are one shared global across all players (LoGD keeps these as module
-- settings). First occupant: the Dark Horse Tavern's Five Sixes jackpot,
-- seeded at its stock starting pot of 100 gold.
CREATE TABLE greendragon_settings (
    key TEXT PRIMARY KEY,
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    updated TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    value BIGINT NOT NULL
);

INSERT INTO greendragon_settings (key, value) VALUES ('fivesix_jackpot', 100);
