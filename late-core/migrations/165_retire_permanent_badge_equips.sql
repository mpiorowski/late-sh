-- Badges and flags become rentals only: the permanent equips migration 148
-- left standing are cleared, and every owner is handed 30 days of the same
-- emoji as a rental so nobody goes bare overnight.
--
-- 148 retired the permanent SKUs but kept rendering them through a COALESCE
-- fallback in the chat label query, which left two problems. An owner could
-- never be shown as active in the Shop, because the SKU is gone from the
-- catalog and no listed row could carry the marker. And renting a flag only
-- masked the permanent one, which came back at expiry, so a permanent owner
-- could never end up wearing nothing. Both go away with the equip.
--
-- The goodbye is a real rental, not a special case: `source_sku` is the month
-- SKU selling the same emoji and the payload is that item's own, so the Shop
-- shows it active with its remaining time exactly as a bought one does, and
-- it lapses on its own with no new code and no background task.
--
-- Order matters: grant first, clear second. The grant reads `equipped_slot`.
--
-- No notify is fired here. `shop_user_changed` is written by the purchase
-- path in app code, and a migration runs at deploy, before the sessions that
-- would listen for it exist.

INSERT INTO shop_consumable_effects
    (user_id, room_id, effect_kind, source_sku, payload, ends_at)
SELECT DISTINCT ON (up.user_id, legacy.slot)
    up.user_id,
    NULL::uuid,
    legacy.slot,
    rental.sku,
    rental.payload,
    current_timestamp + make_interval(secs => (rental.payload->>'duration_secs')::int)
FROM user_purchases up
JOIN marketplace_items legacy
  ON legacy.id = up.item_id
 AND legacy.item_kind = 'badge'
 AND legacy.slot IN ('chat_badge', 'chat_flag')
JOIN marketplace_items rental
  ON rental.sku = legacy.sku || '_month'
 AND rental.item_kind = 'badge_rental'
WHERE up.equipped_slot = legacy.slot
  -- One live row per (user, effect_kind) is the invariant every rental write
  -- upholds (`ShopConsumableEffect::activate_user_effect_in_tx`). Someone
  -- already renting over their permanent badge has moved on: leave their
  -- clock alone rather than stack a second live row over it. They lose the
  -- permanent fallback at their own expiry, which is the end state anyway.
  AND NOT EXISTS (
    SELECT 1
    FROM shop_consumable_effects e
    WHERE e.user_id = up.user_id
      AND e.effect_kind = legacy.slot
      AND e.room_id IS NULL
      AND e.active = true
      AND e.ends_at > current_timestamp
  )
-- DISTINCT ON needs a total order to be deterministic, and a second equipped
-- row in one slot would insert a second live effect that no later write could
-- ever clean up.
ORDER BY up.user_id, legacy.slot, up.created DESC;

UPDATE user_purchases
SET equipped_slot = NULL,
    updated = current_timestamp
WHERE equipped_slot IN ('chat_badge', 'chat_flag');
