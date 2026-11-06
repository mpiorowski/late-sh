-- The herald's announcement kind (late-ssh `app/herald`): the operator's
-- `#announcements` posts of the day an edition covers, compressed into one
-- text-only post the day the edition prints (subject
-- `announcement:{edition}`). Migration 257 opened the check to the
-- scoreboard.
ALTER TABLE herald_posts DROP CONSTRAINT herald_posts_kind_check;
ALTER TABLE herald_posts ADD CONSTRAINT herald_posts_kind_check
    CHECK (kind IN ('artboard', 'piece', 'scoreboard', 'paper', 'announcement'));
