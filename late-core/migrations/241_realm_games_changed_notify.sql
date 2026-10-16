-- Cross-replica realm refresh goes through LISTEN/NOTIFY (root CONTEXT.md,
-- multi-replica rule), the daily_matches shape (migrations 211/212). Every
-- replica LISTENs on `realm_game_changed` and re-reads its Lobby snapshot;
-- each one derives "this game changed / its day rolled / it finished / it is
-- forming" by diffing the snapshot it had against the one it just read, so an
-- action, a join, a sweep or a finish written on one replica reaches every
-- open board and every banner on every other one at once instead of on the
-- next 60s sweep. Row level, so a sweeper's guarded UPDATE that matches
-- nothing fires nothing; empty payload, because the listener re-reads and
-- never trusts a payload.
CREATE OR REPLACE FUNCTION notify_realm_game_changed() RETURNS trigger AS $$
BEGIN
    PERFORM pg_notify('realm_game_changed', '');
    RETURN NULL;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER realm_games_changed
    AFTER INSERT OR UPDATE OR DELETE ON realm_games
    FOR EACH ROW EXECUTE FUNCTION notify_realm_game_changed();
