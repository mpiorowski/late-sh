UPDATE marketplace_items
SET payload = jsonb_set(payload, '{duration_secs}', '43200'::jsonb),
    description = 'Bump the current room in the room list for twelve hours.',
    updated = current_timestamp
WHERE sku = 'chat_room_bump';
