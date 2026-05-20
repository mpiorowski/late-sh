# Artboard Context

## Scope

`late-ssh/src/app/artboard` implements the interactive shared ASCII Artboard page for late.sh. It owns per-session UI state, keyboard/mouse routing, rendering overlays, local editor integration, snapshot browsing, attribution display, and edit-ban enforcement.

It does not own the process-wide board server or the durable persistence loop. Those live in `late-ssh/src/dartboard.rs`, but they are documented here because the Artboard page depends on their lifecycle.

Naming note: `Artboard` is the user-facing name. Code and upstream crates still use `dartboard` heavily (`src/dartboard.rs`, `dartboard_core`, `dartboard_local`, `dartboard_editor`, `dartboard_tui`). Search both names.

## High-Level Model

- Top-level screen: `Screen::Artboard`, key `4`, also reachable through `Tab` / `Shift+Tab`.
- Shared canvas: `dartboard_core::Canvas`, canonical size `384 x 192`.
- Server: one in-process `dartboard_local::ServerHandle` per `late-ssh` process.
- Session connection: created lazily when the user enters Artboard; dropped when leaving Artboard.
- Initial mode: `view`; `i`, `I`, `Enter`, or canvas left-click enters active edit mode.
- Persistence: JSONB rows in `artboard_snapshots` through `late_core::models::artboard::Snapshot`.
- Public gallery: `late-web/src/pages/gallery/`, read-only over saved DB snapshots, not live server memory.

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
- Snapshot browser state
- Private notices

## File Map

- `late-ssh/src/app/artboard/mod.rs`
  - Public module declarations only: `data`, `input`, `page`, `provenance`, `state`, `svc`, `ui`.

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
  - `ArtboardSnapshotService` and `ArtboardArchiveLoader` list daily/monthly archive snapshots asynchronously from DB.
  - Archive rows decode into `ArtboardArchiveSnapshot { board_key, kind, label, canvas, provenance }`.

- `late-ssh/src/app/artboard/state.rs`
  - Main per-session Artboard state.
  - Wraps `dartboard_editor::EditorSession` for cursor, viewport, selection, swatches, floating brush, edit actions, and pointer behavior.
  - Maintains local-only state: brush, drag brush, paint color, help overlay, glyph picker, hover position, snapshot browser, swatch preview suppression.
  - `tick()` drains archive loader results, live `watch` snapshots, and service events.
  - Local mutations use `edit_canvas` or `submit_canvas_diff`: diff local canvas changes into `CanvasOp`, update local/shared provenance, then submit to the service.
  - Archive view is read-only; edit paths refuse to submit while `snapshot_browser.active` is set.
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
  - View mode supports cursor movement, page/home/end, Alt-arrow panning, right-drag pan, `?` local help, `g` snapshot browser, and `i`/Enter activation.
  - Active/help/glyph modes delegate to `input.rs`.
  - Snapshot browser has its own key/event routing.
  - Converts `InputAction::Copy` into `app.pending_clipboard` and `InputAction::Leave` into edit-mode deactivation.

- `late-ssh/src/app/artboard/ui.rs`
  - Rendering for canvas, info sidebar, swatch strip, help overlay, glyph picker, owner overlay, floating preview, selection, and snapshot browser.
  - Uses `ratatui`, `dartboard_tui`, and app theme helpers.
  - `canvas_area_for_screen` must match Artboard frame layout; hit tests depend on it.
  - Custom canvas rendering preserves wide glyph behavior and avoids cursor/overlay collisions.

- `late-ssh/src/dartboard.rs`
  - Process-wide server/store/persistence wrapper.
  - Defines canvas constants, server spawning, persisted load, explicit flush, autosave, daily snapshots, monthly snapshots, and live-board blanking.

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

Gallery behavior:
- `late-web/src/pages/gallery/` reads saved `artboard_snapshots` rows directly.
- It lists `main`, `daily:*`, and `monthly:*`.
- It renders a selected saved snapshot and exposes persisted provenance for hover/cell ownership.
- The `main` gallery entry is the latest saved DB row, not a live `ServerHandle` stream, so it can lag active drawing by the persistence interval.

## Input Model

Artboard has two main interaction modes plus archive viewing:

- `view`: inspect board, move cursor/viewport, keep global page switching (`1-4`, `Tab`, `Shift+Tab`) available.
- `active`: edit board; single-key global shortcuts are suppressed so typing goes to the canvas/editor.
- `snapshot`: read-only historical daily/monthly archive view. `g` opens the browser in view mode; selecting an archive replaces the local snapshot until returning live.

Important routing:
- `Esc` closes transient Artboard overlays first, then clears floating brush / sampled brush / selection in active mode, then returns to view mode.
- `q` closes the snapshot browser when it is open; active Artboard editing blocks global quit.
- View mode does not claim global page switching unless help/glyph picker/active interaction is open.
- Archive views cannot enter active mode and edit paths refuse to submit changes.

Keyboard reference:

| Action | Keys / Mouse | Notes |
| --- | --- | --- |
| Open Artboard | `4`, `Tab`, `Shift+Tab` | Dedicated top-level screen; entering connects a local client |
| Move in view mode | Arrows, `Home`, `End`, `PgUp`, `PgDn`, mouse wheel | Inspect/pan without drawing |
| Pan viewport in view mode | `Alt+arrows`, right-drag | Moves viewport without moving the cursor for Alt-arrows |
| Enter active mode | `i`, `I`, `Enter`, canvas left-click | Disabled for archive snapshots |
| Snapshot browser | `g` | `j/k` or arrows move, `Enter` selects, top row returns live |
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
| Help | `Ctrl+P` or `?` in view mode | Four tabs: Overview / Drawing / Brushes / Session |
| Ownership overlay | `Ctrl+\` | Renders owner initials with deterministic colors |
| Leave edit mode | `Esc` | Also closes help/glyph picker/local transient state first |
| Leave Artboard page | `1-4`, `Tab`, `Shift+Tab` | Available from view mode |

Mouse-specific extras:
- Click swatch pin icon to pin/unpin a swatch.
- `Ctrl+click` a swatch body clears that swatch slot.
- Double-click a non-space canvas glyph samples it into a temporary one-glyph brush.
- Mouse wheel over the info overlay is swallowed so it does not pan the board underneath.

## Rendering Notes

- Artboard has a dedicated renderer; it does not use the generic arcade game frame/sidebar.
- `ui.rs` renders the canvas, info sidebar, swatches, notices, help overlay, glyph picker, and snapshot browser.
- The info sidebar shows mode, cursor/cell, owner, local paint color, brush status, selection, and peers.
- The ownership overlay changes only canvas rendering. `Owner` / `Cell` rows stay visible in the info sidebar either way.
- Cursor rendering uses the wide glyph origin for continuation cells.
- Swatch layout deliberately keeps the bottom canvas row visible and avoids overlapping the info block/notice row.

## Tests

Primary integration tests:
- `late-ssh/tests/artboard/main.rs` contains shared helpers.
- `late-ssh/tests/artboard/svc.rs` covers shared canvas sync, provenance attribution, peer join/leave, overflow rejection, unknown/system replace provenance resync, persistent save/restore, explicit flush, daily prune, and monthly rollover blanking.
- `late-ssh/tests/artboard/state.rs` covers multiline paste and archive browser read-only/return-to-live behavior.

Related integration tests:
- `late-ssh/tests/app_input_flow.rs` covers Artboard screen switching, active-mode global hotkey blocking, `Ctrl+C` copy behavior, local help routing, and active `?` drawing behavior.
- `late-core/tests/artboard_snapshot.rs` covers snapshot upsert replacement, uniqueness, prefix listing, and delete by board key.

Inline module tests:
- `provenance.rs`: paint/clear provenance and replace retagging.
- `state.rs`: coordinate conversion, owner initials/colors, help scroll, floating/selection behavior, paste cursor logic, swatch/glyph behavior.
- `input.rs`: mouse routing, raw control mapping, swatch interactions, double-click glyph brush, help/glyph picker routing, selection, paste/stamp behavior.
- `page.rs`: view-mode right-drag pan, non-canvas right-click handling, Alt-arrow pan.
- `ui.rs`: canvas layout, info/sidebar layout, help tabs/hit tests, swatch boxes, wide glyph cursor origin, snapshot/browser rendering helpers.

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
- Snapshot browser selection index `0` means live; archive items are offset by one.
- Swatch slot `0` is the primary clipboard slot and is not pinnable.
- Local paint palette is separate from the server-assigned peer color.
- Connection rejection lives on `DartboardSnapshot.connect_rejected`, not only on events.

## Fragile Areas

- Provenance concurrency uses `Arc<Mutex<_>>`; local optimistic edits and server broadcasts both touch provenance. Ordering mistakes can misattribute cells.
- Monthly rollover uses system user/client IDs `0`; actor lookup can fail intentionally and should fall back to cloned shared provenance.
- Wide glyph handling affects cursor rendering, selection coverage, double-click sampling, provenance, swatches, and ownership overlay.
- `diff_canvas_op` abstracts many editor mutations into server ops; editor changes can affect sync granularity and provenance application.
- Snapshot archive listing decodes full canvas/provenance JSON for every daily/monthly row; expanding retention may require pagination or summaries.
- UI hit testing depends on exact layout math shared by `ui.rs`, `input.rs`, and `page.rs`.
- SGR mouse coordinates are 1-based at the parser boundary; Artboard hit tests assume normalized coordinates from app input.
- Global input integration can regress if `artboard_blocks_global_page_switch` stops considering active/help/glyph states.
