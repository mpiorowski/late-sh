# PLAN.md — Arcade Economy & Multiplayer Roadmap

## Immediate next: Artboard (embedded dartboard) in late-sh

**Goal:** integrate `~/p/my/dartboard` into `late-sh` as a shared multiplayer drawing board reachable from the Games screen ("Artboard"), with `Ctrl+Q` as the quit key so `Esc` stays available for local selection/floating cancel.

### Dartboard interaction model (authoritative reference)

Every user-facing decision below should match this description of standalone dartboard (`~/p/my/dartboard/dartboard/src/app.rs`). Where late-sh deliberately diverges, the divergence is called out explicitly.

**Transport & identity**

- Wire types: `UserId = u64`, `ClientOpId = u64`, `Seq = u64`. Server assigns `UserId` on connect — late-sh's own `Uuid` stays local to the integration layer and never reaches the wire.
- Client → server: `Hello { name, color }`, `Op { client_op_id, op }`.
- Server → client: `Welcome { your_user_id, your_color, peers, snapshot }`, `Ack { client_op_id, seq }`, `OpBroadcast { from, op, seq }` (echoed to sender too), `PeerJoined`, `PeerLeft`, `Reject { client_op_id, reason }`.
- Color assignment: the server remaps a requested color that collides with an in-use one to the next free palette entry. In the vendored `dartboard-local` the palette is 10 entries and doubles as the seat cap — the 11th connect is a `ConnectRejected`. Clients never pick their own final color; they must read `Welcome.your_color`.
- Canvas ops (`dartboard-core::ops`): `PaintCell { pos, ch, fg }`, `ClearCell { pos }`, `PaintRegion { cells: Vec<CellWrite> }` (batched writes used for multi-cell stamps), `ShiftRow { y, kind }` / `ShiftCol { x, kind }` (whole row/column moves, not N cell writes). Conflict resolution is last-write-wins by server-assigned `Seq`; there is no CRDT.
- Per-peer cursors are **not** on the wire. `Peer` carries only `user_id`/`name`/`color`. Any cursor/selection/floating view in late-sh is strictly session-local.
- Welcome race: Remote clients must drain until `Welcome` lands before letting the user submit the first op, or `Welcome.snapshot` will stomp it. For in-proc `LocalClient` the `Welcome` is enqueued synchronously during `connect_local`, but the drain-until-welcome invariant is still the cleanest contract.

**Two top-level modes: `Draw` and `Select`**

- `Draw` is the default. Typed characters `insert_char` at the cursor and advance x by glyph display-width (wide glyphs advance 2). Backspace moves left then clears, honoring wide-glyph origin. Delete clears at cursor, snapping to the glyph origin first. Enter is a typewriter-style `move_down` — it does **not** wrap x back to column 0.
- `Select` is entered by Shift+Arrow, by mouse drag, or by lifting a selection to floating. In Select, typing a character fills the entire selection with that glyph (respecting ellipse shape). Esc clears the selection and returns to Draw.

**Selection shapes: Rect and Ellipse**

- Rect is the default; Ctrl+Left-Down starts an ellipse-shaped drag. Ellipse fill/border uses a standard `(dx/rx)² + (dy/ry)² ≤ 1` test; the border primitive draws `*` only on selected cells that have an unselected neighbor.
- Bounds are normalized against the canvas before capture so a selection that lands on a wide-glyph continuation extends outward to cover the whole glyph.

**Swatches (5-slot clipboard LRU with pinning)**

- Capacity 5. `Ctrl+C` / `Ctrl+X` on a selection (or single cell, when no selection exists) pushes a `Clipboard` into slot 0; older unpinned slots shift down; pinned slots are immune to eviction and never shift. If every slot is pinned, the push is dropped.
- Mouse click on a swatch body activates it (see below). Mouse click on a swatch's pin zone toggles pin state.
- `Ctrl+A/S/D/F/G` is a home-row shortcut for activating swatches 0..4.

**Floating selection (the "stamp" mode)**

- Activated by clicking a swatch, `Ctrl+V`, or lifting the current selection. The floating payload is a `Clipboard` (width × height × `Option<CellValue>`) anchored at the cursor; moving the cursor moves the preview.
- Two render modes: **opaque** (None cells in the clipboard erase what's under them) and **transparent** (None cells pass through). `Ctrl+T` toggles transparency. Re-activating the same swatch while it is already floating also toggles transparency; activating a different swatch replaces the float and resets to opaque.
- Commits: `Ctrl+V` stamps at the current cursor **and keeps floating active** (the preview remains — this is how "repeat stamp along a path" works). `Esc` dismisses. Typing any non-binding char while floating dismisses floating first, then inserts that char.
- Mouse while floating:
  - `Moved` → cursor follows mouse, preview moves with it.
  - `Left Down` → begin a paint stroke and stamp at cursor.
  - `Left Drag` → continue paint stroke. Pure-horizontal runs snap the brush to a `brush_width`-aligned grid from the stroke anchor so wide stamps tile without overlap. Diagonal segments fill via Bresenham with a "don't stamp more than once per `brush_width`" gate so long diagonal drags don't produce a smear.
  - `Left Up` → end paint stroke (commits the stroke to undo as one step).
  - `Right Down` → dismiss floating.
- A paint stroke is a client-side transaction: the canvas is snapshotted at stroke start (`paint_canvas_before`), ops are submitted immediately, and the snapshot is pushed to the undo stack only when the stroke ends. Late-sh v1 has no undo, so it only needs the server-side "submit immediately" half.

**Lift → move → commit / dismiss lifecycle**

- While a selection is in Select mode, `Ctrl+X`-then-`Ctrl+V` is one legitimate flow, but the idiomatic one is: lift the selection into a floating selection (the source region remains cleared while floating is in flight), move the cursor to the drop site, `Ctrl+V` to stamp or Enter/click to commit. `Esc` cancels the move and restores the original selection.
- late-sh implements this as a single `CanvasOp::PaintRegion` on commit: the region contains `Clear` writes for every source cell plus `Paint` writes for every destination cell, so the server sees one atomic move rather than a "clear + paint" pair that might interleave with a peer's op.

**Row / column shifts**

- `Ctrl+H` / `Ctrl+Backspace` = `push_left`; `Ctrl+J/K/L` = `push_down/up/right`; `Ctrl+Y/U/I` (or `Ctrl+Tab`) `/O` = `pull_from_left/down/up/right`. These emit `ShiftRow` / `ShiftCol` ops against the cursor's row or column. They are distinct from cell writes because the server applies them atomically against the canonical canvas — replaying them as N cell writes would fight concurrent peers.

**Keyboard: other bindings relevant to user↔canvas behavior**

- Plain arrows / Home / End / PageUp / PageDown → move cursor within visible bounds; leaving Select via a plain arrow clears the selection.
- Shift+Arrow → enter Select, extend to cursor. Shift+Home/End/PageUp/PageDown do the same but jump to visible-bounds edges.
- Alt+Arrow and Ctrl+Shift+Arrow → pan the viewport by 1 (no cursor move).
- Alt+C → copy current selection (or full canvas) to the system clipboard via OSC 52.
- Ctrl+Space → smart-fill the selection (`|` for vertical-line shape, `-` for horizontal, `*` for everything else).
- Ctrl+B → draw an ASCII border inside the selection; shape-aware for ellipse.
- Ctrl+T → transpose the selection's anchor and cursor corners (useful after starting a drag from the wrong corner).
- Ctrl+Z / Ctrl+R → undo / redo in standalone only. Undo is gated on "no other writers" because a local snapshot stack is incoherent under LWW multiplayer; embedded's "every peer is a local user I own" case sidesteps this. **Unbound in late-sh v1** for the same LWW reason.
- Ctrl+Q → quit.
- Ctrl+P / F1 → toggle help overlay (standalone chrome; not load-bearing for user↔canvas).
- Tab / BackTab → cycle active local user in standalone's Embedded 5-user demo. Irrelevant to late-sh because each SSH session is exactly one user.

**Emoji picker**

- Open keys: `Ctrl+]`, `Ctrl+5`, or raw GS (`\x1d`). Picker is a modal: search + tabs + keyboard-and-mouse navigation; Enter inserts the selected glyph at the cursor; Alt+Enter inserts and keeps the picker open. The picker ultimately funnels into `insert_char`, i.e. a single `PaintCell` — so downstream op plumbing is unaffected by whether late-sh adopts the picker UI or defers it. v1 may ship without the picker; the open-key reservations should still be noted so they can be added later without keymap churn.

**Mouse: outside floating mode**

- `Left Down` on canvas sets the cursor to that cell, clears any prior selection (unless modifiers say otherwise), and records `drag_origin`. Modifiers on `Left Down`:
  - `Ctrl` → start an ellipse-shaped selection drag.
  - `Alt` with an existing anchor → extend that selection to the clicked cell.
- `Left Drag` → if the mouse actually moved or we were already selecting, anchor a selection at `drag_origin` and follow the cursor.
- `Left Up` → clear `drag_origin`.
- `Right Down` on the canvas → begin viewport pan. `Right Drag` → pan. `Right Up` → end pan.
- `Scroll` (wheel) → no zoom in standalone; scroll events only matter inside the emoji picker.
- `Moved` with no button down → **no-op in standalone** outside the floating-preview case. This is a live divergence in current late-sh (see "Open questions" below).

**Bracketed paste**

- A paste is applied relative to the cursor. `\n` / normalized `\r\n` wraps x back to the column where the paste started and advances y by 1. Runs that exceed the canvas width or height are truncated, not wrapped. All emitted glyphs use the active user's color.

**Viewport**

- Cursor motion scrolls the viewport one cell at a time just enough to keep the cursor visible; there is no "recentering." Viewport origin is clamped so the viewport never extends past the canvas edge. Right-drag and Alt/Ctrl+Shift+Arrow are the only ways to pan without moving the cursor.

### Where late-sh is today

All Discovery items from the earlier plan are now either landed or superseded. What's actually true as of this branch:

- `dartboard-server` no longer sits between late-sh and the canvas. `vendor/dartboard-local` is the in-proc server + `LocalClient` with no `tokio-tungstenite` dependency, and late-sh depends on `dartboard-{core,local,tui}`. The "Phase A crate split" prerequisite is obsolete — the split is done inside the vendored tree.
- `App::enter_alt_screen` in `late-ssh/src/app/state.rs` already enables `?1000h` + `?1003h` + `?1006h` (+ `?2004h` for bracketed paste), and `leave_alt_screen` tears them down in reverse order. Drag and `Moved` events reach the VTE layer today.
- `ParsedInput` in `late-ssh/src/app/input.rs` already carries `ShiftArrow`, `AltArrow`, `CtrlShiftArrow`, `Home`, `End`, `PageUp`, `PageDown`, `Delete`, `CtrlDelete`, `AltC`, `BackTab`, `Paste(Vec<u8>)`, and `Mouse(MouseEvent)` with `kind ∈ {Down, Up, Drag, Moved, Scroll…}`, `button ∈ {Left, Middle, Right}`, SGR-decoded modifiers, and 1-based coords. Parser tests cover modifier bits and drag/move.
- The arcade selector recognizes `GameRow::Artboard = 7`; the Games hub renders a tile with live peer count.
- `late-ssh/src/app/games/dartboard/{mod,svc,state,ui,input}.rs` exist. `DartboardService` owns the `LocalClient`, spawns a dedicated thread for op submission + message drain, and publishes a `DartboardSnapshot` via `watch` plus `DartboardEvent` via `broadcast`. `ConnectRejected` is modeled on the snapshot because it fires before any caller can subscribe.
- `state.rs` implements cursor/viewport, type/backspace/delete, brush sampling from drag, `Rect`-only selection with lift / commit-as-`PaintRegion` / dismiss, bracketed paste with paste-origin x wrapping, and system-clipboard export. It drains `watch` + `broadcast` in `tick()`.

### What's actually left

Scope for the rest of this integration is now about **parity with dartboard's interaction model**, not about plumbing. The remaining gaps against the interaction spec above:

1. **Selection shapes.** Only `Rect` is implemented. Add `Ellipse` (Ctrl+Left-Down to initiate; shape-aware fill; shape-aware border).
2. **Swatches.** Not implemented. Needs the 5-slot LRU with pin, `Ctrl+C`/`Ctrl+X` to push, `Ctrl+A/S/D/F/G` home-row activation, swatch panel UI, and mouse hit-testing for body-vs-pin zones.
3. **Floating mode full semantics.** `lift_selection_to_floating` exists, but:
   - No transparency toggle (`Ctrl+T`) or opaque/transparent distinction in the floating renderer (`floating_view` hardcodes `transparent: false`).
   - `Ctrl+V` while floating should re-stamp without dismissing; currently the code path commits and exits.
   - Mouse paint-stroke while floating (brush-width snap, Bresenham diagonal) is not implemented. Current late-sh mouse drag paints with the active/drag brush, not with the floating clipboard.
   - Swatch re-activation doesn't toggle transparency.
4. **Row / column shifts.** `Ctrl+hjkl/yuio` unbound; the `ShiftRow` / `ShiftCol` ops are defined in `dartboard-core` but never emitted by late-sh.
5. **Smart fill, draw border, transpose corner.** Unbound (`Ctrl+Space`, `Ctrl+B`, `Ctrl+T`).
6. **Viewport pan.** `Alt+Arrow` and `Ctrl+Shift+Arrow` currently jump the cursor to the visible edge instead of panning (see `handle_event` in `app/games/dartboard/input.rs`). Right-drag pan is not implemented.
7. **Emoji picker.** Not implemented. Open-key reservations (`Ctrl+]`, `Ctrl+5`, GS) should be decided one way or the other before the arcade's global keymap grows more claims on those codes.
8. **Peer-presence UI.** Snapshot carries `peers: Vec<Peer>`, but the UI only shows a count. Listing connected peers with their assigned color matches standalone's help panel and makes the color-collision remap visible to users.
9. **Leave-on-last-session semantics.** `DartboardService` threads one `LocalClient` per SSH session; no explicit teardown other than `Drop`. Worth a one-liner confirming the server stays alive at peer_count=0 (it does, because `ServerHandle` is held by `App` state).

### Decisions locked in

- Embedded late-sh dartboard exits with `Ctrl+Q`; bare `Esc` remains local to the canvas (clears selection / dismisses floating).
- v1 is one shared in-proc canvas for the lifetime of the `late-sh` process. In-memory only.
- late-sh depends only on `dartboard-{core,local,tui}`; it must not transitively pull `dartboard-server` or `tokio-tungstenite`.
- Server-assigned `UserId` (`u64`) is the source of truth on the wire. late-sh's session `Uuid` + username go into `Hello` for thread naming and peer display, not onto the wire as an identity.
- Per-peer cursors stay off the wire in v1, matching dartboard's protocol.
- Undo/redo stays unbound in late-sh v1: a local snapshot stack is incoherent under LWW when other peers write, and this is the exact reason dartboard gates undo behind `undo_enabled()`.
- Floating commits that move a region use a single `PaintRegion` op containing both clears and paints, so a concurrent peer can't interleave between the source-clear and destination-paint.

### Open questions

- **`Moved` without a button.** Standalone dartboard ignores `Moved` outside the floating-preview case; current late-sh updates the cursor on every `Moved`, which doubles the cursor's responsiveness but also means hovering over the canvas constantly repositions focus. Pick one: (a) match standalone and only honor `Moved` while floating, or (b) keep the current behavior and document it as a deliberate deviation. Current code = (b); interaction spec defaults to (a).
- **Emoji picker in v1.** Ship it, or reserve the open keys and defer? Inserting a glyph is trivially `PaintCell`; the picker itself is a ~300-line modal. Deferring is fine if the open-key codes are reserved in `ParsedInput`/the global keymap now so the eventual add doesn't rebind anything.
- **Canvas widget source.** `dartboard-tui::CanvasWidget` is already the render primitive on both sides — extraction is done. The open question is only whether the late-sh sidebar (swatches, peers, brush label) should move into `dartboard-tui` too, or stay as bespoke late-sh UI.

### Verification

- Multiplayer service tests in `late-ssh/tests/games/`: two sessions subscribe to the same host; A paints, B ticks and sees it; peer join/leave; `ConnectRejected` when the 11th session joins.
- Parser tests for the modified-arrow and SGR-mouse forms already exist; extend them when new `ParsedInput` variants are added (e.g. to distinguish `Alt+Enter` from `Enter` for a future emoji picker).
- Run:
  - `cargo check -p late-ssh`
  - `cargo check -p late-ssh --tests`
  - `cargo test -p late-ssh dartboard`

---

## Phase 1: Blackjack MVP

**Goal:** ship a playable blackjack loop that validates the chip economy, service/state/input/ui boundaries, and the path toward shared multiplayer table games.

### Why Blackjack first

- Natural chip sink — betting is the whole point
- Simple rules, fast hands (~30s each)
- PvE so no matchmaking / coordination
- Establishes `state.rs` / `svc.rs` / `input.rs` / `ui.rs` pattern for Poker later
- Works in single-player mode today, upgrades to multi-seat cleanly when we wire the chat room later

### MVP scope — decisions locked in

| Decision | Choice |
|---|---|
| Seats per table | 1 active player in the current implementation |
| Shoe | 6-deck casino shoe, reshuffles at penetration |
| Bet range | 10–100 chips (from `state.rs` `MIN_BET`/`MAX_BET`) |
| Dealer rule | Stands on soft 17 |
| Blackjack payout | 3:2 (rounded toward zero on odd bets) |
| Splits | Not in MVP |
| Doubles | Not in MVP |
| Insurance / even money | Not in MVP |
| Settlement | Optimistic — local balance updates the moment `settle()` returns; `credit_payout` runs fire-and-forget; `HandSettled` event is confirmation only |
| Arcade placement | Existing games picker, but admin-only until Phase 2 lands |
| Chat room wiring | Deferred — migration 024 stays parked for Phase 2 |
| `refund_bet_task` | Dropped for MVP — no abandonment path exists |

### Already shipped

- `blackjack/state.rs` — pure math helpers plus a thin client-side blackjack view state. The app-local state now mainly owns UI input, current user balance, pending request tracking, and subscribed receivers.
- `blackjack/svc.rs` — `BlackjackService` now owns the authoritative shared table state in-memory, publishes `BlackjackSnapshot` via `watch`, and emits per-user action/result events via `broadcast` following the `vote/svc.rs` pattern.
- `BlackjackSnapshot` is now the read model for the UI. The game screen renders from snapshots instead of reading mutable blackjack internals directly.
- Arcade wiring is in place, but Blackjack is currently gated behind `is_admin` and shown grayed out for non-admin users.
- Migration `024_add_game_rooms.sql` — present but still unused in code (supports `kind='game'` + `game_kind` column + partial unique index on `(game_kind, slug)`).

### MVP shipped

- `ChipService` has `debit_bet` and `credit_payout`.
- `BlackjackService` owns the shared table and handles bet/deal/hit/stand/settle transitions.
- `watch` snapshots publish the latest table view; `broadcast` events publish per-user results/errors.
- App-local blackjack state is now a thin client wrapper with local input buffer, local balance, pending request tracking, and subscribed receivers.
- Input/UI/app-shell wiring is complete.
- Blackjack is admin-gated in the arcade while the shared table remains incomplete.

### Verification shipped

- `cargo check -p late-ssh`
- `cargo check -p late-ssh --tests`
- `cargo test -p late-ssh blackjack --lib`

---

## Current status

The code has already moved past strict per-session MVP architecture in one important way:

- Blackjack is no longer owned as authoritative state by each SSH session.
- The service is now the authority and publishes snapshots/events.
- Clients subscribe to the shared table snapshot and keep only thin local UI state.

That means the app now has **shared-state multiplayer plumbing**, but **not full multi-seat blackjack yet**.

### What exists right now

- One shared in-memory blackjack table owned by `BlackjackService`
- `watch` snapshots for latest table state
- `broadcast` events for per-user async results/errors
- One active player at a time
- Other connected clients can observe the same shared table state
- Admin-only gate in the arcade while this remains unfinished

### What is still missing before this counts as true multiplayer blackjack

- Seat map (`seat -> user`)
- Sit/leave flow
- Multiple simultaneous bets before a hand starts
- Per-seat hands and settlement
- Turn order across seated players
- AFK/disconnect handling
- Multiple tables / table IDs
- Game-room/chat binding via migration 024

## Phase 2: Multi-Seat Blackjack

Turn the current shared single-table implementation into a true multi-seat table game. This is where chat room wiring, migration 024, seat management, timers, and disconnect handling all land.

**Scope of Phase 2:**
- Bind a table to a `ChatRoom` of `kind='game'`, `game_kind='blackjack'`, single permanent row seeded at startup (slug `bj-001`, not `game-blackjack`, so the 1→N path is free)
- Expand the current single shared table into `Arc<Mutex<HashMap<RoomId, BlackjackTable>>>` owned by `BlackjackService`
- Seat management: 5 seats, sit/leave independent from chat membership
- Turn timers: 15s per action, 20s for betting, 3-strike AFK unseat
- Hard-disconnect hook via `SessionRegistry` drop → auto-stand + free seat at end of round
- Per-table chat (reuses existing chat infra once the room is wired)
- Split-pane UI: game table on top, scoped chat on bottom
- Activity feed broadcasts for big wins (`🃏 @mat won 80 chips at Blackjack`)
- Extract shared host concerns (seat state, turn timer, disconnect handling) into `app/games/table_host.rs` — wait for Poker to confirm the abstraction

### First concrete steps from the current code

- Replace `active_player_id` with a seat model
- Change snapshot shape from single-player hand/bet fields to per-seat table fields
- Add join/sit/leave actions and events
- Add round phases for multi-player betting and player turn rotation
- Keep the current `watch` snapshot + `broadcast` event split

**Still deferred to Phase 3+:**
- Splits, doubles, insurance
- Multiple tables (second room)
- Private tables (`visibility='private'`)
- Hand history / stats table
- Per-table chip leaderboard

---

## Phase 3+: Future (not planned yet)

### Monthly chip leaderboard resets
- Archive monthly chip leaders (top 3 get a permanent badge?)
- Reset balances to baseline at month end
- "Hall of Fame" display somewhere

### Strategy multiplayer (Chess, Battleship)
- No chips needed — W/L record + rating
- Async: make a move, come back later
- Game completion counts toward daily streaks
- `/challenge @user chess` in chat for matchmaking

### More casino games (Poker)
- Texas Hold'em: PvP, uses chip betting
- Needs turn management, pot logic, hand evaluation
- Validates the `table_host.rs` extraction
- Higher complexity — build after Blackjack Phase 2 validates the multi-seat host

### Chat-based matchmaking
- Activity feed broadcast when someone sits at an empty table
- `/play <game>` and `/challenge @user <game>` commands
- Accept/decline prompts

---

## Game category model (unified view)

| Category | Games | Win condition | Leaderboard section | Streaks | Chips |
|----------|-------|--------------|-------------------|---------|-------|
| Daily puzzles | Sudoku, Nonograms, Minesweeper, Solitaire | Solve the daily | Today's Champions | Yes | +50 bonus per completion |
| High-score | Tetris, 2048 | Personal best | All-Time High Scores | No | No |
| Casino | Blackjack, Poker (future) | Grow your chip balance | Chip Leaders | Optional | Bet and win/lose |
| Strategy | Chess, Battleship (future) | Beat opponent | W/L + Rating | Yes (game completed) | No |
