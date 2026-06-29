UPDATE reward_templates
SET description = 'Defeat the Archdemon Mal''gareth in Lateania. Awards chips once per account.',
    updated = current_timestamp
WHERE key = 'lateania_archdemon_defeat';

UPDATE reward_templates
SET description = 'Defeat the King Who Was Promised Nothing in Lateania''s final Frontier zone. Awards chips once per account.',
    updated = current_timestamp
WHERE key = 'lateania_frontier_king_defeat';
