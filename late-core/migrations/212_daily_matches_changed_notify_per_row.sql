-- Migration 211's trigger was statement level, and a statement-level trigger
-- fires even when the statement changes no row: every replica's sweeper
-- runs a guarded UPDATE each minute that almost always matches nothing, so
-- every replica re-read its lobby snapshot for no news. Row level fires only
-- for rows that changed. The payload stays empty, and Postgres collapses
-- identical notifies inside one transaction, so a write over many rows is
-- still one notify.
DROP TRIGGER daily_matches_changed ON daily_matches;

CREATE TRIGGER daily_matches_changed
    AFTER INSERT OR UPDATE OR DELETE ON daily_matches
    FOR EACH ROW EXECUTE FUNCTION notify_daily_match_changed();
