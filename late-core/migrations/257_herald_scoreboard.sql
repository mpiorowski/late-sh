-- The herald's Sunday kind (late-ssh `app/herald`): the week's best in
-- every late.sh game, the arcade, the Lobby and the doors (subject
-- `week:{last day}`). Migration 256 opened the check to the piece and the
-- paper.
ALTER TABLE herald_posts DROP CONSTRAINT herald_posts_kind_check;
ALTER TABLE herald_posts ADD CONSTRAINT herald_posts_kind_check
    CHECK (kind IN ('artboard', 'piece', 'scoreboard', 'paper'));
