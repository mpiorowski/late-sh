-- The pot's mid-week threshold lines ("the pot is over 25,000 chips") are
-- gone (SHOP.md Phase 5, decided 2026-08-27). The pot's size now sits in the
-- status HUD on every screen all week, so a #lounge nudge only repeated what
-- the border already said. This column was the high-water mark that made
-- each line once-per-pot across replicas and restarts; nothing reads it now.
ALTER TABLE pots DROP COLUMN announced_threshold;
