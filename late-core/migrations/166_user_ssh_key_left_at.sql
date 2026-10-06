-- When this device last left the app. Written as a session ends, stamped at
-- the moment its keyboard went quiet rather than the moment the connection
-- closed: a terminal parked open overnight and shut in the morning left last
-- night, which is when its reader stopped attending.
--
-- This is per device (per key), not per account, because "when was I last
-- here" is a question about this terminal. It is the mark a bare `/summary`
-- reads from on the next session on this device, and only that: the
-- in-session `new messages` divider (the AFK line) is a separate, never
-- persisted mark, and a phone leaving says nothing about the desktop.
--
-- NULL means the key has never ended a session with the app, or the mark was
-- taken by a session already: a bare `/summary` falls back to its default
-- window.
ALTER TABLE user_ssh_keys
    ADD COLUMN left_at TIMESTAMPTZ;
