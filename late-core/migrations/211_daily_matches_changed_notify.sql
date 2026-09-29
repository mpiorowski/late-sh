-- Cross-replica daily match refresh goes through LISTEN/NOTIFY (root
-- CONTEXT.md, multi-replica rule). Every replica LISTENs on
-- `daily_match_changed` and re-reads its lobby snapshot (open challenges,
-- active matches, unseen results), so a move, a claim, a finish or a payout
-- written on one replica reaches the #lounge strip and the lobby on every
-- other one at once instead of on the next 60s sweep. Statement level with
-- an empty payload: the listener re-reads, it never trusts a payload.
CREATE OR REPLACE FUNCTION notify_daily_match_changed() RETURNS trigger AS $$
BEGIN
    PERFORM pg_notify('daily_match_changed', '');
    RETURN NULL;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER daily_matches_changed
    AFTER INSERT OR UPDATE OR DELETE ON daily_matches
    FOR EACH STATEMENT EXECUTE FUNCTION notify_daily_match_changed();
