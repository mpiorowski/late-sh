-- The splash wall: every piece hung in the gallery shows over the door
-- for one UTC day, in hang order, one piece a day. The gallery's hourly
-- refresh stamps the day's piece (`ArtboardPiece::splash_for_day`), the
-- same shape as the puzzle's `featured_on` (migration 188): the partial
-- unique index is the whole concurrency story, two replicas racing on it
-- and the loser reading the winner's row back. It excludes taken-down
-- pieces, so a removal after the day was assigned leaves the day empty
-- (the cup) and nothing is promoted into the gap.
ALTER TABLE artboard_pieces ADD COLUMN splash_on DATE;
CREATE UNIQUE INDEX artboard_pieces_splash_on_idx
    ON artboard_pieces (splash_on)
    WHERE removed_at IS NULL;
