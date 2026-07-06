-- Green Dragon commentary: the one shared chat primitive (LoGD
-- lib/commentary.php). One table serves every talk room — the village
-- square, the inn, the Dark Horse board, the shade channel, the gardens,
-- and the veterans' rock are all just `section` values, and later features
-- (clan halls) land as new sections, not new tables. `user_id` is NULL for
-- system lines; `name` snapshots the speaker's character name at post time
-- so reads never join the character blobs. Non-"says" venues bake their
-- talk verb into the body as an emote at post time, exactly like upstream.
-- Retention mirrors upstream's `expirecontent` default (180 days), pruned
-- opportunistically on write.
CREATE TABLE greendragon_commentary (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    updated TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    section TEXT NOT NULL,
    user_id UUID REFERENCES users(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    body TEXT NOT NULL
);

CREATE INDEX greendragon_commentary_section_idx
    ON greendragon_commentary (section, created DESC, id DESC);
