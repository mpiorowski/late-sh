-- Season 3 opens on the campaign rules season 2 was played by, and season 2
-- (ten players) was decided on day 15 by whoever led the land rush: the
-- domination bar for ten was 21% of the world, free land counted towards it,
-- and nobody had to fight to clear it. The campaign now wins on half the
-- world held for three nights, or the biggest share when day 20 closes, with
-- more points a night, and its winner is paid no less than the campaign's
-- floor (late-ssh/src/app/door/realm/rulesets.rs, CAMPAIGN; RULES.md
-- "Orders" and "Ranked payouts").
--
-- A world freezes its rules into its state at the opening, so the new
-- CAMPAIGN reaches season 4 on its own. Season 3 is rewritten here, once,
-- while it is still mustering and nobody has spent a point: these are the
-- five fields of the new CAMPAIGN at the house's fast pace
-- (RealmRulesetSnapshot::at_pace). The revision moves so a write racing this
-- one loses its CAS and reloads, and the realm_game_changed trigger tells
-- every replica to re-read.
UPDATE realm_games
SET state = jsonb_set(
        jsonb_set(
            jsonb_set(
                jsonb_set(
                    jsonb_set(
                        jsonb_set(state,
                            '{ruleset,actions_per_day_tiers}', '[[4, 18], [10, 15], [20, 12]]'),
                        '{ruleset,domination_share_tiers}', '[[20, 0.5]]'),
                    '{ruleset,season_days}', '20'),
                '{ruleset,min_active_days_to_win}', '10'),
            '{ruleset,win_min}', '20000'),
        '{revision}', to_jsonb((state->>'revision')::bigint + 1)),
    updated = current_timestamp
WHERE season = 3
  AND ruleset_id = 'campaign'
  AND status IN ('open', 'active')
  AND state->'ruleset'->>'pace_id' = 'fast';
