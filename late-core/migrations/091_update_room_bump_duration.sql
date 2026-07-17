UPDATE marketplace_items
SET payload = jsonb_set(payload, '{duration_secs}', '21600'::jsonb),
    description = 'Bump the current room in the room list for six hours.',
    updated = current_timestamp
WHERE sku = 'chat_room_bump';
