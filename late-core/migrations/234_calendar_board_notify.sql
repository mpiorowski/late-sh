-- calendar_changed makes every session read the board again, so it fires
-- only for what another session can see: a board post's insert, edit or
-- delete, and an "I'm in". A personal event is read only by its owner's
-- own session, which reloads after its own writes. Row-level, so a
-- statement that changes no row (the start sweeper's claim on an empty
-- minute, an "I'm in" said twice) stays quiet. Postgres folds a
-- transaction's identical notifications into one.
DROP TRIGGER calendar_events_changed ON calendar_events;
DROP TRIGGER calendar_rsvps_changed ON calendar_rsvps;

CREATE TRIGGER calendar_board_inserted AFTER INSERT ON calendar_events
    FOR EACH ROW WHEN (NEW.owner_id IS NULL) EXECUTE FUNCTION calendar_changed_notify();
CREATE TRIGGER calendar_board_updated AFTER UPDATE ON calendar_events
    FOR EACH ROW WHEN (NEW.owner_id IS NULL) EXECUTE FUNCTION calendar_changed_notify();
CREATE TRIGGER calendar_board_deleted AFTER DELETE ON calendar_events
    FOR EACH ROW WHEN (OLD.owner_id IS NULL) EXECUTE FUNCTION calendar_changed_notify();
CREATE TRIGGER calendar_rsvps_changed AFTER INSERT OR UPDATE OR DELETE ON calendar_rsvps
    FOR EACH ROW EXECUTE FUNCTION calendar_changed_notify();
