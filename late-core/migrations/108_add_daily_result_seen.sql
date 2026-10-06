-- Finished daily matches linger in the lobby until each player has seen the
-- result (opened the board and left, or dismissed the row). Per-player and
-- durable: a timeout loser who was offline finds the result waiting on the
-- next login instead of the match silently vanishing.
ALTER TABLE daily_matches
    ADD COLUMN challenger_result_seen_at TIMESTAMPTZ,
    ADD COLUMN opponent_result_seen_at TIMESTAMPTZ;

-- Matches finished before this feature existed were "seen" under the old
-- rules (they just disappeared); without this backfill every past result
-- would resurface as unseen news on deploy.
UPDATE daily_matches
SET challenger_result_seen_at = current_timestamp,
    opponent_result_seen_at = current_timestamp
WHERE status = 'finished';
