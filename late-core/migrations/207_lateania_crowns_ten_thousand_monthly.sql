-- Every Lateania crown pays the same again, 10,000 chips, and the account
-- lockout stretches from 7 to 30 days: a full run is 40,000 a month, in band
-- with the other boss payouts and the month tier of the shop. The gates from
-- migration 158 stay as they are (once per character, and the lockout on the
-- account); claims already banked keep what they paid.

UPDATE reward_templates
SET reward_chips = 10000,
    cooldown_seconds = 2592000,
    description = 'Defeat the Archdemon Mal''gareth in Lateania. Pays once per character, and at most once every 30 days.',
    updated = current_timestamp
WHERE key = 'lateania_archdemon_defeat';

UPDATE reward_templates
SET reward_chips = 10000,
    cooldown_seconds = 2592000,
    description = 'Defeat the King Who Was Promised Nothing in Lateania''s final Frontier zone. Pays once per character, and at most once every 30 days.',
    updated = current_timestamp
WHERE key = 'lateania_frontier_king_defeat';

UPDATE reward_templates
SET reward_chips = 10000,
    cooldown_seconds = 2592000,
    description = 'Defeat Yssgar, the Sundering Deep, in Lateania. Pays once per character, and at most once every 30 days.',
    updated = current_timestamp
WHERE key = 'lateania_sundering_deep_defeat';

UPDATE reward_templates
SET reward_chips = 10000,
    cooldown_seconds = 2592000,
    description = 'Defeat Kaethyr Ascendant, Who Sang the God Awake, in Kaelmyr. Pays once per character, and at most once every 30 days.',
    updated = current_timestamp
WHERE key = 'lateania_kaethyr_ascendant_defeat';
