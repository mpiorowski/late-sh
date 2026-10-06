-- The runner: a user's character in the deadchannel game (GAME.md, Phase 2
-- design pass). One row per user, created by the consented
-- `/join #deadchannel`; nothing else ever creates one. Phase 2 grows this
-- row column by column (level, signal, rations, gear); v1 holds the look.
--
-- `look` is the avatar as piece codes and tints (GAME.md, "The data
-- model"): art lives in code, the row only says which pieces are worn.
-- Parsed into a typed struct at load and rejected loudly on an unknown
-- code, so the column carries no schema of its own here.
CREATE TABLE deadchannel_runners (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    updated TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    user_id UUID NOT NULL UNIQUE REFERENCES users(id) ON DELETE CASCADE,
    look JSONB NOT NULL
);

-- Every replica LISTENs here and re-reads every runner's look on any
-- change (root CONTEXT.md, multi-replica rule): a look worn on one replica
-- must paint on every replica, and the runner directory is that read. The
-- payload is the user id for logging, not something a listener trusts.
CREATE OR REPLACE FUNCTION notify_deadchannel_runner_changed() RETURNS trigger AS $$
BEGIN
    PERFORM pg_notify('deadchannel_runner_changed', NEW.user_id::text);
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER deadchannel_runners_changed
    AFTER INSERT OR UPDATE ON deadchannel_runners
    FOR EACH ROW EXECUTE FUNCTION notify_deadchannel_runner_changed();
