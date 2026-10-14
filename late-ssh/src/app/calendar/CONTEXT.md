# Calendars: the events board

Screen 7. `late-core/models/calendar` owns the tables, the three rules
(`event_access`), the post guards (daily cap over the `calendar_posts` log,
ban) and every write under a row lock, with the owner scope inside every
statement; `late-core/models/calendar_ban` the ban rows. Full design,
controls and the fixture are in root `CALENDAR.md`.

## Files

- `svc.rs`: the orchestration layer and the only async code. Every `*`
  task owns its span and metrics; `settle` is the one match that turns a
  `CalendarError` into a metric and a message (a refusal is told in the
  user's words, a failure is logged and told in general terms). `save`
  ships `EventPosted` on a new board post through the activity publisher
  and answers a revision refusal as `SaveError::Conflict`, the one refusal
  the editor recovers from in place;
  `start_sweeper_task` claims started board events every minute
  (`CalendarStore::claim_started`) and ships `EventStarting` for each.
  `start_notify_worker` turns `CalendarChanged`, or the listener coming
  back, into an epoch bump plus a re-read of the board's live events into
  the shared `watch`; its minute tick re-reads the board alone. The board
  already published stays up until a fresh read replaces it.
- `state.rs`: the session's mirror. Events for the visible range, the ids
  they said they are in, the modal stack, the cursor. `tick` drains the
  epoch, the board watch and the private reply channel; `live_at` is the
  board watch minus what ended; `show_event` is how the strip and the
  panel land on the page. Never writes, never queries.
- `navigation.rs`: selection, double-click identity, the context menu's
  items from `event_access`, modal frames that restore the cursor on pop,
  `finish_delete`, `replace_saved`.
- `editor.rs`: the form. Six text fields; the `Target` control (board or
  just me) only on a new draft; DST occurrence controls only for a repeated
  local hour; `parser.rs` fills the timing from a title suffix once. The
  editor keeps the zone it opened in, so an account timezone
  change mid-edit moves nothing. A save that lost to a newer revision sets
  `conflict`; Ctrl+L or a click on the error row reloads with the draft
  kept.
- `date_entry.rs`: `Oct 2`, `2026-10-02`, `+2w`, `tomorrow` for the date
  fields.
- `toolbar.rs`: the two header rows, every control a hit target.
- `ui.rs`: the month grid (compact under 14 rows), the agenda, the list,
  the foot's hints for the selection, the details and delete modals, the
  context menu. The cursor is an amber dot (`marked_line`), never a fill;
  modals keep the filled `selected_line` for their focused control. Styles resolve
  in the active theme at draw time; hits and scroll panes are recorded at
  draw and cleared on resize.
- `input.rs`: page keys and clicks on screen 7, everything while a
  calendar modal or menu is up, nothing elsewhere.
- `live.rs`: the board on the house's surfaces. The strip source
  (`LiveSource::BoardEvent`): timed events stamped an hour before the start
  and again at the start, the drawn calendar page, the words, the compact
  line. `panel_events`: the Live panel's rows, timed events with the start
  as a clock time in the viewer's zone. `day_text`: the #lounge topic row
  on a day with all-day board events.

## Contracts

- The board is shared, the rest is private. Live board events reach every
  session through the replica's `watch`; a viewer's "I'm in" list travels
  on their session channel only. A viewer's own events are on this page
  alone: never the strip, the panel or the topic row.
- Timed events get the strip and the panel from `STRIP_LEAD` before the
  start and the start headline; all-day events get the topic row for their
  day and no headline.
- An invalidation clears the mirror, re-reads when the page or a calendar
  modal is open, and re-reads an open details modal; a post staff took down
  closes with `Gone`. Home's strip and Live panel read only the shared
  board, so Home never loads the session's own month.
- `calendar_changed` fires for board changes only (row triggers): a
  personal write or a statement that changes no row wakes nobody.
- A new draft is private; the editor's target control flips it to the
  board. Board titles paint amber, the viewer's own green.
- Rendering and the tick do no database I/O.
- The two #lounge lines, the live window and the daily cap are constants
  (`STRIP_LEAD`, `BOARD_POSTS_PER_DAY` in `late-core`), not settings.

## Tests

`make test-llm ARGS="-p late-core -p late-ssh -E 'test(calendar)'"`.
Model rules and the claim in `late-core/src/models/calendar_test.rs`; the
two lines, refusals and the listener in `svc_test.rs`; the mirror in
`state_test.rs`; selection and menus in `navigation_test.rs`; keys in
`input_test.rs`; the form in `editor_test.rs`; what the page draws in
`ui_test.rs`; the strip source in `live_test.rs`; the whole app in
`late-ssh/src/app/calendar_flow_test.rs`.
