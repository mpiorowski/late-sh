-- The undercity's guide opens by itself the first time a runner comes
-- down, and only that once, on any device and any replica: the stamp is
-- the claim (a conditional update, `mark_guide_seen`). `?` on the street
-- opens it every other time. The change trigger (migration 202) does not
-- watch this column, so the stamp wakes no directory.
ALTER TABLE deadchannel_runners
    ADD COLUMN guide_seen_at TIMESTAMPTZ;
