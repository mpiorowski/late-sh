-- Booth History no longer carries community votes. Ordering and pruning are
-- pure recency (last_played_at), so the vote table is dead weight.
DROP TABLE IF EXISTS media_history_votes;
