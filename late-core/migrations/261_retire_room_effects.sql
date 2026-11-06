-- Room Spark, Room Glow, and Room Pulse painted a minute of sparkles, a
-- glow, or a pulse over the selected room's messages. Retire all three and
-- kill any live effect rows, the shape of 152. Room Bump stays on sale.
--
-- The rows are deactivated rather than deleted so `user_purchases` and
-- `shop_consumable_effects.source_sku` keep their foreign keys and history.
-- They were the only items carrying `daily_limit`, which the purchase path
-- no longer reads.

UPDATE shop_consumable_effects
SET active = false,
    updated = current_timestamp
WHERE effect_kind IN ('room_spark', 'room_glow', 'room_pulse')
  AND active = true;

UPDATE marketplace_items
SET active = false,
    updated = current_timestamp
WHERE sku IN ('chat_room_spark', 'chat_room_glow', 'chat_room_pulse');
