-- The three roguelike win tiers drop from 50,000 to 40,000 chips (owner
-- decision 2026-09-08). The pickup tier (Amulet, Orb, Brogue's escape) stays
-- at 20,000, so the gap between reaching the bottom and getting back out is
-- 2x rather than 2.5x.
--
-- Only the amount moves: the gates set by migration 158 (one payout per
-- ingested run, and a 7-day per-milestone lockout per account) are untouched,
-- and claims already banked keep whatever they paid at the time.
UPDATE reward_templates
SET reward_chips = 40000,
    updated = current_timestamp
WHERE key IN ('nethack_ascension', 'dcss_win', 'brogue_mastery');
