-- Process-wide switches as rows, not memory (root CONTEXT.md, multi-replica
-- rule). A switch flipped on one replica must flip on every replica and
-- survive a restart; an in-memory AtomicBool does neither.
--
-- One row per switch, keyed by a name the code maps from a closed enum
-- (late-core `models/app_flag.rs`). Every switch the code knows must have a
-- row: a missing row is a load error, never a default, so nobody can ship a
-- switch whose off-state is an accident of the seed.
CREATE TABLE app_flags (
    key TEXT PRIMARY KEY,
    enabled BOOLEAN NOT NULL,
    updated TIMESTAMPTZ NOT NULL DEFAULT current_timestamp
);

-- Every replica LISTENs here and re-reads the whole table on any change:
-- the payload is the key for logging, not something a listener trusts.
CREATE OR REPLACE FUNCTION notify_app_flag_changed() RETURNS trigger AS $$
BEGIN
    PERFORM pg_notify('app_flag_changed', NEW.key);
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER app_flags_changed
    AFTER INSERT OR UPDATE ON app_flags
    FOR EACH ROW EXECUTE FUNCTION notify_app_flag_changed();

-- First contact (GAME.md): the kill switch and the fuse. The kill switch
-- starts on as it always did in memory; the fuse starts unlit, so stage 1
-- keeps firing for admins only until a deliberate `/haunt live on`.
INSERT INTO app_flags (key, enabled) VALUES
    ('haunt_enabled', true),
    ('haunt_live', false);
