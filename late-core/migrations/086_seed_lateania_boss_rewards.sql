INSERT INTO reward_templates
    (key, title, description, cadence, bucket, domain, difficulty, kind, params, target, reward_chips, weight, is_quest, claim_policy, cooldown_seconds)
VALUES
    (
        'lateania_archdemon_defeat',
        'Defeat the Archdemon',
        'Defeat the Archdemon Mal''gareth in Lateania.',
        NULL,
        NULL,
        'lateania',
        'hard',
        'game_win',
        '{"game":"mud","payout_kind":"archdemon_defeat"}'::jsonb,
        1,
        10000,
        100,
        false,
        'per_event',
        NULL
    ),
    (
        'lateania_frontier_king_defeat',
        'Defeat the Frontier King',
        'Defeat the King Who Was Promised Nothing in Lateania''s final Frontier zone.',
        NULL,
        NULL,
        'lateania',
        'hard',
        'game_win',
        '{"game":"mud","payout_kind":"frontier_king_defeat"}'::jsonb,
        1,
        20000,
        100,
        false,
        'per_event',
        NULL
    )
ON CONFLICT (key) DO UPDATE SET
    title = EXCLUDED.title,
    description = EXCLUDED.description,
    cadence = EXCLUDED.cadence,
    bucket = EXCLUDED.bucket,
    domain = EXCLUDED.domain,
    difficulty = EXCLUDED.difficulty,
    kind = EXCLUDED.kind,
    params = EXCLUDED.params,
    target = EXCLUDED.target,
    reward_chips = EXCLUDED.reward_chips,
    weight = EXCLUDED.weight,
    is_quest = EXCLUDED.is_quest,
    claim_policy = EXCLUDED.claim_policy,
    cooldown_seconds = EXCLUDED.cooldown_seconds,
    active = true,
    updated = current_timestamp;
