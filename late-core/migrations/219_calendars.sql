-- The house events board and private personal calendars.
--
-- An event with owner_id NULL is on the board: anyone not banned may post
-- one, everyone sees it, anyone may say they are in. An event with an owner
-- is that account's own and nobody else ever reads it.
CREATE TABLE calendar_events (
    id UUID PRIMARY KEY,
    created TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    owner_id UUID REFERENCES users(id) ON DELETE CASCADE,
    -- Who posted it. A board post outlives its poster's account; a personal
    -- event cascades with its owner.
    creator_id UUID NOT NULL,
    title TEXT NOT NULL CHECK (length(btrim(title)) BETWEEN 1 AND 300),
    description TEXT NOT NULL DEFAULT '' CHECK (length(description) <= 10000),
    start_date DATE,
    end_date DATE,
    start_at TIMESTAMPTZ,
    end_at TIMESTAMPTZ,
    -- The zone an all-day event's civil dates are read in, captured at
    -- creation so a later account change does not move the event.
    creator_timezone TEXT NOT NULL,
    -- The event as instants, derived from the timing above in creator_timezone:
    -- what the upcoming queries, the live strip and the start announcement read.
    starts_at TIMESTAMPTZ NOT NULL,
    ends_at TIMESTAMPTZ NOT NULL,
    -- The start announcement's claim: the sweeper that stamps it posts the
    -- #lounge line, so one replica announces however many are sweeping.
    announced_at TIMESTAMPTZ,
    revision BIGINT NOT NULL DEFAULT 1,
    CHECK ((start_date IS NOT NULL AND end_date IS NOT NULL AND end_date > start_date AND start_at IS NULL AND end_at IS NULL)
        OR (start_date IS NULL AND end_date IS NULL AND start_at IS NOT NULL AND (end_at IS NULL OR end_at > start_at))),
    CHECK (ends_at > starts_at),
    CHECK (announced_at IS NULL OR owner_id IS NULL)
);
CREATE INDEX calendar_event_owners ON calendar_events(owner_id);
CREATE INDEX calendar_event_posts_per_day ON calendar_events(creator_id, created) WHERE owner_id IS NULL;
CREATE INDEX calendar_all_day_range ON calendar_events USING gist (daterange(start_date, end_date, '[)')) WHERE start_date IS NOT NULL;
CREATE INDEX calendar_instant_range ON calendar_events USING gist (tstzrange(starts_at, ends_at, '[)'));
CREATE INDEX calendar_unannounced ON calendar_events(starts_at) WHERE owner_id IS NULL AND announced_at IS NULL;

-- "I'm in" on a board event. One row per person per event; the count is the
-- story the board ships.
CREATE TABLE calendar_rsvps (
    event_id UUID NOT NULL REFERENCES calendar_events(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (event_id, user_id)
);
CREATE INDEX calendar_rsvp_users ON calendar_rsvps(user_id);

-- Posting to the board can be taken away. Shaped like artboard_bans; a ban
-- never touches personal events.
CREATE TABLE calendar_bans (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    created TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    target_user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    actor_user_id UUID NOT NULL,
    reason TEXT NOT NULL DEFAULT '',
    expires_at TIMESTAMPTZ,
    UNIQUE (target_user_id)
);
CREATE INDEX calendar_bans_expires_at ON calendar_bans(expires_at);

CREATE FUNCTION calendar_changed_notify() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    PERFORM pg_notify('calendar_changed', '');
    RETURN NULL;
END;
$$;
CREATE TRIGGER calendar_events_changed AFTER INSERT OR UPDATE OR DELETE ON calendar_events FOR EACH STATEMENT EXECUTE FUNCTION calendar_changed_notify();
CREATE TRIGGER calendar_rsvps_changed AFTER INSERT OR UPDATE OR DELETE ON calendar_rsvps FOR EACH STATEMENT EXECUTE FUNCTION calendar_changed_notify();
