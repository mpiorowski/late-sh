UPDATE marketplace_items
SET
    description = 'Trigger a room-wide sparkle effect for one minute once per day.',
    payload = jsonb_set(payload, '{duration_secs}', '60'::jsonb, true),
    updated = current_timestamp
WHERE sku = 'chat_room_spark';

UPDATE marketplace_items
SET
    description = 'Add a room-wide glow effect for one minute once per day.',
    payload = jsonb_set(payload, '{duration_secs}', '60'::jsonb, true),
    updated = current_timestamp
WHERE sku = 'chat_room_glow';

UPDATE marketplace_items
SET
    description = 'Send a room-wide pulse effect for one minute once per day.',
    payload = jsonb_set(payload, '{duration_secs}', '60'::jsonb, true),
    updated = current_timestamp
WHERE sku = 'chat_room_pulse';
