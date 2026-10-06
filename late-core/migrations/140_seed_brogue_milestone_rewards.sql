-- Lifetime badge payouts for the Brogue pair (devdocs/PLAN-ROGUELIKE-BOARDS.md
-- Phase 3), the exact twins of the NetHack (090) and DCSS (136) pairs:
-- Escaped mirrors the artifact pickup tier, Mastered (the super-victory, out
-- with the Birthright of Yendor) mirrors the full win. Once per account,
-- enforced by the lifetime game_payout_claims row the credit path takes.
INSERT INTO reward_templates
    (key, title, description, cadence, bucket, domain, difficulty, kind, params, target, reward_chips, weight, is_quest, claim_policy, cooldown_seconds)
VALUES
    (
        'brogue_escape',
        'Escape the Dungeons of Doom',
        'Grab the Amulet of Yendor from depth 26 and climb back out of Brogue alive. Awards chips once per account.',
        NULL,
        NULL,
        'brogue',
        'hard',
        'game_win',
        '{"game":"brogue","payout_kind":"escape"}'::jsonb,
        1,
        10000,
        100,
        false,
        'per_event',
        NULL
    ),
    (
        'brogue_mastery',
        'Master the Dungeons of Doom',
        'Carry the Amulet of Yendor down to depth 40 and transcend Brogue through the portal. Awards chips once per account.',
        NULL,
        NULL,
        'brogue',
        'hard',
        'game_win',
        '{"game":"brogue","payout_kind":"mastery"}'::jsonb,
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
