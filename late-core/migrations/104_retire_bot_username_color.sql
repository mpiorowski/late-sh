-- Bot Username Color charged 1000 chips to recolor bot/graybeard/dealer names,
-- and only inside the buyer's own client: it decorated somebody else's name,
-- and nobody but the buyer could see it. Retire the item and kill any live
-- effect rows. The username label already carries the bartender drink tint;
-- it does not need a second decoration.
--
-- The row is deactivated rather than deleted so `user_purchases` and
-- `shop_consumable_effects.source_sku` keep their foreign keys and history.

UPDATE shop_consumable_effects
SET active = false,
    updated = current_timestamp
WHERE effect_kind = 'bot_username_color'
  AND active = true;

UPDATE marketplace_items
SET active = false,
    updated = current_timestamp
WHERE sku = 'chat_bot_username_color_day';

-- It was the only user-scoped effect, so every chat consumable is now
-- room-targeted and no query filters `shop_consumable_effects` by user. Drop
-- the partial index that served those lookups; it is pure write cost now.
DROP INDEX IF EXISTS shop_consumable_effects_active_user_idx;
