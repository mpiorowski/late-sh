-- Chip balance changes must always notify 'chip_user_changed': per-session
-- chip counters refresh from that channel (ShopService listener). Hand-wired
-- pg_notify calls in application SQL proved forgettable (the /gift CTEs were
-- never executed because nothing referenced them), so the table itself now
-- owns the notify. Two triggers because an INSERT trigger's WHEN clause
-- cannot reference OLD.
CREATE FUNCTION notify_chip_user_changed() RETURNS trigger AS $$
BEGIN
    PERFORM pg_notify('chip_user_changed', NEW.user_id::text);
    RETURN NULL;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER user_chips_notify_insert
    AFTER INSERT ON user_chips
    FOR EACH ROW
    EXECUTE FUNCTION notify_chip_user_changed();

CREATE TRIGGER user_chips_notify_update
    AFTER UPDATE ON user_chips
    FOR EACH ROW
    WHEN (OLD.balance IS DISTINCT FROM NEW.balance)
    EXECUTE FUNCTION notify_chip_user_changed();
