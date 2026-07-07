-- Make #announcements a permanent room (auto-join, cannot leave).
-- If the public topic room already exists, promote it without changing its id.
-- If it doesn't exist, create it.

INSERT INTO chat_rooms (kind, visibility, auto_join, permanent, slug)
VALUES ('topic', 'public', true, true, 'announcements')
ON CONFLICT (visibility, slug) WHERE kind = 'topic'
DO UPDATE SET auto_join = true, permanent = true, updated = current_timestamp;

-- Add all existing users to #announcements.
INSERT INTO chat_room_members (room_id, user_id)
SELECT r.id, u.id
FROM chat_rooms r
CROSS JOIN users u
WHERE r.slug = 'announcements'
  AND r.kind = 'topic'
  AND r.visibility = 'public'
ON CONFLICT (room_id, user_id) DO NOTHING;
