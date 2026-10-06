-- The Old Signal's payout moves onto the door milestones' rails: a reward
-- template read by `ChipService::credit_run_cooldown_reward_template`, paying
-- once per mark (the event key is `<runner row id>:<mark>`) and at most once
-- every 30 days per account, the same monthly rule as Lateania's crowns and
-- the roguelikes. 40,000 chips, the Lateania full run's month.
INSERT INTO reward_templates
    (key, title, description, cadence, bucket, domain, difficulty, kind, params, target, reward_chips, weight, is_quest, claim_policy, cooldown_seconds)
VALUES
    (
        'deadchannel_old_signal_slain',
        'Put down the Old Signal',
        'Put down the Old Signal at the bottom of the undercity. Pays once per mark, and at most once every 30 days.',
        NULL,
        NULL,
        'deadchannel',
        'hard',
        'game_win',
        '{"game":"deadchannel","payout_kind":"old_signal_slain"}'::jsonb,
        1,
        40000,
        100,
        false,
        'cooldown',
        2592000
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
