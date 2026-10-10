# Calendars

Screen 7 uses `late-core/models/calendar` for authorization, UTC instants, civil
all-day dates (exclusive end), revisions and indexed persistence. `svc.rs` alone
performs async database work. `state.rs` drains private session replies and shared
server-notice/invalidation watches. `input.rs` routes page and modal actions;
`navigation.rs` owns stable date/slot/event selection, click identity, context
menus and modal return frames. `editor.rs` owns semantic field focus, conditional
controls, form scrolling and text-cell caret geometry. `ui.rs` resolves theme
styles and records page/modal hit and scroll geometry at render time.
`parser.rs` performs deterministic title-suffix inference once per new draft.
`ical.rs` handles bounded RFC 5545 parsing/serialization; `import.rs` owns the
paste/URL dialog and event chooser. New/Edit event offers Import iCal to fill the
current form from iCalendar content or a public HTTP(S)/webcal URL; multi-event
feeds select one event. The editor remains a modal parent so cancellation restores
the draft. Selection fills its fields while preserving identity, source, access,
revision, delegation and the original dirty-check baseline; Save applies changes.
`y` copies the selected/details event through the session's OSC 52 clipboard.
URL reads run in `svc.rs` with the shared guarded downloader, a 1 MiB limit and
20-second timeout; cancelled/replaced dialogs reject stale import generations.
Imports retain all-day exclusive ends and convert UTC/IANA/floating times into
the account zone. Recurring series and unsupported zones produce visible errors.
`toolbar.rs` groups actions, calendar/view selectors and date navigation across
two control rows; compact layouts pair selectors with actions and keep navigation
and the period on a third row. Selector arrows cycle choices; labels/values open
pickers. Controls use the calendar canvas background. Bold control labels
embed an accent mnemonic among bright text: `c` Calendar, `s` Settings; both cases
work for header letter shortcuts. Selector values show an ellipsis only when
clipped. Bright titles and
values contrast with muted timing, dim metadata, faint separators and subdued
grid rules. Selection adds a marker and patches every span last, using at least
4.5:1 text contrast against fixed fills or terminal-owned inversion. High Contrast
derives a calendar-local selection fill at least 3:1 against its canvas. Hourly cards
reserve selection for the selected event; source buttons do not suggest focus.
Compact month cells omit markers when needed to keep the date and event count intact;
selected cells retain their selection style.
Account-local today has a subtle background tint in month cells, hourly columns
and date headings, with event cards and selection fills taking precedence.
Modal headings use the
canvas-safe accent rather than the glow color, which is white in some light themes.
`date_entry.rs` handles explicit date fields separately: flexible absolute forms,
unambiguous numeric dates and signed/ago/in offsets. Go-to-date offsets start from
the selection; editor offsets use account-local today. Today/weekday words and
omitted years use account-local today in both. Calendar months/years clamp the day,
then weeks/days apply; `humantime` parses fixed durations and notification leads.
Editor dates normalize on blur/Save without losing invalid text or re-enabling
inference. Notice leads are nonnegative whole seconds, capped at 3650 days.

Single clicks select; double clicks activate the same semantic target. Month/day
headers open agendas, event rows open details, and empty timed slots create drafts.
Explicit overflow hits open agendas immediately. `EventAt(id, date)` preserves
the clicked date on spanning month/timeline events; list/agenda hits use `Event`.
Right-click actions reflect current access. Pickers have independent wheel/page
scrolling. Agenda/Upcoming/Details/Editor return through saved selection and scroll
state; role/sharing invalidation clears saved shared content before reauthorization.
Timed arrows move half-hour slots or days; `j/k` reveals selected events vertically
and horizontally. Viewport row counts drive list selection scrolling.

The editor visits only applicable controls and scrolls by rendered row heights.
All-day hides times, disabled notices hide lead time, permission checks hide staff
controls, and repeated-time choices appear only for ambiguous local timestamps.
Save/Cancel/Import iCal remain fixed. Clicked text uses Ratatui grapheme cell widths to place
the caret. Discard confirmation defaults to keeping the draft; Escape keeps it.

The central `CalendarChanged` listener channel carries table-trigger invalidation
and reconnect resync. Shared personal content is cleared before reauthorization;
load and detail request generations reject stale replies independently. Private
snapshots never enter process-global watches. Each replica shares server notices;
visible Home/Calendars surfaces refresh eligibility each minute. Tick only drains
memory and dispatches async tasks; rendering does not query PostgreSQL.

Creator tier survives account role changes. Current DB roles govern writes, under
locks; private/public checks scope every event read. Revision mismatches preserve
the editor draft. Admin server events default protected; delegation permits full
moderator control. Moderator-created server events permit moderator CRUD but only
admins change notifications. Personal events belong exclusively to their owner.
Public calendars are read-only; overlays retain source labels/permissions.

Settings on `s` persist week start, default view, server overlay and calendar
sharing separately from account Settings. Account timezone or UTC governs timed
editing/display; all-day dates do not shift. All-day notice boundaries use the
creator's captured zone. DST gaps are rejected; repeats require separate start/end
Earlier/Later choices. Repeated-hour labels include the zone abbreviation;
overlap lanes check absolute instants as well as occupied half-hour cells, so
adjacent short events cannot paint over each other. Day/lane widths share available
space and keep title text ahead of timing/source metadata.
Upcoming notices are derived, with no delivery ledger or terminal alerts.

See root `CALENDAR.md` for controls, complete parked-feature list and fixture.
Run focused checks through Linux `make test-llm`, including `test(calendar)`;
`make check` is human-owned.
