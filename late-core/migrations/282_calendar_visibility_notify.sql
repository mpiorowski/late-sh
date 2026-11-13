-- A board event becoming personal must disappear from every viewer's cache,
-- including when nobody has RSVP'd and no RSVP deletion sends a notification.
DROP TRIGGER calendar_board_updated ON calendar_events;
CREATE TRIGGER calendar_board_updated AFTER UPDATE ON calendar_events
    FOR EACH ROW WHEN (OLD.owner_id IS NULL OR NEW.owner_id IS NULL)
    EXECUTE FUNCTION calendar_changed_notify();
