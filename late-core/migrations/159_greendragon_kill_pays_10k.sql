-- Green Dragon's dragon kill comes down from 20,000 to 10,000 (owner
-- decision 2026-08-27, SHOP.md Phase 6 table). The gate is unchanged: every
-- kill pays, keyed on the character row id and the kill number, because the
-- kill resets the character to level 1 and the climb back is the price.
-- Migration 158 set the 20,000; this is the forward correction, not an edit.
UPDATE reward_templates
SET reward_chips = 10000,
    updated = current_timestamp
WHERE key = 'greendragon_dragon_slain';
