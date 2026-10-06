-- Sliding Puzzle's daily art is a gallery piece: the first player to open
-- the puzzle on a UTC day claims the most applauded piece not yet featured,
-- hung before that day, and stamps it `featured_on`. Two pieces hung the same
-- day queue up, one per day; nothing else is moderated or stored.
--
-- The partial unique index is the whole concurrency story: two replicas
-- claiming the same day race on it and the loser reads the winner's row
-- back (`ArtboardPiece::feature_for_day`). It excludes taken-down pieces so
-- a mod removal frees the day for the next piece in line.
ALTER TABLE artboard_pieces ADD COLUMN featured_on DATE;
CREATE UNIQUE INDEX artboard_pieces_featured_on_idx
    ON artboard_pieces (featured_on)
    WHERE removed_at IS NULL;
