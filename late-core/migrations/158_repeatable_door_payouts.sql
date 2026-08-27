-- Phase 6 of SHOP.md: every door milestone pays again. A NetHack ascension is
-- the same 20+ hours the second time, and a Green Dragon kill or an A Dark
-- Room escape is a full run every time, so a repeat pays the full amount. The
-- gate is whatever naturally limits the game; only where nothing does is a
-- lockout added. The numbers live in SHOP.md's Phase 6 table, not here.
--
-- Two gate shapes appear below:
--   'cooldown'  the credit path pairs a per-run (or per-character) uniqueness
--               claim with a 7-day per-account lockout, both in one
--               all-or-nothing grant (GamePayout::grant_multi).
--   'per_event' the game itself is the gate: a Green Dragon kill resets the
--               character, an A Dark Room ending wipes the save.
--
-- Claims already banked stay exactly where they are. They carry
-- period_kind = 'lifetime', which none of the new gates read, so the first
-- repeat under this migration pays even to an account that claimed years ago.

-- Roguelikes: run identity plus a 7-day lockout, each milestone on its own.
UPDATE reward_templates
SET reward_chips = 20000,
    claim_policy = 'cooldown',
    cooldown_seconds = 604800,
    description = 'Reach the bottom of the dungeon and claim the Amulet of Yendor in NetHack. Pays once per run, and again 7 days after the last time it paid.',
    updated = current_timestamp
WHERE key = 'nethack_amulet';

UPDATE reward_templates
SET reward_chips = 50000,
    claim_policy = 'cooldown',
    cooldown_seconds = 604800,
    description = 'Carry the Amulet of Yendor up through Gehennom and the planes, then ascend in NetHack. Pays once per run, and again 7 days after the last time it paid.',
    updated = current_timestamp
WHERE key = 'nethack_ascension';

UPDATE reward_templates
SET reward_chips = 20000,
    claim_policy = 'cooldown',
    cooldown_seconds = 604800,
    description = 'Descend through the Realm of Zot and pick up the Orb in Dungeon Crawl Stone Soup. Pays once per run, and again 7 days after the last time it paid.',
    updated = current_timestamp
WHERE key = 'dcss_orb';

UPDATE reward_templates
SET reward_chips = 50000,
    claim_policy = 'cooldown',
    cooldown_seconds = 604800,
    description = 'Carry the Orb of Zot back up and out of the dungeon in Dungeon Crawl Stone Soup. Pays once per run, and again 7 days after the last time it paid.',
    updated = current_timestamp
WHERE key = 'dcss_win';

UPDATE reward_templates
SET reward_chips = 20000,
    claim_policy = 'cooldown',
    cooldown_seconds = 604800,
    description = 'Grab the Amulet of Yendor from depth 26 and climb back out of Brogue alive. Pays once per run, and again 7 days after the last time it paid.',
    updated = current_timestamp
WHERE key = 'brogue_escape';

UPDATE reward_templates
SET reward_chips = 50000,
    claim_policy = 'cooldown',
    cooldown_seconds = 604800,
    description = 'Carry the Amulet of Yendor down to depth 40 and transcend Brogue through the portal. Pays once per run, and again 7 days after the last time it paid.',
    updated = current_timestamp
WHERE key = 'brogue_mastery';

-- Green Dragon: the kill resets the character to level 1, and the daily turn
-- cap makes the climb back 7-10 days. The kill number on the character row is
-- the whole gate; no lockout is needed on top of it.
UPDATE reward_templates
SET reward_chips = 20000,
    claim_policy = 'per_event',
    cooldown_seconds = NULL,
    description = 'Grow strong enough to face the Green Dragon in its forest lair and slay it. Pays for every kill.',
    updated = current_timestamp
WHERE key = 'greendragon_dragon_slain';

-- A Dark Room: the ending wipes the save, so a repeat is the whole arc again
-- (about five days). The run id is the gate.
UPDATE reward_templates
SET reward_chips = 15000,
    claim_policy = 'per_event',
    cooldown_seconds = NULL,
    description = 'Light the fire, raise the village, cross the wasteland, and fly the wrecked starship out through the debris cloud. Pays for every run that gets out.',
    updated = current_timestamp
WHERE key = 'darkroom_escape';

UPDATE reward_templates
SET reward_chips = 20000,
    claim_policy = 'per_event',
    cooldown_seconds = NULL,
    description = 'Clear the ravaged battleship, kill the immortal wanderer, take the fleet beacon, and fly out holding it. Pays for every run that gets out with it.',
    updated = current_timestamp
WHERE key = 'darkroom_beacon_escape';

-- Lateania: the character persists, so a maxed one kills the easy two in an
-- evening (the lockout answers that), and `d` deletes the character, so a
-- per-character gate alone would be a reroll farm (the lockout keys on the
-- account, not the character). Both gates, together.
UPDATE reward_templates
SET reward_chips = 10000,
    claim_policy = 'cooldown',
    cooldown_seconds = 604800,
    description = 'Defeat the Archdemon Mal''gareth in Lateania. Pays once per character, and at most once every 7 days.',
    updated = current_timestamp
WHERE key = 'lateania_archdemon_defeat';

UPDATE reward_templates
SET reward_chips = 10000,
    claim_policy = 'cooldown',
    cooldown_seconds = 604800,
    description = 'Defeat the King Who Was Promised Nothing in Lateania''s final Frontier zone. Pays once per character, and at most once every 7 days.',
    updated = current_timestamp
WHERE key = 'lateania_frontier_king_defeat';

UPDATE reward_templates
SET reward_chips = 20000,
    claim_policy = 'cooldown',
    cooldown_seconds = 604800,
    description = 'Defeat Yssgar, the Sundering Deep, in Lateania. Pays once per character, and at most once every 7 days.',
    updated = current_timestamp
WHERE key = 'lateania_sundering_deep_defeat';

UPDATE reward_templates
SET reward_chips = 20000,
    claim_policy = 'cooldown',
    cooldown_seconds = 604800,
    description = 'Defeat Kaethyr Ascendant, Who Sang the God Awake, in Kaelmyr. Pays once per character, and at most once every 7 days.',
    updated = current_timestamp
WHERE key = 'lateania_kaethyr_ascendant_defeat';
