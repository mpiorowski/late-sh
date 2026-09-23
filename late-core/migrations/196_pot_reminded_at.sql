-- The pot's closing-soon reminder in #lounge: one line per pot, half an hour
-- before the draw. Every replica sweeps, so the stamp is the claim: the
-- guarded UPDATE in `Pot::claim_reminder` sets it once and every later
-- sweeper, on any replica, matches nothing. NULL until the reminder is sent,
-- and forever on a pot whose draw came before any sweeper saw the window.
ALTER TABLE pots ADD COLUMN reminded_at TIMESTAMPTZ;
