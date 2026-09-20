# The Late Edition (`app/paper`) Context

## Metadata
- Domain: @graybeard's daily paper: one edition per UTC day, printed once per public room and read by every login.
- Last updated: 2026-09-11 (ANNOUNCEMENTS: the login `#announcements` modal is gone (`app/announcements.rs` deleted, nothing on `App`); the paper prints every `#announcements` post from the covered day verbatim at the top, read at open time with no claim and no press (`ChatMessage::list_public_room_between_with_author`, newest `PAPER_ANNOUNCEMENTS_LIMIT` = 50), `#announcements` is excluded from the room columns in `list_candidates`, and a day with an announcement and no column still counts as a paper: `/paper` shows it at once, the login pop waits for the sweep (`PaperEdition::is_swept`). A post lands in the next day's edition, never the same day's. `PaperOutcome::Ready` carries a `PaperIssue` (edition, announcements, wall). Earlier, 2026-09-06: ON THE WALL is one rule now: up to `PAPER_WALL_PIECES` = 3 of yesterday's pieces, most applauded first, no applause floor and no line budget. Earlier, 2026-09-05: the modal's lines carry `PaperInk` and `ui.rs` picks the colours in the draw, so the paper stops printing in whichever session last rendered on this thread; ON THE WALL: yesterday's most applauded Artboard gallery pieces, up to `PAPER_WALL_PIECES` = 3, read at open time with no claim (`ArtboardPiece::most_applauded_hung_on`), printed in their own colours under the Outside page; `PaperOutcome::Ready` carries the `Vec<PaperWall>`.)
- Status: Active

## What it is

A newspaper, not a per-reader summary. `/summary` is per viewer because its window is the reader's own device mark. The paper's window is fixed (edition dated D covers the UTC day D-1), so one room's column reads the same to everyone and is printed exactly once. Cost is per room per day, never per reader.

It is also where the operator's word lands: every `#announcements` post from the covered day prints at the top, as written, with no model in the path and no threshold. The paper replaced the old login announcements modal on 2026-09-11. That changed the delivery guarantee: the modal reached every reader at their next login, the paper reaches them in the next day's edition, and only once at login. A same-day notice ("maintenance tonight") is not something the paper can carry; the operator posts a day ahead, or the room itself is the channel.

## Module map

| File | Role |
|---|---|
| `svc.rs` | `PaperService`: the sweeper (the press) and the open requests (the newsstand); `tick(app)`: the session-side orchestration (login pop, `/paper`, flag writes). The three system prompts live here. |
| `state.rs` | `PaperState` (per session), `PaperModal`, `PaperCommand` + parser, `PaperInk`/`PaperSpan`/`PaperLine` (the ink vocabulary), and `lay_out`: the pure function from an edition's rows plus this reader's rail order to the modal's lines. Reads no palette. |
| `ui.rs` | The centered modal, announcements-shaped; `ink_style` maps `PaperInk` onto the theme, exhaustively, inside the draw. |
| `input.rs` | Keys while the modal is up: `j/k`/arrows/PgUp/PgDn scroll, `Esc`/`q`/`Enter` close. |
| `late-core/src/models/paper.rs` | Every read and write of `paper_room_editions` and `paper_sections` (migration 173). |

## The press (multi-replica rule as applied)

- Every replica runs `start_sweeper_task` (`PAPER_SWEEP_INTERVAL`, 5 min). A sweep is: today's edition, list unsettled public rooms with human messages in the window, settle each, then the sections.
- **Rows are the claims.** A room under `PAPER_MIN_MESSAGES` (5) gets a `quiet` row and no call. Otherwise `claim_printing` inserts a `printing` row (`ON CONFLICT ... DO UPDATE` takes over a `printing` row older than the stale bound or a `failed` row under the attempt cap), and only the winner calls the model; `finish` flips it to `ready` with the text, or `quiet` when the model had nothing usable (one call, no retry). A failed print marks the row `failed` and keeps its attempt count: the next sweep claims it again until `PAPER_MAX_ATTEMPTS` (3), then the row is settled for the day, so an outage never turns the day's budget into a retry storm on the shared key. A `printing` row older than `PAPER_STALE_CLAIM` (20 min) is a dead replica's and is taken over; `mark_quiet` takes over stale and failed rows too. Sections already settled are skipped before any claim, so `Lost` only ever counts a claim another replica holds right now.
- Rooms: `visibility = 'public'`, kind in lounge/topic/language, slug not `announcements` (`ANNOUNCEMENTS_SLUG`; those posts print verbatim, never as a column), system-feed lines excluded, no viewer and no ignore list (`ChatMessage::list_public_room_between`). Private rooms, DMs, game rooms, and #deadchannel never reach it.
- Sections: `reading` (yesterday's News shares via `Article::list_shared_between`, `quiet` when nobody shared) and `outside` (grounded `generate_reply`; the user turn carries the date anchor and the last `PAPER_OUTSIDE_MEMORY_EDITIONS` printed Outside pages as "already covered" so a slow week does not repeat; AI news is rationed to one line and only when enormous; a `NOTHING` answer settles `quiet`). `outside` prints only while `paper_outside_enabled` is on.
- Switches (`app_flags`, closed `AppFlag` enum): `paper_enabled` (kill switch) and `paper_outside_enabled` (the Outside page), both seeded on. `/paper on|off` and `/paper outside on|off` flip them, admin only, through `AppFlagService::set_task` like `/haunt`.
- Admin press hooks, banners with a tally when done: `/paper print` runs today's sweep now (`print_edition`, shared with the sweeper); `/paper preview` lays out tomorrow's edition over today so far **in memory** (`preview_edition`: same printers and threshold, no claims, no rows) and opens it as the caller's modal only, so the midnight sweep prints the real edition over the whole day and no other reader ever sees a draft; `/paper reset` deletes today's rows and the caller's `paper_shown_on` stamp so both the print and the login pop can be seen again. `note_room_print` / `note_section_print` are the one place a print outcome becomes a tally line, a metric, and a log line.
- Clients are scoped to each query; nothing holds a pooled connection across the model call.

## The newsstand

- `request(user_id, trigger)` loads today's rows, and only today's, plus the covered day's `#announcements` posts (`read_announcements`: `ChatRoom::find_public_non_dm_by_slug` then `ChatMessage::list_public_room_between_with_author` over the edition window, oldest first, the newest `PAPER_ANNOUNCEMENTS_LIMIT` = 50; no room means no announcements, not an error). `Login`: if the edition has any `ready` page, or an announcement over an edition the sweeper has already reached (`is_swept`: any row at all, quiet ones included), `User::claim_paper_shown` stamps `users.settings.paper_shown_on = edition`; an announcement over an unswept edition answers `Empty` and spends nothing, so a reader in at 00:02 is not stamped over a paper whose columns are minutes away. A lost claim (other device, other replica) sends nothing. `Command` (`/paper`) always answers and costs nothing. A ready answer is a `PaperIssue`: the edition's rows, the announcements, the wall.
- The session arms `login_pop_pending` for everyone with the `paper_at_login` tweak on (Ctrl+O Tweaks → Startup → "Daily paper at login", default on), newcomers included: for them the paper is the answer to "is anyone here?" and, since a new account is only in the auto-join rooms, mostly an Elsewhere list with `/join` hints. `tick` fires it only once the opening sequence is over: splash down and the clubhouse tour settled (`clubhouse::state::State::tutorial_settled`, which treats an armed-but-not-started tour as unsettled so the modal never lands over the walkthrough's key capture).
- Layout (`lay_out`): byline, ANNOUNCEMENTS (each post as `@author · HH:MM` then its body lines as written, no markdown pass; the section is absent on a day the operator said nothing), YOUR ROOMS in rail order (favorites first, from `ChatState::visual_order`), ELSEWHERE ON LATE.SH (public rooms you are not in, bumped rooms first, top `PAPER_ELSEWHERE_LIMIT` = 3, with a `/join` hint on topic rooms only, since `/join #<code>` would open a new topic room rather than the language room), WHAT WE WERE READING, OUTSIDE, ON THE WALL (yesterday's most applauded gallery pieces, best first, up to `PAPER_WALL_PIECES` = 3, loaded in `open` with no claim since they are rows already; that is the whole rule, no applause floor and no line budget, decided 2026-09-06; each glyph prints in the colour it was painted (`gallery::ui::piece_paint_lines` to `PaperInk::Paint`, unpainted glyphs as `Body`); a piece that fails to decode is dropped, not fatal; a faint line under the pieces points at page 4), then a footer naming quiet rooms, rooms still at the press, and rooms that missed it (`failed` at the cap). Nothing in the layout is per reader beyond ordering and membership.
- The modal sits above every other overlay in input and render order; a ready paper waits in `pending_modal` while a newcomer's tour holds the keys. `/paper preview` carries today's announcements so far the same way (`PressOutcome::Previewed.announcements`), with no wall.
- `/paper` shows an "at the press…" modal until the rows land; `Esc` on it drops the request (`awaiting` cleared), so a late answer never pops over something else.

## Telemetry

`record_paper_print(PaperPrintResult)` per page (printed / quiet / lost / failed) and `record_paper_open(PaperOpenResult)` per request (login / command / empty / already_shown / unavailable / failed). Print failures log through `late_core::error_span!` with the edition and room.

## Tests

`state_test.rs` (whole-modal layout assertion with announcements at the top, command parsing), `ui_test.rs` (the modal drawn under one theme after being built under another), `svc_test.rs` (window math, column tidying, the newsstand's claim path against a real DB including an announcement-only paper that pops at login only once the edition is swept, the login pop with the announcement above the columns and `/paper` driven through a full `App`), `late-core/src/models/paper_test.rs` (claims, reclaim, finish, sections, candidates with `#announcements` skipped), `chat_message_test.rs` (the announcements window query), `user_test.rs` (the shown stamp).

## Gotchas

- The reading and outside prompts inherit `GRAYBEARD_PERSONA` from `app/ai/ghost.rs`, whose chat rules say never to name people; the column rules are appended after it and explicitly override that, because a paper that names nobody is useless.
- A room made private after its page printed keeps the row and loses the reader: `PaperEdition::load` joins on `visibility = 'public'`.
- The grounded path is prompt-enforced only (see `AiService::generate_json_with_search`); `tidy_column` is what makes the Outside reply safe to render.
- **Lines carry ink, never colour.** `theme`'s palette lives in a thread
  local that `App::render` sets from the reader's profile, but the modal is
  built during `tick`, which runs earlier in the same step
  (`ssh.rs::render_once`: input, then `tick`, then `render`) and on whatever
  tokio worker thread the session woke on. `lay_out` reading the palette
  meant the paper printed in whichever session last rendered on that thread,
  so `/paper` came up a different colour almost every time. `lay_out` now
  emits `PaperInk` and `ui.rs` resolves it in the draw. Anything else that
  builds styled spans off the render pass has the same bug: the chat
  `/members` overlay had it too, and carries the same cure
  (`common/overlay.rs::OverlayInk`).
