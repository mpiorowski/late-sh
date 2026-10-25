-- The herald's next kinds (late-ssh `app/herald`, AUDIENCE.md, the
-- herald's kinds): a piece from the gallery (subject `piece:{id}`, each
-- posted once) and the Late Edition's card on a day with nothing drawn
-- (subject `paper:{edition}`). One row a day, so the piece-once read
-- needs no index.
ALTER TABLE herald_posts DROP CONSTRAINT herald_posts_kind_check;
ALTER TABLE herald_posts ADD CONSTRAINT herald_posts_kind_check
    CHECK (kind IN ('artboard', 'piece', 'paper'));
