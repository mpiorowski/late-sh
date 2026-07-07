ALTER TABLE cat_companions
    ADD COLUMN IF NOT EXISTS adopted_at TIMESTAMPTZ;

INSERT INTO cat_companions (user_id, adopted_at)
SELECT p.user_id, p.created
FROM user_purchases p
JOIN marketplace_items i ON i.id = p.item_id
WHERE i.sku = 'cat_companion'
ON CONFLICT (user_id) DO UPDATE SET
    adopted_at = COALESCE(cat_companions.adopted_at, EXCLUDED.adopted_at),
    updated = current_timestamp;
