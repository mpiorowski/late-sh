# Calendars: the events board

Screen 7, in the Tab cycle. The house events board with a private calendar
behind it: anyone posts to the board, everyone sees it, anyone says they are
in. A viewer's own events sit on the same grid, drawn green, and nobody else
ever sees them. The board's job is to ship a story into the room: a post is a
line in #lounge, a timed event heads the Home `now` strip and the Live panel
from the hour before, an all-day event takes the #lounge topic row for its
day, and a start is a headline. There is no Home panel of its own.

`late-core/src/models/calendar.rs` owns the tables, the three rules
(`event_access`), the post guards and every write under a row lock, with the
owner scope inside every statement; `calendar_ban.rs` the ban rows.

## 1. The three rules

- The poster edits and deletes their own post. Nobody else edits it.
- Moderators and admins delete any post. The delete dialog names the poster.
- Anyone says they are in on a board post. A personal event takes no "I'm in",
  and nobody but its owner ever reads it, staff included: to anyone else every
  read and write answers as if it did not exist.

Posting has two guards, both decided inside the post transaction:
`BOARD_POSTS_PER_DAY` (5) board posts per account per UTC day, and a board
ban. Edits and personal events never count. A deleted post still does: the cap
counts `calendar_posts`, a log of every board post made, which deletes never
touch. `/mod ban calendar @name [duration] [reason]`, `/mod unban calendar
@name`, `/mod view bans calendar`; a ban stops new posts and nothing else.

## 2. The page

Header row: the month and the viewer's timezone. Control row: `n New event`,
`t Today`, `[ Previous`, `] Next`, `v Month`/`v List`, `b Board shown`/`b
Board hidden`, each a hit target; an error sits under them. Month is six
Monday-start weeks, each day previewing its events with the in-count first
(`·6 Movie night`); on a wide terminal the selected day's agenda sits beside
the grid; under 14 rows the grid is compact (`2+12` counts). List is the month
in day order. Board posts read amber, own events green; the header says
`loading` while a month is fetched.

The cursor is an amber dot on the selected day's first event, never a fill;
`j`/`k` move it along the day. A foot row says what the keys do to the
selection.

| Key | Action |
|-----|--------|
| `n`, `e`, `d` | New, edit, delete (confirmation) as the rules allow |
| `i` | I'm in / I'm out on a board post |
| `[` / `]`, `t` | Month back / forward, today |
| `v`, `b` | Month or List; board shown or hidden |
| `j` / `k`, arrows | Events in the day; days (List: events) |
| `Enter` | Open the marked event, or start one on an empty day |
| PageUp / PageDown | Scroll |
| mouse | Click selects, double click opens, right-click offers Open, I'm in/out, Edit, Delete as the rules allow |

Details show the title, the timing in the viewer's zone, the poster, who is in
(`6 in, you included`), the description, then the allowed buttons. The editor
takes a title, a description, the target (just me by default, or the board;
fixed once saved), all-day or a start and end. A title like `movie night
tomorrow at 9pm` fills the date and time once (`parser.rs`); it never infers
again. Date fields take `Oct 2`, `2026-10-02`, `+2w`, `tomorrow`. Escape offers
to discard a changed draft and keeps it by default. A save that lost to a
newer edit elsewhere shows an error; `Ctrl+L` or a click on it reloads the
newer revision and keeps the draft.

The session keeps the view, the date and whether the board is shown.

## 3. The story

- **A post** ships a `#lounge` ticker line (`ActivityKind::EventPosted`):
  `mat posted Movie night to the board, in 2d 3h`, the title mention-safe.
- **The hour before** a timed event it joins the Home `now` strip
  (`LiveSource::BoardEvent`, `app/live`): a drawn calendar page, the title,
  `in 58m · 6 in`, the poster, `o` or a click for the board. It joins again at
  the start, lit, reading `on now`.
- **The Live panel** holds a row for each timed board event from `STRIP_LEAD`
  before its start until it ends, soonest first, the start as a clock time in
  the viewer's zone (`event   21:00 Movie night ·6`).
- **An all-day event** takes the #lounge topic row for its day in amber
  (`📅 Hackathon · Community call`, `7 for the board` where `/rules` sits). The
  strip still wins its five minutes; the strip and the panel never show an
  all-day event, and it gets no start headline.
- **The start** of a timed event is a headline that stays in #lounge history
  (`EventStarting`): `📅 Movie night is on now, 6 in. Press 7 for the board.`
  Every replica sweeps once a minute; the row's `announced_at` stamp is the
  claim, so one replica posts.

Personal events reach none of this. Opening from the strip or the panel lands
on the event's day with its details up.

## 4. Timing

The account timezone (or UTC) governs every timed display and the editor's
parsing; the effective zone is in the header and the editor footer. An open
editor keeps the zone it opened in. All-day events store civil dates with an
exclusive end, read in the zone captured at posting, so a later account change
does not move them. Timed starts and optional ends are UTC instants; a
start-only event lasts an hour. Nonexistent local times are errors; repeated
local times need Earlier or Later for the start and the end separately. Every
event also carries derived `starts_at`/`ends_at`: a timed post is live for
`starts_at - 1h <= now < ends_at`, an all-day post for its day.

## 5. Data and distribution

- Tables: `calendar_events` (a board post has `owner_id` NULL, a personal
  event its owner), `calendar_rsvps` (one row per person per post),
  `calendar_posts` (the cap's log), `calendar_bans`.
- Row triggers fire `calendar_changed` only for what another session can see:
  a board post's insert, edit or delete, and an "I'm in". A personal write or a
  statement that changes no row wakes nobody.
- The central listener's `CalendarChanged` channel bumps an epoch every session
  reads on its tick: it drops what it loaded, re-reads at once when the page or
  a calendar modal is open, otherwise when it next opens the page. An open
  details modal re-reads and closes with `Gone` if staff took the post down.
- The board's live events (`CalendarStore::live_board`) are one process-shared
  `watch` per replica, re-read on every notify and once a minute; the minute's
  re-read invalidates no session, and the published board stays up until a
  fresh read replaces it. The strip, the panel and the topic row all read it,
  so Home never loads the session's own month. A viewer's "I'm in" list
  travels on their session channel only.
- Rendering and the tick do no database I/O. Hit geometry is recorded at draw
  and cleared on resize.
- The lines, the live window and the cap are constants (`STRIP_LEAD`,
  `BOARD_POSTS_PER_DAY` in `late-core`), not settings.

## 6. Files

- `svc.rs`: the orchestration layer and the only async code. Every `*_task`
  owns its span and metrics; `settle` is the one match that turns a
  `CalendarError` into a metric and a message (a refusal in the user's words, a
  failure logged and told in general terms). `save` ships `EventPosted` and
  answers a revision refusal as `SaveError::Conflict`. `start_sweeper_task`
  claims started board events (`CalendarStore::claim_started`).
  `start_notify_worker` handles `CalendarChanged` and the listener's resync.
- `state.rs`: the session's mirror (events for the visible range, own "I'm in"
  ids, the modal stack, the cursor). `tick` drains the epoch, the board watch
  and the private reply channel; `live_at` is the board minus what ended;
  `show_event` is how the strip and the panel land on the page. Never writes,
  never queries.
- `navigation.rs`: selection, double-click identity, the context menu's items
  from `event_access`, modal frames that restore the cursor on pop.
- `editor.rs`: the form; the `Target` control only on a new draft; DST
  occurrence controls only for a repeated local hour; `conflict` on a lost save.
- `date_entry.rs`, `parser.rs`, `toolbar.rs`, `ui.rs`, `input.rs` (page keys
  and clicks on screen 7 and while a calendar modal is up, nothing elsewhere).
- `live.rs`: the strip source, `panel_events` (the Live panel rows),
  `day_text` (the topic row).

## 7. Telemetry

Page visits and attention ride the screen metrics (`screen="calendars"`). The
service records every write (`late_ssh_calendar_writes_total{write}`: board or
personal post, edit, delete, staff delete, in, out), every refusal by rule
(`..._refusals_total{reason}`), every database failure by operation
(`..._failures_total{op}`), every line handed to #lounge
(`..._announcements_total{line}`), and every open from the strip or the panel
(`..._opens_total{from}`). Nothing below `settle` logs.

## 8. Fixture and tests

`make seed-calendar` creates `cal_user`, `cal_other`, `cal_mod` and
`cal_admin` with keys in `tmp/calendar-seed-keys/`, a board with posts from
each (one on now, one in half an hour, one tomorrow evening, an all-day one
today, a crowded day), a few "I'm in"s and a private event for `cal_user`.
Connect with `ssh -o IdentitiesOnly=yes -i tmp/calendar-seed-keys/cal_user -p
2222 localhost`; `1` shows the strip, the panel and the topic row, `7` the
board. Rerun to reset the fixture's deterministic events.

`make test-llm ARGS="-p late-core -p late-ssh --lib -E 'test(calendar)'"`. Model
rules and the claim in `late-core/src/models/calendar_test.rs`; the lines,
refusals and the listener in `svc_test.rs`; the mirror in `state_test.rs`;
selection and menus in `navigation_test.rs`; keys in `input_test.rs`; the form
in `editor_test.rs`; the page in `ui_test.rs`; the strip source in
`live_test.rs`; the Live panel row in `app/live/panel_test.rs`; the whole app
in `late-ssh/src/app/calendar_flow_test.rs`.

## 9. Not built, on purpose

Recurrence, invitations, reminders beyond the strip and the panel, per-event
notification leads, sharing a personal calendar, week and day views,
iCalendar import or export, calendar settings. The board is a clubhouse
mechanic, not a calendar application; the usage metrics above answer first
whether any of these earns a place.
