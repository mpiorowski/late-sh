-- The second lifetime payout behind A Dark Room, the twin of `darkroom_escape`
-- (143): fly out holding the fleet beacon taken off the immortal wanderer on
-- the ravaged battleship's command deck, and reach the wanderer homefleet.
--
-- Flat 10,000, the same as the plain escape and the four Lateania crowns. It
-- is a separate claim from `darkroom_escape`, so an account that already
-- escaped once can earn this one too, and neither can pay twice.
INSERT INTO reward_templates
    (key, title, description, cadence, bucket, domain, difficulty, kind, params, target, reward_chips, weight, is_quest, claim_policy, cooldown_seconds)
VALUES
    (
        'darkroom_beacon_escape',
        'Find the homefleet',
        'Clear the ravaged battleship, kill the immortal wanderer, take the fleet beacon, and fly out holding it. Awards chips once per account.',
        NULL,
        NULL,
        'darkroom',
        'hard',
        'game_win',
        '{"game":"darkroom","payout_kind":"beacon_escape"}'::jsonb,
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
