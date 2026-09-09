-- One Bonsai for everyone (owner decision 2026-09-08). The living branch
-- graph that shipped as the "Dynamic Bonsai" shop unlock is now the only
-- bonsai, and every tree starts over from a fresh seed: no migration of the
-- old growth points into the graph, no carried-over chat badge, no second
-- tree kept alive next to the first. The classic stage ladder, its daily
-- care rows, and the graveyard go with it.

-- Classic bonsai: gone, data included. The chip ledger keeps every watering
-- bonus it ever paid (source_kind 'bonsai_daily_care' on the old rows), so
-- nothing about balances or Top Chips moves.
DROP TABLE bonsai_daily_care;
DROP TABLE bonsai_graveyard;
DROP TABLE bonsai_trees;

-- The branch-graph table takes the plain name.
ALTER TABLE bonsai_v2_trees RENAME TO bonsai_trees;
ALTER INDEX idx_bonsai_v2_trees_user_updated RENAME TO idx_bonsai_trees_user_updated;
ALTER TABLE bonsai_trees RENAME CONSTRAINT bonsai_v2_trees_pkey TO bonsai_trees_pkey;
ALTER TABLE bonsai_trees RENAME CONSTRAINT bonsai_v2_trees_user_id_key TO bonsai_trees_user_id_key;
ALTER TABLE bonsai_trees RENAME CONSTRAINT bonsai_v2_trees_user_id_fkey TO bonsai_trees_user_id_fkey;

-- Everyone resets: the next login plants a fresh tree.
DELETE FROM bonsai_trees;

-- The shop unlock is retired, never deleted: `user_purchases` keeps the
-- history of who bought it. Its slot is cleared on every purchase row so
-- `equipped_slot` carries nothing anywhere any more (badges and flags went
-- rental in migration 148, and this was the last equip).
UPDATE user_purchases p
SET equipped_slot = NULL,
    updated = current_timestamp
FROM marketplace_items i
WHERE i.id = p.item_id
  AND i.sku = 'dynamic_bonsai'
  AND p.equipped_slot IS NOT NULL;

UPDATE marketplace_items
SET active = false,
    updated = current_timestamp
WHERE sku = 'dynamic_bonsai';
