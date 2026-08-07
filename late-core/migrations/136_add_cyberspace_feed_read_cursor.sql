-- How far the user's cyberspace feed reading has got. Unread entries are the
-- ones published after this stamp; NULL means they have never opened the pane,
-- which counts as nothing unread rather than as a whole page of it.
--
-- The cursor is ours, not their content: it is a timestamp of when this user
-- looked, so it stays clear of the caching their API terms forbid.
ALTER TABLE cyberspace_accounts
ADD COLUMN feed_read_at TIMESTAMPTZ;
