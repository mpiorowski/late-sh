-- Every time a member opens a door from the Games hub (late-ssh
-- `app/door/hub`): one row per Enter on a card that launches, written
-- after the card's "currently unavailable" gate and before the connect, so
-- a launch the host then refuses still counts as a door opened and a card
-- that only shows a banner does not. `card` is the hub's closed roster key
-- (`late_core::models::door_opening::DoorCard`). Read by the paper's
-- YESTERDAY column (the covered day's doors opened, per card).
CREATE TABLE door_openings (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    card TEXT NOT NULL,
    opened_at TIMESTAMPTZ NOT NULL DEFAULT current_timestamp
);

-- The paper's read: a day's rows, per card.
CREATE INDEX door_openings_opened_at_idx ON door_openings (opened_at, card);
