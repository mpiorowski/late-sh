-- Which C-Mail conversations the user pinned into their rail, in their order.
-- Their API addresses a conversation by an opaque id and never by a name, so
-- each entry carries the other participant's username alongside it: the rail
-- row has to render before anything has been fetched, and an id is not a
-- label anyone recognizes.
--
-- Ids and usernames only, never their content. Unread counts and message
-- history stay on their side (their conversation list reports unreadCount,
-- which is why this needs no read cursor the way the cIRC rooms do).
ALTER TABLE cyberspace_accounts
    ADD COLUMN cmail_threads jsonb NOT NULL DEFAULT '[]'::jsonb;
