-- Realm paid a flat 2000 to the winner, which put weeks of multiplayer war
-- below a single daily Asterion escape (4000) and an order of magnitude below
-- the door-game milestones it actually resembles (Green Dragon 10k, A Dark
-- Room 15-20k, NetHack ascension 40k).
--
-- The pot is now computed per finished game — 3000 chips per player who
-- started it, split by tier (see lobby/realm/svc.rs::payout_plan) — so a duel
-- is worth 6000 and a ten-player war 30,000. Placing in a crowded game is the
-- harder feat and now pays like it.
--
-- These rows keep owning the claim policy (per_event, one payment per rank
-- per game) and the game/payout_kind identity; `reward_chips` is the floor of
-- each tier, which is what a game at the small end of that tier actually
-- pays. The service passes the computed amount.
UPDATE reward_templates SET reward_chips = 6000,
    description = 'Win a realm game of up to 3 players (pot scales with the roster).'
    WHERE key = 'realm_win_small';
UPDATE reward_templates SET reward_chips = 8400,
    description = 'Win a realm game of 4-6 players (pot scales with the roster).'
    WHERE key = 'realm_win_mid';
UPDATE reward_templates SET reward_chips = 3600,
    description = 'Finish second in a realm game of 4-6 players (pot scales with the roster).'
    WHERE key = 'realm_runnerup_mid';
UPDATE reward_templates SET reward_chips = 12600,
    description = 'Win a realm game of 7 or more players (pot scales with the roster).'
    WHERE key = 'realm_win_large';
UPDATE reward_templates SET reward_chips = 5250,
    description = 'Finish second in a realm game of 7 or more players (pot scales with the roster).'
    WHERE key = 'realm_runnerup_large';
UPDATE reward_templates SET reward_chips = 3150,
    description = 'Finish third in a realm game of 7 or more players (pot scales with the roster).'
    WHERE key = 'realm_third_large';
