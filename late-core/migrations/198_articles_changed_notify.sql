-- Cross-session news refresh goes through LISTEN/NOTIFY (root CONTEXT.md,
-- multi-replica rule). Every replica LISTENs on `articles_changed` and
-- re-reads its newest articles into the shared news snapshot; each session
-- counts its own unread badge from that snapshot. Statement level with an
-- empty payload: the listener re-reads, it never trusts a payload.
CREATE OR REPLACE FUNCTION notify_articles_changed() RETURNS trigger AS $$
BEGIN
    PERFORM pg_notify('articles_changed', '');
    RETURN NULL;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER articles_changed
    AFTER INSERT OR UPDATE OR DELETE ON articles
    FOR EACH STATEMENT EXECUTE FUNCTION notify_articles_changed();
