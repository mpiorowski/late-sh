# Adding a New Game

This guide is for contributors who want to add a new multiplayer game room to
late.sh. By the end you should know what to write, where it lives, and which
patterns the existing games (Blackjack, Poker, Tic-Tac-Toe) already prove out.

If anything here disagrees with the code, trust the code and please open a PR
to fix this file.

## What "a game" actually is here

A game in late.sh is a **persistent room** that any user can enter, sit at, and
play. Each room is one row in the `game_rooms` table. The runtime state (board,
seats, turn) lives in process memory, not in the DB. Restart the SSH process
and the room still exists, but the runtime starts fresh.

Every game shares the same outer chrome:

- A directory page that lists rooms (`Rooms` screen, key `4`)
- A modal flow to create a new room
- A two-pane active view: game on top, embedded chat on bottom

Your job is to plug into that chrome by implementing two traits and writing the
game's own runtime. You will not touch the rooms layer.

Live reference implementations:

- `late-ssh/src/app/rooms/tictactoe/` — minimal example, ~6 small files
- `late-ssh/src/app/rooms/poker/` — asymmetric-info example with public table
  state plus per-user private hole-card state
- `late-ssh/src/app/rooms/blackjack/` — complex example with chips, settlements,
  AFK timer
- `late-ssh/src/app/rooms/CONTEXT.md` — internal architecture notes

Read the TTT folder first. It is the smallest legal implementation.

## The mental model in one minute

Every game has a **service** and a **state**:

| | Service (`svc.rs`) | State (`state.rs`) |
|---|---|---|
| Lifetime | one per `room.id`, lives in the manager | one per session that entered the room |
| Owns | the truth (board, seats, turn) | a cached snapshot of the latest publish |
| Communication | publishes via `tokio::sync::watch` channel | subscribes via `watch::Receiver` |
| Mutation | tokio tasks under a `tokio::sync::Mutex` | only mutates local UI state (cursor, selection) |
| Public API | `*_task` methods (fire-and-forget) | direct method calls (cheap, sync) |

The service is the linearizable truth. Many sessions in the same room share one
service via `Arc<Mutex<...>>`. Each session also has its own `State` with a
cached snapshot, updated lock-free in `tick()`. Rendering reads the cache.

This split is the central pattern. Internalize it before writing code.

## File shape

Inside `late-ssh/src/app/rooms/<your_game>/`:

```
mod.rs           ← module declarations only, no re-exports
manager.rs       ← impl RoomGameManager + impl ActiveRoomBackend for State
svc.rs           ← in-memory service: SharedState, watch sender, *_task methods
state.rs         ← per-session client wrapper with cached snapshot
input.rs         ← key bytes -> state method calls (returns InputAction)
ui.rs            ← snapshot -> ratatui widgets
create_modal.rs  ← impl CreateRoomModal for your form
settings.rs      ← (optional) typed settings struct + JSON serde
```

Don't add `pub use` re-exports in `mod.rs`. The rooms code references each
module by full path on purpose; keep that explicit.

## The trait surface

Three traits in `late-ssh/src/app/rooms/backend.rs`. You implement all three.

### `RoomGameManager` — process-wide singleton

Returned strings drive directory rendering, slug generation, and labels. The
single instance is constructed once at startup and registered with the
`RoomGameRegistry`.

```rust
pub trait RoomGameManager: Send + Sync {
    fn kind(&self) -> GameKind;
    fn label(&self) -> &'static str;             // "Tic-Tac-Toe"
    fn slug_prefix(&self) -> &'static str;       // "ttt" -> "ttt-019a8d2e1f4b"
    fn default_room_name(&self) -> &'static str; // pre-fills the create modal
    fn default_settings(&self) -> Value;         // serde_json::json!({}) is fine
    fn open_create_modal(&self) -> Box<dyn CreateRoomModal>;
    fn directory_meta(&self, room: &RoomListItem) -> DirectoryMeta;       // pace/stakes labels
    fn directory_hints(&self, room_id: Uuid) -> Option<DirectoryHints>;   // live "X/Y seated"
    fn enter(&self, room: &RoomListItem, user_id: Uuid, chip_balance: i64)
        -> Box<dyn ActiveRoomBackend>;
}
```

`directory_meta` reads `room.settings` (opaque JSON; you parse it). It runs on
every render of every directory row, so keep it cheap. Cache parsing if your
settings struct is heavy.

`directory_hints` reads your in-memory state to count seats. Returning `None`
yields `?/N` in the directory until somebody enters and boots the runtime.

`enter` is the lazy-boot hook. Inside it you'll typically call
`self.get_or_create(room.id, ...)` to find or build a `Service` and wrap it in
a `State::new(svc, user_id)`. See the TTT `manager.rs` for the canonical shape.

### `ActiveRoomBackend` — per-session, lives on `App.active_room_game`

```rust
pub trait ActiveRoomBackend: Send {
    fn room_id(&self) -> Uuid;
    fn tick(&mut self);                                      // drain watch channel
    fn touch_activity(&self);                                // AFK reset (interior mutability)
    fn handle_key(&mut self, byte: u8) -> InputAction;       // Ignored | Handled | Leave
    fn handle_arrow(&mut self, _key: u8) -> bool { false }
    fn preferred_game_height(&self, area: Rect) -> u16;      // height negotiation
    fn draw(&self, frame: &mut Frame, area: Rect, ctx: GameDrawCtx<'_>);
    fn title_details(&self) -> Option<RoomTitleDetails> { None }
    fn chip_balance(&self) -> Option<i64> { None }
    fn can_sync_external_chip_balance(&self) -> bool { false }
    fn sync_external_chip_balance(&mut self, _balance: i64) {}
}
```

Notes:

- `tick` runs every frame. Its job is `if rx.has_changed() { snapshot = rx.borrow_and_update().clone(); }`. Keep it cheap.
- `touch_activity` takes `&self` because the call site only has a shared
  borrow. Use interior mutability (`Mutex`, atomic) if you actually need to
  mutate. TTT writes `{}` and that's a valid choice.
- `handle_key` returning `InputAction::Leave` is the only way for the game to
  ask the rooms layer to exit the active room. Use it for Esc / `q`.
- `preferred_game_height` is a wish. The rooms layer enforces a chat
  minimum (currently 8 rows) — your wish gets clamped if needed.
- `title_details` lets you contribute strings to the rooms title bar. Anything
  you don't want to show, leave as `None`.
- The chip methods are optional. If your game has nothing to do with chips,
  ignore them — defaults return `None`/`false`.

### `CreateRoomModal` — owns your create form

```rust
pub trait CreateRoomModal: Send {
    fn draw(&self, frame: &mut Frame, area: Rect);
    fn handle_event(&mut self, event: &ParsedInput) -> CreateModalAction;
}

pub enum CreateModalAction {
    Continue,   // keep modal open, redraw
    Cancel,     // close modal
    Submit { display_name: String, settings: serde_json::Value },
}
```

The rooms layer shows a game picker first, then hands off to your modal once
the user chooses your game. **Your modal owns its full UI, state, and
keyboard.** Layout, focus model, validation, paste handling — all yours.

When the user submits, return `CreateModalAction::Submit { display_name,
settings }`. The rooms layer pairs your settings JSON with the `GameKind` it
already knows about, and persists the row. The modal never sees a `GameKind`.

If your game has no configurable options, follow the TTT modal — just a name
field and a footer.

## Step-by-step: adding a game

1. **Add a `GameKind` variant** in `late-core/src/models/game_room.rs`:
   - Extend the enum + `as_str` + `parse` + `ALL` array.
   - Pick a stable lowercase string for the DB column (e.g. `"connect_four"`).
   - This is the only enum change you'll make.

2. **Create the folder** `late-ssh/src/app/rooms/<your_game>/` and the
   `mod.rs` with `pub mod` lines for each file you'll add. Don't add re-exports.

3. **Write `svc.rs`** — the runtime. Pattern to follow:

   ```rust
   #[derive(Clone)]
   pub struct YourGameService {
       room_id: Uuid,
       snapshot_tx: watch::Sender<YourGameSnapshot>,
       snapshot_rx: watch::Receiver<YourGameSnapshot>,
       state: Arc<Mutex<SharedState>>,    // tokio::sync::Mutex
   }
   ```

   Public methods follow the `*_task` convention: each spawns a tokio task
   that locks `state`, calls a `SharedState` method to mutate, then calls
   `publish(&state)` to send a fresh snapshot. Callers never block. See
   `tictactoe/svc.rs` for the canonical form.

4. **Write `state.rs`** — the per-session client:

   ```rust
   pub struct State {
       user_id: Uuid,
       cursor: usize,                                 // local UI state
       snapshot: YourGameSnapshot,                    // cached
       svc: YourGameService,                          // handle to truth
       snapshot_rx: watch::Receiver<YourGameSnapshot>,
   }
   ```

   `tick()` drains the channel; other methods either delegate to
   `svc.*_task(...)` or mutate local UI state.

5. **Write `input.rs`** — pure functions over `&mut State`:

   ```rust
   pub fn handle_key(state: &mut State, byte: u8) -> InputAction { ... }
   pub fn handle_arrow(state: &mut State, key: u8) -> bool { ... }
   ```

   Map `Esc` (and conventionally `q`) to `InputAction::Leave`. Avoid `j`/`k`
   and arrows for vertical navigation — those are reserved for chat scroll
   when in an active room. TTT uses `w`/`x` for vertical cursor; follow that.

6. **Write `ui.rs`** — render the snapshot:

   ```rust
   pub fn draw_game(frame: &mut Frame, area: Rect, state: &State, usernames: &HashMap<Uuid, String>) { ... }
   ```

   Always handle a small-area fallback (compact layout). Reach for the
   `theme::*` palette in `late-ssh/src/app/common/theme.rs`; do not hardcode
   colors.

7. **Write `create_modal.rs`** — implement `CreateRoomModal`. Keep state on
   the struct (display name, focus, any options). Return `Submit` only when
   inputs validate. See `tictactoe/create_modal.rs` for a single-field
   minimal modal, `blackjack/create_modal.rs` for a multi-field one with
   pace/stake options.

8. **Write `manager.rs`** — implement both `RoomGameManager` and
   `ActiveRoomBackend for State`. The manager holds
   `Arc<Mutex<HashMap<Uuid, YourGameService>>>` and a `get_or_create` method
   for the lazy-boot pattern.

9. **Wire the manager into the registry** in `late-ssh/src/main.rs`:

   ```rust
   let your_game_table_manager = YourGameTableManager::new(/* deps */);
   let room_game_registry = RoomGameRegistry::new(
       blackjack_table_manager.clone(),
       tictactoe_table_manager,
       your_game_table_manager,            // add this
   );
   ```

   Update `RoomGameRegistry::new` in `late-ssh/src/app/rooms/registry.rs` to
   accept the new arg and add the field + match arm in `manager()`. This is
   the single place a `match GameKind` happens.

10. **Add the filter label** in `late-ssh/src/app/rooms/filter.rs`:
    add an arm to the `label` match for your new kind.

11. **Update CONTEXT files**: append a short section to
    `late-ssh/src/app/rooms/CONTEXT.md` covering your game's runtime model,
    keys, seats, and any invariants. Keep it tight.

That's the whole list. No edits to `rooms/{ui,input,svc,state}.rs`,
`rooms/backend.rs`, or `App` itself.

## Settings JSON

`game_rooms.settings` is `serde_json::Value` — opaque to the rooms layer. Your
game owns the shape.

Recommended pattern (see `blackjack/settings.rs`):

```rust
#[derive(Serialize, Deserialize, ...)]
pub struct YourGameSettings { ... }

impl YourGameSettings {
    pub fn from_json(value: &Value) -> Self {
        serde_json::from_value::<Self>(value.clone())
            .unwrap_or_default()
            .normalized()
    }
    pub fn to_json(&self) -> Value { ... }
    pub fn normalized(self) -> Self { /* clamp values to allowed options */ }
}
```

`unwrap_or_default()` makes corrupt or old-schema rows survive — they fall back
to defaults instead of crashing the directory render.

If your game has no configurable options, return `serde_json::json!({})` from
`default_settings` and ignore the `settings` field everywhere.

## Multiplayer & state

Two players in the same room share **one** `YourGameService` (via the manager's
HashMap) and have **two** `State` structs (one per session). Writes serialize
through the service's mutex; reads happen lock-free against each session's
cached snapshot.

If a session presses a key based on a slightly stale cache, the service
re-validates against the truth under the lock. Be defensive in `SharedState`
methods: turn checks, occupancy checks, "are you actually seated" checks.
Stale-cache races are normal and harmless if you validate.

### Asymmetric-info games

Most simple games publish the same snapshot to all sessions. Poker now proves
the pattern for games where each player sees a different view (own hole cards
visible, others' hidden), without changing the room trait surface. See the
"Asymmetric-Info Game Pattern" section in
`late-ssh/src/app/rooms/CONTEXT.md` for the recommended split-channel pattern.

Short version: one `watch::Sender<PublicSnapshot>` plus a
`HashMap<Uuid, watch::Sender<PrivateSnapshot>>` keyed by user_id. The per-user
private channel is created in `manager.enter` (which already gets `user_id`).
Keep the deck inside `SharedState` only; never put secret state on a snapshot
that any user could receive.

## The trip a key takes

Useful to keep in your head as you read the code:

```
User presses '5' inside an active TTT room
  ↓
rooms/input.rs::handle_active_room_key
  ↓ (chat-first heuristic decides this is a game key)
backend.handle_key(b'5')                    ← trait dispatch
  ↓
tictactoe/input.rs::handle_key(state, b'5')
  ↓
state.set_cursor(4); state.place_at_cursor()
  ↓
svc.place_task(user_id, 4)                  ← spawns tokio task, returns
  ↓ (on tokio worker)
state.lock().await
  ↓
SharedState::place(user_id, 4)              ← validates + mutates truth
  ↓
publish(&state)                             ← snapshot_tx.send(...)
  ↓
all subscribed receivers in this room get notified

Next render frame for any session:
  ↓
backend.tick()
  ↓
snapshot_rx.has_changed() == true
  ↓
self.snapshot = rx.borrow_and_update().clone()
  ↓
backend.draw(...) renders the new snapshot
```

The same shape for every game. Only `SharedState::place` (or your equivalent)
changes per game — that's where the rules live.

## UI conventions

- Use `theme::*` for all colors. Don't hardcode.
- Provide a compact fallback when `area.height < N` or `area.width < M`. The
  rooms layout can shrink your pane unexpectedly when chat takes priority.
- `preferred_game_height` is a hint, not a guarantee. Don't index into fixed
  vertical chunks without checking the actual area.
- Renders run lock-free against your local cache. Never `.await` or grab a
  service mutex from inside `draw`.
- The rooms layer concatenates strings from `title_details`. Keep them short.

## Key conventions

- `Esc` exits the active room (via `InputAction::Leave`).
- `q` is conventionally aliased to `Esc` for game keys; do this in your
  manager's `handle_key` if you want it (Blackjack does, TTT does).
- Avoid `i`, `j`, `k`, arrows up/down, scroll, and message-action keys
  (`d`, `r`, `e`, `p`, `c`, `f`, `g`) — these are routed to embedded chat
  before reaching the game. Full list in
  `rooms/input.rs::should_route_active_room_chat_key`.
- Backtick toggles Dashboard — don't bind it.

## Project conventions

- **`bail!`/`anyhow!`/`tracing` error strings start lowercase.** UI banners
  keep sentence case.
- **Use `Uuid::now_v7()`** for new IDs, not `Uuid::new_v4()`.
- **No em dash** (`—`) in UI copy or prose. Use `-`, `:`, or rephrase.
- **Don't run `cargo test`, `cargo nextest`, or `cargo clippy` as a contributor
  agent** — the maintainer runs those. `make check` is the human-side gate
  before opening a PR.
- **Tests** for pure logic (rules, settings parsing, key routing helpers) go
  inline as `#[cfg(test)] mod tests`. Anything touching `RoomsService`, DB,
  chips, or service tasks goes in `late-ssh/tests/` and uses testcontainers.

## What you do not need to touch

If you're following this guide and find yourself editing one of these files,
stop and re-check — you probably want to extend a trait method instead:

- `rooms/svc.rs` — game-agnostic CRUD over `game_rooms`. Stores opaque JSON.
- `rooms/state.rs` — drains rooms snapshot/events into App.
- `rooms/input.rs` — routes keys for directory, picker, modal, active room.
- `rooms/ui.rs` — renders directory, picker, active room split, delegates
  game drawing.
- `rooms/backend.rs` — the trait definitions.
- `App` itself — your game's session state lives in your `State` struct,
  reached via `App.active_room_game: Option<Box<dyn ActiveRoomBackend>>`.

If you have a real reason to touch one of those — e.g. a new key in the
chat-first heuristic, or a new field on `RoomTitleDetails` — explain why in
the PR. The trait boundary is what keeps this directory navigable as more
games land.

## Worked example to read

If you want a single file to read end-to-end before writing anything, read
`late-ssh/src/app/rooms/tictactoe/svc.rs`. It is ~200 lines and covers:

- The Service / SharedState split
- Tokio task spawning for mutation
- Watch channel for fanout
- Pure rules (`winning_mark`)
- Status messages for UI feedback
- Defensive validation in every mutation method

Once that file makes sense, the rest of the TTT folder reads in 10 minutes,
and adding your own game becomes mostly typing.

Welcome aboard.
