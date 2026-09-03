# The Late Edition (`app/paper`) Context

## Metadata
- Domain: @graybeard's daily paper: one edition per UTC day, printed once per public room and read by every login.
- Last updated: 2026-09-03 (first version: room columns, Elsewhere, What we were reading, the flagged Outside page, the login pop and `/paper`).
- Status: Active

## What it is

A newspaper, not a per-reader summary. `/summary` is per viewer because its window is the reader's own device mark. The paper's window is fixed (edition dated D covers the UTC day D-1), so one room's column reads the same to everyone and is printed exactly once. Cost is per room per day, never per reader.

## Module map

| File | Role |
|---|---|
| `svc.rs` | `PaperService`: the sweeper (the press) and the open requests (the newsstand); `tick(app)`: the session-side orchestration (login pop, `/paper`, flag writes). The three system prompts live here. |
| `state.rs` | `PaperState` (per session), `PaperModal`, `PaperCommand` + parser, and `lay_out`: the pure function from an edition's rows plus this reader's rail order to the modal's lines. |
| `ui.rs` | The centered modal, announcements-shaped. |
| `input.rs` | Keys while the modal is up: `j/k`/arrows/PgUp/PgDn scroll, `Esc`/`q`/`Enter` close. |
| `late-core/src/models/paper.rs` | Every read and write of `paper_room_editions` and `paper_sections` (migration 173). |

## The press (multi-replica rule as applied)

- Every replica runs `start_sweeper_task` (`PAPER_SWEEP_INTERVAL`, 5 min). A sweep is: today's edition, list unsettled public rooms with human messages in the window, settle each, then the sections.
- **Rows are the claims.** A room under `PAPER_MIN_MESSAGES` (5) gets a `quiet` row and no call. Otherwise `claim_printing` inserts a `printing` row (`ON CONFLICT ... DO UPDATE ... WHERE status = 'printing' AND claimed_at < stale`), and only the winner calls the model; `finish` flips it to `ready` with the text. A failed print `release`s the row so the next sweep retries; a `printing` row older than `PAPER_STALE_CLAIM` (20 min) is a dead replica's and is taken over.
- Rooms: `visibility = 'public'`, kind in lounge/topic/language, system-feed lines excluded, no viewer and no ignore list (`ChatMessage::list_public_room_between`). Private rooms, DMs, game rooms, and #deadchannel never reach it.
- Sections: `reading` (yesterday's News shares via `Article::list_shared_between`, `quiet` when nobody shared) and `outside` (grounded `generate_reply`; the user turn carries the date anchor and the last `PAPER_OUTSIDE_MEMORY_EDITIONS` printed Outside pages as "already covered" so a slow week does not repeat; AI news is rationed to one line and only when enormous; a `NOTHING` answer settles `quiet`). `outside` prints only while `paper_outside_enabled` is on.
- Switches (`app_flags`, closed `AppFlag` enum): `paper_enabled` (kill switch) and `paper_outside_enabled` (the Outside page), both seeded on. `/paper on|off` and `/paper outside on|off` flip them, admin only, through `AppFlagService::set_task` like `/haunt`.
- Admin press hooks, banners with a tally when done: `/paper print` runs today's sweep now; `/paper preview` drops tomorrow's rows and prints that edition over today so far, so it re-counts every time (real rows, so midnight leaves them alone); `/paper reset` deletes today's and tomorrow's rows and the caller's `paper_shown_on` stamp so both the print and the login pop can be seen again. `print_edition(edition, floor, ceiling, with_outside)` is the one printer all three and the sweeper share.
- Clients are scoped to each query; nothing holds a pooled connection across the model call.

## The newsstand

- `request(user_id, trigger)` loads today's rows (`/paper` looks one day ahead first, so an admin's preview is readable the moment it prints; the login pop never does). `Login`: if the edition has any `ready` page, `User::claim_paper_shown` stamps `users.settings.paper_shown_on = edition`; a lost claim (other device, other replica) sends nothing. `Command` (`/paper`) always answers and costs nothing.
- The session arms `login_pop_pending` for everyone with the `paper_at_login` tweak on (Ctrl+O Tweaks → Startup → "Daily paper at login", default on), newcomers included: for them the paper is the answer to "is anyone here?" and, since a new account is only in the auto-join rooms, mostly an Elsewhere list with `/join` hints. `tick` fires it only once the opening sequence is over: splash down, announcements dismissed, and the clubhouse tour settled (`clubhouse::state::State::tutorial_settled`, which treats an armed-but-not-started tour as unsettled so the modal never lands over the walkthrough's key capture).
- Layout (`lay_out`): byline, YOUR ROOMS in rail order (favorites first, from `ChatState::visual_order`), ELSEWHERE ON LATE.SH (public rooms you are not in, bumped rooms first, top `PAPER_ELSEWHERE_LIMIT` = 3, with a `/join` hint), WHAT WE WERE READING, OUTSIDE, then a footer naming quiet rooms and rooms still at the press. Nothing in the layout is per reader beyond ordering and membership.
- The modal sits directly under the login announcements in input and render order; a ready paper waits in `pending_modal` while they are up.
- `/paper` shows an "at the press…" modal until the rows land; `Esc` on it drops the request (`awaiting` cleared), so a late answer never pops over something else.

## Telemetry

`record_paper_print(PaperPrintResult)` per page (printed / quiet / lost / failed) and `record_paper_open(PaperOpenResult)` per request (login / command / empty / already_shown / unavailable / failed). Print failures log through `late_core::error_span!` with the edition and room.

## Tests

`state_test.rs` (whole-modal layout assertion, command parsing), `svc_test.rs` (window math, column tidying, the newsstand's claim path against a real DB, the login pop and `/paper` driven through a full `App`), `late-core/src/models/paper_test.rs` (claims, reclaim, finish, sections, candidates), `user_test.rs` (the shown stamp).

## Gotchas

- The reading and outside prompts inherit `GRAYBEARD_PERSONA` from `app/ai/ghost.rs`, whose chat rules say never to name people; the column rules are appended after it and explicitly override that, because a paper that names nobody is useless.
- A room made private after its page printed keeps the row and loses the reader: `PaperEdition::load` joins on `visibility = 'public'`.
- The grounded path is prompt-enforced only (see `AiService::generate_json_with_search`); `tidy_column` is what makes the Outside reply safe to render.
