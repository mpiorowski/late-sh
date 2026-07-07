INSERT INTO reward_templates
    (key, title, description, cadence, bucket, domain, difficulty, kind, params, target, reward_chips, weight, is_quest, claim_policy, cooldown_seconds)
VALUES
    (
        'nethack_amulet',
        'Acquire the Amulet of Yendor',
        'Reach the bottom of the dungeon and claim the Amulet of Yendor in NetHack. Awards chips once per account.',
        NULL,
        NULL,
        'nethack',
        'hard',
        'game_win',
        '{"game":"nethack","payout_kind":"amulet_acquired"}'::jsonb,
        1,
        10000,
        100,
        false,
        'per_event',
        NULL
    ),
    (
        'nethack_ascension',
        'Ascend to Demigod',
        'Carry the Amulet of Yendor up through Gehennom and the planes, then ascend in NetHack. Awards chips once per account.',
        NULL,
        NULL,
        'nethack',
        'hard',
        'game_win',
        '{"game":"nethack","payout_kind":"ascension"}'::jsonb,
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
