-- The roguelike milestones join Lateania's rule: a milestone pays once a
-- month. The per-account lockout on each of the six moves from 7 to 30 days;
-- the amounts (20,000 for the pickup, 40,000 for the win) and the one payout
-- per ingested run stay as they are, and claims already banked keep what they
-- paid. A strong player's ceiling per roguelike is 60,000 a month.

UPDATE reward_templates
SET cooldown_seconds = 2592000,
    description = 'Reach the bottom of the dungeon and claim the Amulet of Yendor in NetHack. Pays once per run, and again 30 days after the last time it paid.',
    updated = current_timestamp
WHERE key = 'nethack_amulet';

UPDATE reward_templates
SET cooldown_seconds = 2592000,
    description = 'Carry the Amulet of Yendor up through Gehennom and the planes, then ascend in NetHack. Pays once per run, and again 30 days after the last time it paid.',
    updated = current_timestamp
WHERE key = 'nethack_ascension';

UPDATE reward_templates
SET cooldown_seconds = 2592000,
    description = 'Descend through the Realm of Zot and pick up the Orb in Dungeon Crawl Stone Soup. Pays once per run, and again 30 days after the last time it paid.',
    updated = current_timestamp
WHERE key = 'dcss_orb';

UPDATE reward_templates
SET cooldown_seconds = 2592000,
    description = 'Carry the Orb of Zot back up and out of the dungeon in Dungeon Crawl Stone Soup. Pays once per run, and again 30 days after the last time it paid.',
    updated = current_timestamp
WHERE key = 'dcss_win';

UPDATE reward_templates
SET cooldown_seconds = 2592000,
    description = 'Grab the Amulet of Yendor from depth 26 and climb back out of Brogue alive. Pays once per run, and again 30 days after the last time it paid.',
    updated = current_timestamp
WHERE key = 'brogue_escape';

UPDATE reward_templates
SET cooldown_seconds = 2592000,
    description = 'Carry the Amulet of Yendor down to depth 40 and transcend Brogue through the portal. Pays once per run, and again 30 days after the last time it paid.',
    updated = current_timestamp
WHERE key = 'brogue_mastery';
