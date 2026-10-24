-- The herald's first version posts one kind of picture, yesterday's
-- Artboard. The poster kind migration 252 allowed for was dropped before
-- launch: too few new posters to carry a slot (AUDIENCE.md, the herald's
-- future work names what replaces it). The column stays, so the next
-- kinds widen this check instead of changing the row's shape; the index
-- served the poster-once query and nothing reads it now.
ALTER TABLE herald_posts DROP CONSTRAINT herald_posts_kind_check;
ALTER TABLE herald_posts ADD CONSTRAINT herald_posts_kind_check
    CHECK (kind IN ('artboard'));
DROP INDEX herald_posts_kind_subject_idx;
