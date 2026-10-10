CREATE TABLE calendar_preferences (
    user_id UUID PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    week_start SMALLINT NOT NULL DEFAULT 0 CHECK (week_start IN (0, 6)),
    default_view TEXT NOT NULL DEFAULT 'month' CHECK (default_view IN ('month','week','three_day','day','list')),
    server_overlay BOOLEAN NOT NULL DEFAULT true,
    public BOOLEAN NOT NULL DEFAULT false,
    revision BIGINT NOT NULL DEFAULT 1
);
CREATE INDEX calendar_public_owners ON calendar_preferences(user_id) WHERE public;
CREATE TABLE calendar_events (
    id UUID PRIMARY KEY,
    owner_id UUID REFERENCES users(id) ON DELETE CASCADE,
    -- Immutable provenance survives account deletion; personal ownership cascades.
    creator_id UUID NOT NULL,
    creation_tier TEXT NOT NULL CHECK (creation_tier IN ('user','moderator','admin')),
    mod_editable BOOLEAN NOT NULL DEFAULT false,
    title TEXT NOT NULL CHECK (length(btrim(title)) BETWEEN 1 AND 300),
    description TEXT NOT NULL DEFAULT '' CHECK (length(description) <= 10000),
    start_date DATE,
    end_date DATE,
    start_at TIMESTAMPTZ,
    end_at TIMESTAMPTZ,
    creator_timezone TEXT NOT NULL,
    notice_lead_seconds BIGINT CHECK (notice_lead_seconds BETWEEN 0 AND 315360000),
    notice_start TIMESTAMPTZ NOT NULL,
    notice_end TIMESTAMPTZ NOT NULL,
    revision BIGINT NOT NULL DEFAULT 1,
    CHECK ((start_date IS NOT NULL AND end_date IS NOT NULL AND end_date > start_date AND start_at IS NULL AND end_at IS NULL)
        OR (start_date IS NULL AND end_date IS NULL AND start_at IS NOT NULL AND (end_at IS NULL OR end_at > start_at))),
    CHECK (notice_end > notice_start),
    CHECK (NOT mod_editable OR (owner_id IS NULL AND creation_tier = 'admin'))
);
CREATE INDEX calendar_event_owners ON calendar_events(owner_id);
CREATE INDEX calendar_all_day_range ON calendar_events USING gist (daterange(start_date, end_date, '[)')) WHERE start_date IS NOT NULL;
CREATE INDEX calendar_timed_range ON calendar_events USING gist (tstzrange(start_at, notice_end, '[)')) WHERE start_at IS NOT NULL;
CREATE INDEX calendar_notice_range ON calendar_events USING gist (tstzrange(notice_start, notice_end, '[)')) WHERE notice_lead_seconds IS NOT NULL;
CREATE FUNCTION calendar_changed_notify() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    PERFORM pg_notify('calendar_changed', '');
    RETURN NULL;
END;
$$;
CREATE TRIGGER calendar_events_changed AFTER INSERT OR UPDATE OR DELETE ON calendar_events FOR EACH STATEMENT EXECUTE FUNCTION calendar_changed_notify();
CREATE TRIGGER calendar_preferences_changed AFTER INSERT OR UPDATE OR DELETE ON calendar_preferences FOR EACH STATEMENT EXECUTE FUNCTION calendar_changed_notify();
