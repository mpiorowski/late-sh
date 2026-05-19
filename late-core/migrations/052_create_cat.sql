CREATE TABLE cat_companions (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    updated TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE UNIQUE,
    last_fed TIMESTAMPTZ,
    last_watered TIMESTAMPTZ,
    last_played TIMESTAMPTZ,
    last_groomed TIMESTAMPTZ,
    last_treated TIMESTAMPTZ
);
