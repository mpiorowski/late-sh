-- Make #bugs a permanent room (auto-join, cannot leave).
-- If the room already exists, promote it to permanent.
-- If it doesn't exist, create it.

INSERT INTO chat_rooms (kind, visibility, auto_join, permanent, slug)
VALUES ('topic', 'public', true, true, 'bugs')
ON CONFLICT (slug) WHERE kind = 'topic'
DO UPDATE SET auto_join = true, permanent = true, updated = current_timestamp;

-- Add all existing users to #bugs.
INSERT INTO chat_room_members (room_id, user_id)
SELECT r.id, u.id
FROM chat_rooms r
CROSS JOIN users u
WHERE r.slug = 'bugs' AND r.kind = 'topic'
ON CONFLICT (room_id, user_id) DO NOTHING;
