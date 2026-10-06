-- A fight on the row is a round of cards: it has `piles`. A binary from
-- before the round, still draining its sessions beside the new one, writes
-- the exchange loop's fight with none, and the round cannot read that row
-- at all. The check refuses the write, so a step in on a draining pod
-- fails whole (no ration spent) and the runner fights after reconnecting.
--
-- The lock keeps such a write from landing between the cleanup and the
-- check; anything that slipped in since the nuke gets its day back.
LOCK TABLE deadchannel_runners IN ACCESS EXCLUSIVE MODE;

UPDATE deadchannel_runners
SET fight = NULL,
    road = NULL,
    rations_left = 10,
    kills_today = 0,
    runs_today = 0,
    updated = current_timestamp
WHERE fight IS NOT NULL AND NOT (fight ? 'piles');

ALTER TABLE deadchannel_runners
    ADD CONSTRAINT deadchannel_runners_fight_has_piles
    CHECK (fight IS NULL OR fight ? 'piles');
