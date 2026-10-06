-- The four Lateania crowns now pay the same: 10,000 chips each, once per
-- account. Before this, only two of them paid at all (Archdemon 10k, Frontier
-- King 20k) and the two deepest crowns were badge-only, which read as the
-- hardest fights in the game being worth the least.
--
-- Claims already banked are not touched: whoever took the King at 20,000 keeps
-- those chips. The lifetime claim rows are what stop a re-kill from paying
-- again, and they are unchanged.
UPDATE reward_templates
SET reward_chips = 10000,
    updated = current_timestamp
WHERE key = 'lateania_frontier_king_defeat';

INSERT INTO reward_templates
    (key, title, description, cadence, bucket, domain, difficulty, kind, params, target, reward_chips, weight, is_quest, claim_policy, cooldown_seconds)
VALUES
    (
        'lateania_sundering_deep_defeat',
        'Defeat Yssgar',
        'Defeat Yssgar, the Sundering Deep, in Lateania. Awards chips once per account.',
        NULL,
        NULL,
        'lateania',
        'hard',
        'game_win',
        '{"game":"mud","payout_kind":"sundering_deep_defeat"}'::jsonb,
        1,
        10000,
        100,
        false,
        'per_event',
        NULL
    ),
    (
        'lateania_kaethyr_ascendant_defeat',
        'Defeat Kaethyr Ascendant',
        'Defeat Kaethyr Ascendant, Who Sang the God Awake, in Kaelmyr. Awards chips once per account.',
        NULL,
        NULL,
        'lateania',
        'hard',
        'game_win',
        '{"game":"mud","payout_kind":"kaethyr_ascendant_defeat"}'::jsonb,
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

-- Badge rows recorded while these two crowns were badge-only hold
-- score_value = 0. Re-record them at the flat payout so the badge history
-- reads in chips like the other crowns. This touches the badge display only:
-- the chips themselves stay re-kill only, paid through the templates above
-- the next time the boss falls (these holders have no lifetime claim row).
UPDATE profile_awards
SET score_value = 10000
WHERE category IN ('lateania_sundering_deep', 'lateania_kaethyr_ascendant')
  AND score_value = 0;
