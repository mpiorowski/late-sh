-- Hack Room (`chat_pinned_vibe`) marked the current room as `hacking` in every
-- viewer's room rail for an hour. It never sold, and it was the one effect
-- allowed to restyle real room-list rows, a special case the rail no longer
-- carries. Retire the item and kill any live effect rows, the shape of 104.
--
-- The row is deactivated rather than deleted so `user_purchases` and
-- `shop_consumable_effects.source_sku` keep their foreign keys and history.

UPDATE shop_consumable_effects
SET active = false,
    updated = current_timestamp
WHERE effect_kind = 'pinned_vibe'
  AND active = true;

UPDATE marketplace_items
SET active = false,
    updated = current_timestamp
WHERE sku = 'chat_pinned_vibe';

-- Room Bump leads the Chat tab's consumables. 071 listed it last (4070); the
-- rest of the block starts at 4020 (4000 and 4010 are retired rows).
UPDATE marketplace_items
SET sort_order = 4005,
    updated = current_timestamp
WHERE sku = 'chat_room_bump';
