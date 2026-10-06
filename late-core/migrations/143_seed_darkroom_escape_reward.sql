-- The lifetime badge payout behind A Dark Room's ending, the twin of the
-- Green Dragon kill (100): one ending, one badge, one payout, enforced by the
-- lifetime game_payout_claims row the credit path takes. The save is wiped on
-- the way out, so a replay is a whole new run and still pays nothing.
INSERT INTO reward_templates
    (key, title, description, cadence, bucket, domain, difficulty, kind, params, target, reward_chips, weight, is_quest, claim_policy, cooldown_seconds)
VALUES
    (
        'darkroom_escape',
        'Get off this rock',
        'Light the fire, raise the village, cross the wasteland, and fly the wrecked starship out through the debris cloud. Awards chips once per account.',
        NULL,
        NULL,
        'darkroom',
        'hard',
        'game_win',
        '{"game":"darkroom","payout_kind":"escape"}'::jsonb,
        1,
        10000,
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
