-- The first-visit tour gained the People sections and the events board and
-- its copy was rewritten, so every account walks it once more: dropping the
-- flag arms it again, and a session with the tour pending lands in the
-- Clubhouse whatever its "Land on" tweak says (late-ssh app/state.rs).
UPDATE users
SET settings = settings - 'clubhouse_tutorial_done',
    updated = current_timestamp
WHERE settings ? 'clubhouse_tutorial_done';
