CREATE TABLE artboard_snapshots (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    updated TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    board_key VARCHAR NOT NULL UNIQUE,
    canvas JSONB NOT NULL
);
