-- Bringing a track to the jukebox pays chips, capped per person per UTC day.
-- Every submission counts today's paid rows before crediting, and that lookup
-- must not turn into a sequential scan over a ledger that only grows.
-- Same reason as `chip_ledger_news_shared_idx` (migration 163); the columns
-- differ because this gate is a count of the day, not a lookup by URL.
CREATE INDEX chip_ledger_song_queued_idx
    ON chip_ledger (user_id, created_at)
    WHERE reason = 'song_queued';
