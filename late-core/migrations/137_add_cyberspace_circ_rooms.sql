-- Which cIRC rooms the user pinned into their rail, in their order. Their API
-- has no join/leave: the roster is whatever it hands out, so this list is our
-- own bookmark of which of those rooms this user wants a rail row for.
--
-- Slugs only, never their content: room names, message history, and per-room
-- read state all live on their side, which is what keeps this clear of the
-- caching their API terms forbid.
ALTER TABLE cyberspace_accounts
ADD COLUMN circ_rooms TEXT[] NOT NULL DEFAULT '{}';
