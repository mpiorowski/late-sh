-- The Aquarium for everyone: nobody buys the tank any more, a user's first
-- connect plants it (marketplace::welcome_aquarium_tank_in_tx), with the same
-- welcome fry and first sprout a purchase used to bring. A tank that already
-- has its care row, which is every tank bought so far (migration 180), keeps
-- what it has. Fish, plants and the shield stay on sale.

-- The unlock is retired, never deleted: `user_purchases` keeps the history of
-- who bought it.
UPDATE marketplace_items
SET active = false,
    updated = current_timestamp
WHERE sku = 'aquarium';
