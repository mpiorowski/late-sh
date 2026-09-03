-- The Late Edition (late-ssh `app/paper`): @graybeard's daily paper.
--
-- One edition per UTC day, dated the day it comes out and covering the day
-- before. It is printed once per public room and once per extra section by
-- whichever replica claims the row first, then read by every login after
-- that (root CONTEXT.md, multi-replica rule): the paid model call is spent
-- per room per day, never per reader.
--
-- A row is the claim. `printing` is held while the call runs (a stale one,
-- older than the sweeper's reclaim window, is taken over rather than
-- trusted); `ready` carries the text; `quiet` records that the room was
-- looked at and fell under the message threshold, so the next sweep does not
-- count it again. A failed print deletes its row, so the next sweep retries.
CREATE TABLE paper_room_editions (
    room_id UUID NOT NULL REFERENCES chat_rooms(id) ON DELETE CASCADE,
    edition DATE NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('printing', 'ready', 'quiet')),
    message_count INTEGER NOT NULL,
    author_count INTEGER NOT NULL,
    text TEXT,
    claimed_at TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    generated_at TIMESTAMPTZ,
    PRIMARY KEY (room_id, edition),
    CHECK ((status = 'ready') = (text IS NOT NULL)),
    CHECK ((status = 'printing') = (generated_at IS NULL))
);

CREATE INDEX paper_room_editions_edition_idx ON paper_room_editions (edition);

-- Edition-level sections, same claim shape: `reading` is what the clubhouse
-- shared into News that day, `outside` is the grounded look at the world
-- (behind the `paper_outside_enabled` switch below).
CREATE TABLE paper_sections (
    edition DATE NOT NULL,
    section TEXT NOT NULL CHECK (section IN ('reading', 'outside')),
    status TEXT NOT NULL CHECK (status IN ('printing', 'ready', 'quiet')),
    text TEXT,
    claimed_at TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    generated_at TIMESTAMPTZ,
    PRIMARY KEY (edition, section),
    CHECK ((status = 'ready') = (text IS NOT NULL)),
    CHECK ((status = 'printing') = (generated_at IS NULL))
);

-- Both switches start on: the presses run, and the outside-world section
-- prints from the first edition (`/paper outside off` drops it if it reads
-- like slop).
INSERT INTO app_flags (key, enabled) VALUES
    ('paper_enabled', true),
    ('paper_outside_enabled', true);
