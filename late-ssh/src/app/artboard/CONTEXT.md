# Artboard Context

## Scope

`late-ssh/src/app/artboard` implements the interactive shared ASCII Artboard page for late.sh. It owns per-session UI state, keyboard/mouse routing, rendering overlays, local editor integration, archive browsing from the rail, attribution display, and edit-ban display/activation integration; the actual ban gate lives in `App::activate_artboard_interaction`.

It does not own the process-wide board server or the durable persistence loop. Those live in `late-ssh/src/dartboard.rs`, but they are documented here because the Artboard page depends on their lifecycle.

Naming note: `Artboard` is the user-facing name. Code and upstream crates still use `dartboard` heavily (`src/dartboard.rs`, `dartboard_core`, `dartboard_local`, `dartboard_editor`, `dartboard_tui`). Search both names.

## High-Level Model

- Top-level screen: `Screen::Artboard`, key `4`, also reachable through `Tab` / `Shift+Tab`.
- Shared canvas: `dartboard_core::Canvas`, canonical size `384 x 192`.
- Server: one in-process `dartboard_local::ServerHandle` per `late-ssh` process.
- Session connection: created lazily when the user enters Artboard; dropped when leaving Artboard.
- Initial mode: `view`; `i`, `I`, `Enter`, or canvas left-click enters active edit mode.
- Persistence: JSONB rows in `artboard_snapshots` through `late_core::models::artboard::Snapshot`.
- Public gallery: `late-web/src/pages/gallery/`, read-only over saved DB snapshots, not live server memory. It does not list pieces yet.
- The gallery (`gallery/`): pieces hung off the live board, applause, the monthly `artboard` award, the page rail. See §Gallery below.

Only canvas mutations are shared. Editor affordances stay local to the current SSH session.

Shared state:
- Canvas contents
- Peer list
- Assigned peer color
- Sequence/ack progress
- Connect rejection state
- Per-cell authorship provenance

Local state:
- Cursor and viewport origin
- Selection anchor and shape
- Floating brush / floating selection preview
- Swatches and pin state
- Selected local paint color
- Temporary sampled glyph brush
- Help tab and scroll
- Glyph picker search state
- Archive browser state (`ArchiveBrowser`: per-kind key lists, the wanted/in-flight/active archive, a small decoded cache)
- Private notices

## File Map

- `late-ssh/src/app/artboard/mod.rs`
  - Public module declarations only: `data`, `gallery`, `input`, `page`, `provenance`, `state`, `svc`, `ui`.

- `late-ssh/src/app/artboard/gallery/` (the gallery subdomain, same file roles)
  - `frame.rs`: pure. `frame_piece(canvas, provenance, bounds, username)` crops the frame into a `FramedPiece` (own canvas, cropped provenance, glyph count, own share, credits, content hash) or a `FrameError` with its notice.
  - `svc.rs`: `GalleryService` (db + `app_flags` watch + the process-wide splash `watch`), the spawned tasks (`list_task`, `hang_task`, `applaud_task`) reporting `GalleryResult` over the session channel, `refresh_splash` / `start_splash_refresh_task` (publishes `None` while the switch is off; the paper reads `ArtboardPiece::most_applauded_hung_on` itself, behind the same switch). Owns the gallery's logs and metrics.
  - `state.rs`: `GalleryState`: rail rows and focus, the four listings, the hang flow (`HangFlow`), notices, the draw-published rects for hit tests, `tick()` draining results.
  - `input.rs`: keys and mouse while the gallery claims input, the archive list in the rail included; returns `GalleryAction` (`FocusBoard` / `BeginHang` / `OpenArchive(kind)` go back to `page.rs`).
  - `ui.rs`: the rail, the listing pane (list + preview), the full-frame piece, the hang modal, the framing bar, `draw_splash_piece`, `piece_text_lines` (the paper).

- `late-ssh/src/app/artboard/data.rs`
  - Static help text for the Artboard help overlay.
  - Documents core controls, local vs shared state, swatches, glyph picker, session behavior, and snapshots.

- `late-ssh/src/app/artboard/provenance.rs`
  - Tracks per-glyph owner usernames in `ArtboardProvenance`.
  - Serializable wire form is sorted `{ cells: Vec<(Pos, String)> }`.
  - `username_at(canvas, pos)` resolves wide glyph continuations through `Canvas::glyph_origin`.
  - Applies attribution updates for `CanvasOp::PaintCell`, `ClearCell`, `PaintRegion`, row/column shifts, and `Replace`.
  - Defines `SharedArtboardProvenance = Arc<Mutex<ArtboardProvenance>>`.
  - Uses `late_core::MutexRecover` for poison-tolerant shared locking.

- `late-ssh/src/app/artboard/svc.rs`
  - Per-session bridge around `dartboard_local`.
  - `DartboardService::new` connects to the shared `ServerHandle`, spawns a named OS client thread, and exposes:
    - `watch::Receiver<DartboardSnapshot>` for canvas/provenance/peers/session identity.
    - `broadcast::Receiver<DartboardEvent>` for ack/reject/peer/connect events.
    - `submit_op(CanvasOp)` for local edits.
  - Stores rejected connections on `DartboardSnapshot.connect_rejected` because rejection can happen before subscribers exist.
  - `ArtboardSnapshotService` and `ArtboardArchiveLoader` serve the rail's archive lists in two cheap steps: `request_list(kind)` fetches one kind's keys (`Snapshot::list_summaries_by_board_key_prefix`, no JSON) as `ArtboardArchiveEntry { board_key, kind, label, updated }`; `request_load(kind, key)` fetches and decodes one row into `ArtboardArchiveSnapshot { board_key, kind, label, canvas, provenance }` off the render path. Results arrive as `ArtboardArchiveResult::{Listed, ListFailed, Loaded, LoadFailed}`; failures are logged in the task.

- `late-ssh/src/app/artboard/state.rs`
  - Main per-session Artboard state.
  - Wraps `dartboard_editor::EditorSession` for cursor, viewport, selection, swatches, floating brush, edit actions, and pointer behavior.
  - Maintains local-only state: brush, drag brush, paint color, help overlay, glyph picker, hover position, the archive browser, swatch preview suppression.
  - `tick()` drains archive loader results, live `watch` snapshots, and service events.
  - Local mutations use `edit_canvas` or `submit_canvas_diff`: diff local canvas changes into `CanvasOp`, update local/shared provenance, then submit to the service.
  - Archive view is read-only; edit paths refuse to submit while `archives.active` is set.
  - Archive browsing (`open_archive_list`, `archive_move`, `close_archive_list`, `exit_archive_view`): the cursor in the rail's list sets `wanted`; `request_wanted_archive` serves it from the board, then the cache (`ARCHIVE_CACHE_SIZE` = 8), then one fetch at a time; a landed fetch re-checks `wanted` so a cursor that moved on is caught up with. Esc keeps the archive on the board; the Board row (`exit_archive_view`) restores live.
  - Owner overlay renders a derived canvas replacing each glyph with owner initials/colors.

- `late-ssh/src/app/artboard/input.rs`
  - Active-mode input handling for raw bytes, parsed events, arrows, mouse, help overlay, glyph picker, swatches, brush stamping, paste, and clipboard effects.
  - Converts raw C0 controls into `dartboard_editor::AppKey` where appropriate.
  - Returns `InputAction::{Ignored, Handled, Copy, Leave}` for app-level integration.
  - Mouse hit testing routes swatch/info overlays before canvas pointer dispatch.
  - Double-clicking a canvas glyph arms a temporary glyph brush.
  - Glyph picker owns input while open.

- `late-ssh/src/app/artboard/page.rs`
  - Page-level integration with `crate::app::state::App`.
  - Distinguishes view mode from active Artboard interaction.
  - View mode supports cursor movement, page/home/end, Alt-arrow panning, right-drag pan, `Ctrl+P` local help (`?` is the global guide), Esc to the rail, and `i`/Enter activation.
  - Active/help/glyph modes delegate to `input.rs`; the rail, listings, archive lists, and the hang flow to `gallery/input.rs`, whose `Ignored` falls through to the view-mode keys so `i` and the Ctrl keys work from the rail; framing and the title prompt let Ctrl+P through too.
  - Converts `InputAction::Copy` into `app.pending_clipboard` and `InputAction::Leave` into edit-mode deactivation.

- `late-ssh/src/app/artboard/ui.rs`
  - Rendering for canvas, info sidebar, swatch strip, help overlay, glyph picker, owner overlay, floating preview, and selection; `draw_game` lays the rail and the gallery pane around them in view mode.
  - Uses `ratatui`, `dartboard_tui`, and app theme helpers.
  - `canvas_area_for_state(size, rail_visible)` must match the frame layout; hit tests and the editor viewport depend on it. The rail's visibility is published by the draw path (`GalleryState::set_rail_visible`), so input math follows the last frame.
  - `render_piece_canvas` draws any piece canvas in the board's style; every gallery surface goes through it.
  - Custom canvas rendering preserves wide glyph behavior and avoids cursor/overlay collisions.

- `late-ssh/src/dartboard.rs`
  - Process-wide server/store/persistence wrapper.
  - Defines canvas constants, server spawning, persisted load, explicit flush, live snapshot capture, autosave, daily snapshots, monthly snapshots, curated snapshot keys, and live-board blanking.

## Lifecycle

1. `late-ssh/src/main.rs` loads the last persisted Artboard row from Postgres with `late_ssh::dartboard::load_persisted_artboard`.
2. Startup initializes shared provenance from the persisted row or an empty `ArtboardProvenance`.
3. Startup spawns the process-wide persistent server with `spawn_persistent_server`.
4. Session bootstrap loads active `ArtboardBan` state. `SessionConfig` carries the shared `dartboard_server`, shared provenance, `ArtboardSnapshotService`, and ban state into every SSH `App`.
5. `App::set_screen(Screen::Artboard)` calls `enter_dartboard()`.
6. `enter_dartboard()` creates a per-session `DartboardService` and `artboard::state::State`, then switches the terminal cursor to steady underline.
7. `DartboardService::new` calls `ServerHandle::try_connect_local`.
8. Accepted clients spawn a per-session OS thread that polls local commands about every 16ms, submits `CanvasOp`s, drains `ServerMsg`s, updates `watch`/`broadcast`, and applies provenance.
9. `App::tick()` calls `dartboard_state.tick()`, which updates local state from service channels unless an archive view is active.
10. `App::leave_dartboard()` drops the local state/client and restores the normal block cursor.

Connection overflow is handled by upstream `dartboard_local::MAX_PLAYERS`. Overflow sessions get `DartboardSnapshot.connect_rejected`; no client loop starts, and later `submit_op` calls are ignored.

## Persistence And Archives

Primary model:
- `late-core/src/models/artboard.rs`
- Table: `artboard_snapshots`
- Columns: `board_key`, `canvas`, `provenance`
- Main board key: `Snapshot::MAIN_BOARD_KEY`, value `main`

Migrations:
- `late-core/migrations/029_create_artboard_snapshots.sql` creates the table with `board_key UNIQUE` and `canvas JSONB NOT NULL`.
- `late-core/migrations/030_add_artboard_provenance.sql` adds `provenance JSONB NOT NULL DEFAULT '{"cells":[]}'`.

Runtime behavior in `late-ssh/src/dartboard.rs`:
- Boot restores `main` if present; otherwise starts with a blank `384 x 192` canvas.
- Canvas saves are coalesced and persisted in a background thread every 5 minutes while dirty.
- `flush_server_snapshot()` persists immediately and is used during shutdown.
- The persistence loop requires an active Tokio runtime at construction. Without one, persistence is disabled with a warning.
- Failed saves mark the state dirty again and retry.

Archive behavior:
- Daily key: `daily:YYYY-MM-DD`.
- Daily rollover wakes at each UTC day boundary and archives the previous UTC day.
- Daily retention keeps the newest 7 daily snapshots.
- Monthly key: `monthly:YYYY-MM`.
- On the first UTC day of a month, rollover saves the prior month from the archived prior-day daily snapshot, clears shared provenance, submits a system `CanvasOp::Replace` blanking the live server canvas, and persists a blank `main`.
- Rollover retries the same pending day every 30 seconds on failure instead of advancing.
- Curated key: `curated:YYYY-MM-DD`; duplicate curated snapshots for the same date use `curated:YYYY-MM-DD-N`.
- `/mod artboard curate YYYY-MM-DD [reason...]` copies `daily:YYYY-MM-DD` into the first available curated key without regenerating the daily snapshot.
- `/mod artboard curate live [reason...]` flushes the current live server canvas plus shared provenance into `main`, then copies `main` into the first available curated key for the current UTC day.
- `/mod artboard restore [YYYY-MM-DD] [reason...]` restores live `main` from the daily snapshot for that UTC date, defaulting to previous UTC day. It copies current `main` to `restore-backup:main:<timestamp>:<uuid>` when present, writes audit/event metadata, replaces the live server canvas/provenance, and persists restored `main`.

Gallery behavior:
- `late-web/src/pages/gallery/` reads saved `artboard_snapshots` rows directly.
- It lists `main`, `daily:*`, `monthly:*`, and `curated:*`.
- It renders a selected saved snapshot and exposes persisted provenance for hover/cell ownership.
- The web page does not expose raw DB JSON to JS. It decodes `Canvas`/provenance server-side and emits compact snapshot JSON: `cells` entries are `[x, y, ch, width, fg, author_index]`, with wide continuations mapped client-side for hover; `authors` is a de-duplicated username array.
- The `main` gallery entry is the latest saved DB row, not a live `ServerHandle` stream, so it can lag active drawing by the persistence interval.

## Gallery

What it is: a piece is an immutable crop of the live board, taken when the hanger frames it. The monthly wipe and the next vandal cannot touch it. Pieces gather applause; the month's best pieces win the `artboard` profile award and chips.

The rail (view mode): `Board` (the live board or the archive being viewed), a GALLERY group (`This month`, `New`, `Hall of fame`, `Mine`), `Hang a piece`, then an ARCHIVES group last (`Daily`, `Monthly`, `Curated`), in a `RAIL_WIDTH + 1` column whose right edge is the full-height rule every page rail has. The gallery rows and `Hang a piece` exist only while `artboard_gallery_enabled` is on. The rail shows while it, a listing, a piece, or an archive list has focus, and folds away when the board takes the keys (Enter on `Board`, `i`, a board click, framing); Esc on the board brings it back. The page lands on the rail with `Board` selected. The numbers next to the rows are there on entry: `ArtboardPiece::listing_counts` (one query, no canvases; refreshed after a hang) for the gallery rows until a listing loads and its own length takes over, and the archive key lists, requested for all three kinds in `State::new`.

Focus (`gallery::state::Focus`): `Rail` (the landing), `Canvas` (the board cursor), `List` (a listing's pieces), `Piece` (one piece full frame), `Archive` (an archive kind's key list drawn in the rail's place). Esc on the board focuses the rail; Enter on a gallery row moves into its list; Enter on an archive row turns the rail into that kind's list; Esc walks back out to the rail; Esc on the rail is not the page's (global Esc); Tab backs out of a pane to the rail (on the rail and the board Tab stays the page switch). There is no `g` key. Arrow keys follow focus. Mouse: a click on a rail row selects and activates it, a click on a list row selects it, the wheel scrolls the list.

Archives from the rail: the list shows keys only (one summary query per kind, fired on entry, newest first, the count on the rail row); the key under the cursor loads in the background and replaces the board as it lands (`Mode` reads `snapshot`, the Board row's tail reads `archive`, the list marks the loading key with `…` and the one on the board with `•`). Enter on a key moves focus to the board with the archive up; Esc goes back to the rail rows and leaves the archive up; the Board row returns to live.

Hanging (`HangFlow`): `Hang a piece` (the rail is the only way in) runs `App::begin_artboard_hang`: the artboard ban gate, the switch, and "not an archive". Then `Framing`: Shift+arrows or a left drag select on the board, Enter runs `State::frame_selection_for_hang` (`frame_piece`), Esc cancels. A refused frame stays in framing with the reason on the framing bar. Then `Confirm`: the modal shows the crop, its numbers, and the credits; type a title (`PIECE_TITLE_MAX_CHARS` = 40), Enter hangs, Esc cancels. `Submitting` waits for the row. A landed hang selects `Mine` and says so in the pane's notice line.

Local rails (`frame.rs`, from `late_core::models::artboard_piece` constants): at least `PIECE_MIN_GLYPHS` = 40 non-blank glyphs, at most `PIECE_MAX_WIDTH` x `PIECE_MAX_HEIGHT` = 100 x 40, at least `PIECE_MIN_OWN_SHARE_PERCENT` = 75 of the glyphs painted by the hanger per cell provenance (wide glyphs count once, at their origin; one whose second half is outside the frame is not in it). SQL rails (`ArtboardPiece::hang`): `PIECE_DAILY_CAP` = 3 per UTC day, counted in the insert's own guard, and `UNIQUE (content_hash, period_month)`: the same glyphs at the same relative positions, colours ignored, cannot hang twice in a month. Copy theft beyond that is a mod's call (`/mod artboard remove`).

Applause: `v` on a piece in a list or full frame. One per person per piece (`artboard_piece_votes` PK), free, `v` again withdraws it, never on your own piece (CHECK on the denormalized `author_user_id`, and the state refuses before the round trip). One applause in flight per session.

Month end (`late-core/src/models/profile_award.rs`): the `artboard` category ranks hangers by their best piece's applause over their pieces of the month (`period_month`, the UTC month hung), only pieces at or over `GALLERY_AWARD_MIN_APPLAUSE` (3). The rank is `ROW_NUMBER` over applause then earliest hang, never `RANK`: this is the one arm that mints chips, and a tie must not pay two first prizes; the tiebreak is the one `previous_month_winner` and the hall of fame use, so the splash's winner is the `ART1` holder. Ranks 1-3 print `ART1`-`ART3` and pay `gallery_prize_chips` (10,000 / 5,000 / 1,000) as `ChipMove::ArtboardPrize` inside the snapshot transaction, keyed off the insert's `RETURNING` rows. **The month is settled once**: applause keeps moving after the rollover (`v` on last month's pieces, and withdrawals), so the arm carries `NOT EXISTS (artboard row for the period)` and ranks nobody once any row exists; without that a hanger who climbed into the top 3 on the 24h re-run would be inserted past `ON CONFLICT` and paid. Applause is therefore counted once, by the first pass within the hour after the rollover.

Where a piece shows up beyond the page: the splash (`gallery::ui::draw_splash_piece`, last month's winner from the process-wide `GalleryService` splash `watch`, refreshed hourly, the coffee cup when it does not fit); The Late Edition's ON THE WALL column (yesterday's most applauded piece, plain glyphs); the profile's Artboard gallery line (`ArtboardPiece::counts_for_user`); chat labels and the badge legends through the award machinery.

Moderation: `/mod artboard remove <id-prefix> [reason]` (`RESTORE_ARTBOARD` cap; the first 13 characters of the id are printed on the key line of the full-frame view, `gallery::ui::piece_id_prefix`; at least `PIECE_ID_PREFIX_MIN_CHARS` = 8 characters; must match exactly one piece; hard delete, applause cascades, audit row keeps the title) and `/mod artboard gallery on|off` (admin; the `app_flags` row, so every replica follows).

Telemetry: `record_gallery_hang(GalleryHangResult)` (hung / daily_cap / duplicate / failed) and `record_gallery_applause(GalleryApplauseResult)` (applauded / withdrawn / own_piece / not_found / failed), both from `gallery/svc.rs`; failures log through `late_core::error_span!`.

Tests: `gallery/frame_test.rs` (crop, credits, hash, the three local rails), `gallery/state_test.rs` (rail rows and focus, the hang flow's title rule), `late-core/src/models/artboard_piece_test.rs` (applause rules, daily cap and duplicate in SQL, mod lookup and removal), `late-core/src/models/profile_award_test.rs::the_gallery_award_ranks_best_pieces_and_pays_once`, `app/input_flow_test.rs::artboard_gallery_hangs_a_framed_piece_from_the_rail` (paint, rail, frame by drag, name, hang, back out), `moderation/command_test.rs::parses_artboard_gallery_commands`.

Not done: the web `/gallery` page does not list pieces.

## Input Model

Artboard has two main interaction modes plus archive viewing:

- `view`: inspect board, move cursor/viewport, keep global page switching (`1-7`, `Tab`, `Shift+Tab`) available.
- `active`: edit board; single-key globals and reserved global control chords are suppressed so typing/control input goes to the canvas/editor.
- `snapshot`: read-only historical daily/monthly/curated archive view. Reached from the rail's ARCHIVES rows; the key under the list cursor replaces the local snapshot until the Board row returns live.

Important routing:
- `Esc` closes transient Artboard overlays first, then clears floating brush / sampled brush / selection in active mode, then returns to view mode. `q` also closes the Artboard help guide, a full-frame piece, and an archive list before global quit handling can run.
- Active Artboard editing blocks global quit.
- View mode does not claim global page switching unless help/glyph picker/active interaction is open.
- Archive views cannot enter active mode and edit paths refuse to submit changes.

Keyboard reference:

| Action | Keys / Mouse | Notes |
| --- | --- | --- |
| Open Artboard | `4`, `Tab`, `Shift+Tab` | Dedicated top-level screen; entering connects a local client |
| Move in view mode | Arrows, `Home`, `End`, `PgUp`, `PgDn`, mouse wheel | Inspect/pan without drawing |
| Pan viewport in view mode | `Alt+arrows`, right-drag | Moves viewport without moving the cursor for Alt-arrows |
| Enter active mode | `i`, `I`, `Enter`, canvas left-click | Disabled for archive snapshots |
| The rail | landing, `Esc` from the board | `j/k` or arrows move, `Enter` opens (Board, a listing, an archive list, hang), `Esc` from a pane or the board back to the rail, `Tab` from a pane back to the rail; the rail folds while the board has the keys |
| Archives | rail rows `Daily` / `Monthly` / `Curated` | The rail becomes the key list; `j/k`, arrows, wheel, `PgUp`/`PgDn`, `Home`/`End` move and the board shows the key under the cursor; `Enter` to the board, `Esc` back to the rail (archive stays up), Board row returns live |
| Hang a piece | rail row `Hang a piece` | Shift+arrows or left drag frame the board, `Enter` names it, `Enter` hangs, `Esc` cancels |
| Applaud a piece | `v` | In a gallery list or full frame; `v` again withdraws |
| Draw / erase active mode | printable chars, `Space`, `Backspace`, `Delete` | Plain typing edits the shared canvas |
| Paint color | `Ctrl+U`, `Ctrl+Y` | Local 16-color palette; separate from peer color |
| Select | `Shift+arrows`, mouse drag | Local selection only |
| Shape ops | `Ctrl+T`, `Ctrl+B`, `Ctrl+Space` | Flip selection corner, draw border, smart-fill |
| Copy / cut to swatch | `Ctrl+C`, `Ctrl+X` | Fills swatch strip; does not sync to peers |
| Activate swatch brush | click swatch, `Ctrl+A/S/D/F/G` | Slots 1-5 on home row |
| Stamp floating brush | `Enter`, `Ctrl+V` | Brush stays active |
| Stroke floating brush | `Ctrl+Shift+arrows` | Repeated stamps while moving |
| Toggle brush transparency | activate same swatch again | Floating preview reflects transparency |
| Glyph picker | `Ctrl+]` | Searchable emoji / Unicode picker |
| Help | `Ctrl+P` | Four tabs: Overview / Drawing / Brushes / Session; `?` is the global guide in view mode, a glyph in edit mode |
| Ownership overlay | `Ctrl+\` | Renders owner initials with deterministic colors |
| Leave edit mode | `Esc` | Also closes help/glyph picker/local transient state first |
| Leave Artboard page | `1-7`, `Tab`, `Shift+Tab` | Available from view mode; blocked while active/help/glyph picker is open |

Mouse-specific extras:
- Click swatch pin icon to pin/unpin a swatch.
- `Ctrl+click` a swatch body clears that swatch slot.
- Double-click a non-space canvas glyph samples it into a temporary one-glyph brush.
- Mouse wheel over the info overlay is swallowed so it does not pan the board underneath.

## Rendering Notes

- Artboard has a dedicated renderer; it does not use the generic arcade game frame/sidebar.
- `ui.rs` renders the canvas, info sidebar, swatches, notices, help overlay, and glyph picker; `gallery/ui.rs` the rail (rows or an archive list), the listing pane, the piece, and the hang surfaces.
- The info sidebar shows mode, cursor/cell, owner, local paint color, brush status, selection, and peers.
- The ownership overlay changes only canvas rendering. `Owner` / `Cell` rows stay visible in the info sidebar either way.
- Cursor rendering uses the wide glyph origin for continuation cells.
- Swatch layout deliberately keeps the bottom canvas row visible and avoids overlapping the info block/notice row.

## Tests

Primary DB-backed tests (adjacent `_test.rs` files in this directory):
- `test_support.rs` contains shared cfg(test) helpers.
- `svc_test.rs` covers shared canvas sync, provenance attribution, peer join/leave, overflow rejection, unknown/system replace provenance resync, persistent save/restore, explicit flush, daily prune, and monthly rollover blanking.
- `state_test.rs` covers multiline paste and the archive browser: keys list newest first without canvases, the cursor loads one board, read-only, the cache on the way back, Esc versus the Board row, curated names.

Related tests:
- `late-ssh/src/app/input_flow_test.rs` covers Artboard screen switching, the rail landing, active-mode global hotkey blocking, `Ctrl+C` copy behavior, local help routing, active `?` drawing behavior, and `artboard_archives_time_travel_from_the_rail` (Daily from the rail, the key lands on the board, Tab and the Board row bring live back).
- `late-core/src/models/artboard_test.rs` covers snapshot upsert replacement, uniqueness, special/daily/monthly archive listing, insert-if-absent, prefix listing, and delete by board key.

Inline module tests:
- `provenance.rs`: paint/clear provenance and replace retagging.
- `state.rs`: coordinate conversion, owner initials/colors, help scroll, floating/selection behavior, paste cursor logic, swatch/glyph behavior.
- `input.rs`: mouse routing, raw control mapping, swatch interactions, double-click glyph brush, help/glyph picker routing, selection, paste/stamp behavior.
- `page.rs`: view-mode right-drag pan, non-canvas right-click handling, Alt-arrow pan.
- `ui.rs`: canvas layout, info/sidebar layout, help tabs/hit tests, swatch boxes, wide glyph cursor origin, the rail-aware board area.

## Key Invariants

- Live board dimensions are `384 x 192`.
- One shared `ServerHandle` exists per `late-ssh` process.
- Users connect to the shared board only while on the Artboard screen.
- Dropping the per-session `DartboardService`/state must free that local client slot.
- Canvas and provenance are persisted together.
- Provenance uses usernames, not stable user UUIDs.
- `ArtboardProvenance` keys are glyph origins, not every occupied cell.
- Provenance for shifts/replaces must be applied against the pre-op canvas.
- Unknown actor `CanvasOp::Replace` does not invent attribution; it reloads cloned shared provenance.
- Archive view is read-only and must not be overwritten by live watch updates during `State::tick()`.
- Active artboard bans block editing through `App::activate_artboard_interaction` and show an error banner while the ban is active; viewing and archive browsing remain available.
- One archive fetch is in flight per session; `wanted` is the only thing the cursor writes, and `request_wanted_archive` is the only reader.
- Swatch slot `0` is the primary clipboard slot and is not pinnable.
- Local paint palette is separate from the server-assigned peer color.
- Connection rejection lives on `DartboardSnapshot.connect_rejected`, not only on events.

## Fragile Areas

- Provenance concurrency uses `Arc<Mutex<_>>`; local optimistic edits and server broadcasts both touch provenance. Ordering mistakes can misattribute cells.
- Monthly rollover uses system user/client IDs `0`; actor lookup can fail intentionally and should fall back to cloned shared provenance.
- Wide glyph handling affects cursor rendering, selection coverage, double-click sampling, provenance, swatches, and ownership overlay.
- `diff_canvas_op` abstracts many editor mutations into server ops; editor changes can affect sync granularity and provenance application.
- Archive lists are keys only and one canvas loads at a time, so retention can grow without the page paying for it; the per-session cache is bounded (`ARCHIVE_CACHE_SIZE`).
- UI hit testing depends on exact layout math shared by `ui.rs`, `input.rs`, and `page.rs`.
- SGR mouse coordinates are 1-based at the parser boundary; Artboard hit tests assume normalized coordinates from app input.
- Global input integration can regress if `artboard_blocks_global_page_switch` stops considering active/help/glyph states, or the gallery's `captures_typing` (a title being typed, a frame being drawn).
- The rail shifts the board 21 columns right while it is up; anything that computes a board cell from a screen point must go through `canvas_area_for_state` with the last draw's rail visibility, never `canvas_area_for_screen`.
