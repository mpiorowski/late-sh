-- Art content ratings are gone: the Artboard content disclaimer is the only
-- gate, and the login splash shows a hung piece only to users who chose
-- Always View there. Votes, staff marks, and the hanger flag go with it.
DROP VIEW artboard_piece_content_ratings;
DROP TABLE artboard_piece_staff_marks;
DROP TABLE artboard_piece_content_votes;
ALTER TABLE artboard_pieces DROP COLUMN owner_marked_nsfw;

-- The SFW / Always / Never splash tweak went with the ratings.
UPDATE users
SET settings = settings - 'art_splash_mode',
    updated = current_timestamp
WHERE settings ? 'art_splash_mode';
