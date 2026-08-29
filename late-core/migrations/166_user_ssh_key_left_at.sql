-- When this device last left the app. Written as a session ends, stamped at
-- the moment its keyboard went quiet rather than the moment the connection
-- closed: a terminal parked open overnight and shut in the morning left last
-- night, which is when its reader stopped attending.
--
-- This is per device (per key), not per account, because "did the person at
-- this terminal step away" is a fact about this terminal. It seeds the next
-- session's AFK line on that device, so the `new messages` divider and the
-- `/summary` catch-up both rest on where this reader actually stopped, and a
-- phone opening a room says nothing about where the desktop's line sits.
--
-- NULL means the key has never ended a session with the app: the session
-- starts with no line, and a bare `/summary` falls back to its default window.
ALTER TABLE user_ssh_keys
    ADD COLUMN left_at TIMESTAMPTZ;
