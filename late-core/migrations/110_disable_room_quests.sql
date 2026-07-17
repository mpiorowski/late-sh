-- Assigned quests are arcade-only for now (owner decision 2026-07-13, see
-- devdocs/FRD-LOBBY-CONSOLIDATION.md): the Rooms demolition strips the
-- room-game runtimes down to sit-down activity events, so every
-- room_rounds_played / room_wins quest becomes unservable. Deactivate the
-- templates (never delete: quest history references them) and drop any
-- current or future assignments pointing at them so the draw refills those
-- slots from the arcade pool. User progress rows cascade with the
-- assignment; past periods keep their history.

UPDATE reward_templates
SET active = false,
    updated = current_timestamp
WHERE kind IN ('room_rounds_played', 'room_wins');

DELETE FROM quest_assignments a
USING reward_templates t
WHERE t.id = a.template_id
  AND t.kind IN ('room_rounds_played', 'room_wins')
  AND a.period_end >= CURRENT_DATE;
