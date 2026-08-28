-- The `/summary` watermark: how far a reader's catch-up of a room has been
-- carried, so a bare `/summary` reads from the end of the last one.
--
-- This exists because `chat_room_members.last_read_at` cannot answer the
-- question. That cursor records presence: a terminal parked on a visible room
-- marks every arriving message read, so it says "caught up" for someone who
-- was asleep in front of it. Every window derived from it had to be widened
-- by a fixed floor to stop `/summary` answering "nothing new" to exactly the
-- person who missed the day, and that floor then re-summarized hours the
-- reader had already been given.
--
-- What this table records instead is not a guess about attention at all. The
-- reader typed `/summary`, and the summary they were handed covered messages
-- up to `summarized_through`. Whether they read it is unknowable and beside
-- the point: it is what they were told, which is the only fact in this area
-- the server actually holds.
--
-- `summarized_through` is the `created` of the newest message the transcript
-- contained, never `now()`. A message that lands while the model call is in
-- flight would fall between a `now()` watermark and the next window's floor
-- and be summarized by nobody. Keyed off a real message instead, consecutive
-- windows abut exactly: every message is covered by at most one summary, and
-- by at least one for as long as the reader keeps asking.
--
-- The row is per (reader, room), not per session: what you were told on your
-- phone you were told, and your desktop should not repeat it.
CREATE TABLE chat_summary_reads (
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    room_id UUID NOT NULL REFERENCES chat_rooms(id) ON DELETE CASCADE,
    summarized_through TIMESTAMPTZ NOT NULL,
    updated TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    PRIMARY KEY (user_id, room_id)
);
