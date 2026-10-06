-- Green Dragon clans (LoGD clan.php + lib/clan/*): one row per clan.
-- Membership lives on the character blobs (clan_id / clan_rank /
-- clan_joined_at), exactly as upstream keeps it on `accounts` — this table
-- only holds the clan itself. `motd_author` / `desc_author` snapshot the
-- editor's character name at write time (upstream stores the account id and
-- joins; our snapshot sidesteps decoding a blob per render, and upstream's
-- own mail-cleanup already breaks on renames). A clan whose last real
-- member (rank >= 10) leaves is deleted — lazily at list render, exactly
-- like upstream's list sweeps.
CREATE TABLE greendragon_clans (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    updated TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    name TEXT NOT NULL,
    tag TEXT NOT NULL,
    motd TEXT NOT NULL DEFAULT '',
    motd_author TEXT NOT NULL DEFAULT '',
    description TEXT NOT NULL DEFAULT '',
    desc_author TEXT NOT NULL DEFAULT '',
    custom_verb TEXT NOT NULL DEFAULT ''
);

-- Uniqueness is case-insensitive (upstream's MySQL collation compares
-- clanname/clanshort case-insensitively).
CREATE UNIQUE INDEX greendragon_clans_name_idx ON greendragon_clans (lower(name));
CREATE UNIQUE INDEX greendragon_clans_tag_idx ON greendragon_clans (lower(tag));
