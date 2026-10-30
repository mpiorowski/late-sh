-- Watcher chat beside a player's own door game is no longer a profile
-- setting: F4 shows or hides it in game, for the session (late-ssh
-- app/door/spectate). Nothing reads the old key, so it goes.
UPDATE users
SET settings = settings - 'show_watch_chat',
    updated = current_timestamp
WHERE settings ? 'show_watch_chat';
