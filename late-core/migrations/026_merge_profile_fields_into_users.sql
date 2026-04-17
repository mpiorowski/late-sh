-- Full cutover from profiles -> users.
--
-- Moves profile-backed fields onto users, normalizes usernames into the new
-- canonical format, enforces uniqueness/format on users.username, and then
-- drops the obsolete 1:1 profiles table.

WITH source_rows AS (
    SELECT
        u.id,
        COALESCE(u.settings, '{}'::jsonb) AS existing_settings,
        COALESCE(p.notify_kinds, '{}'::text[]) AS notify_kinds,
        COALESCE(p.notify_cooldown_mins, 0) AS notify_cooldown_mins,
        COALESCE(NULLIF(BTRIM(p.username), ''), NULLIF(BTRIM(u.username), ''), 'user') AS raw_username
    FROM users u
    LEFT JOIN profiles p ON p.user_id = u.id
),
sanitized AS (
    SELECT
        id,
        existing_settings,
        notify_kinds,
        notify_cooldown_mins,
        CASE
            WHEN cleaned = '' THEN 'user'
            ELSE cleaned
        END AS base_username
    FROM (
        SELECT
            id,
            existing_settings,
            notify_kinds,
            notify_cooldown_mins,
            REGEXP_REPLACE(
                REGEXP_REPLACE(
                    REGEXP_REPLACE(raw_username, '@', '', 'g'),
                    '[^A-Za-z0-9._-]+',
                    '_',
                    'g'
                ),
                '^_+|_+$',
                '',
                'g'
            ) AS cleaned
        FROM source_rows
    ) normalized
),
candidate_names AS (
    SELECT
        id,
        existing_settings,
        notify_kinds,
        notify_cooldown_mins,
        LEFT(base_username, 32) AS candidate_username
    FROM sanitized
),
resolved_names AS (
    SELECT
        id,
        existing_settings,
        notify_kinds,
        notify_cooldown_mins,
        CASE
            WHEN COUNT(*) OVER (PARTITION BY LOWER(candidate_username)) = 1
                THEN candidate_username
            ELSE LEFT(candidate_username, 19) || '_' || LEFT(REPLACE(id::text, '-', ''), 12)
        END AS final_username
    FROM candidate_names
)
UPDATE users u
SET username = r.final_username,
    settings = r.existing_settings || jsonb_build_object(
        'notify_kinds', to_jsonb(r.notify_kinds),
        'notify_cooldown_mins', GREATEST(r.notify_cooldown_mins, 0)
    ),
    updated = current_timestamp
FROM resolved_names r
WHERE r.id = u.id;

ALTER TABLE users
    ADD CONSTRAINT users_username_trimmed_chk
    CHECK (username = BTRIM(username));

ALTER TABLE users
    ADD CONSTRAINT users_username_length_chk
    CHECK (length(username) BETWEEN 1 AND 32);

ALTER TABLE users
    ADD CONSTRAINT users_username_format_chk
    CHECK (username ~ '^[A-Za-z0-9._-]+$' AND POSITION('@' IN username) = 0);

CREATE UNIQUE INDEX idx_users_username_lower ON users (LOWER(username));

DROP TABLE profiles;
