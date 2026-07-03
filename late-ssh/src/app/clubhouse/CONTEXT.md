# Clubhouse Context (late-ssh/src/app/clubhouse)

## Metadata
- Domain: the Late Lounge tavern, top-level screen `0`, the landing screen for every session
- Last updated: 2026-07-03 (opened to everyone: admin gate removed, `0` joins the top nav and Tab cycle, sessions land here on connect; AI bartender greeting with scripted fallback; bartender banner top-left; shared multiplayer lobby, spawn-in-seat, speech bubbles replace the embedded chat panel, emotes, door ambience, dog petting, first-visit tutorial)
- Status: Active

## 1. Summary

A full-bleed walkable ASCII tavern rendered over the whole content area with a
one-line #lounge composer pinned to the bottom. The crowd is real: every
active human on late.sh holds a seat in one process-global lobby, walkers
carry live positions every session renders, and fresh #lounge messages float
over their authors' heads as speech bubbles. There is no chat panel here; the
room is the chat surface, and the full history lives in #lounge on Home.

## 2. Module map

| File | Owns |
|---|---|
| `map.rs` | The 184x50 generated floor plan (`MAP` literal, do not hand-edit; re-run `scripts/gen_clubhouse_map.py --write`), collision (`walkable`), `SEATS`/`STANDING_SPOTS`/`DOOR_STACK`, interactive zones, animation cell lists, `DOOR_SIGN`. |
| `lobby.rs` | `SharedLobby`, the process-global `Arc<Mutex<..>>` presence map: parked spot assignments, walkers, emotes, the dog-pet event, snapshots. |
| `state.rs` | Per-session view state: camera target, animation clock, latest `LobbySnapshot`, arrival/departure door events, the `Tutorial` state machine. |
| `input.rs` | Walking (arrows/hjkl), `i` composer, `w`/`x` emotes, `t` bartender mention, Enter on landmarks/dog, tutorial Enter. Returns `false` for globals. |
| `ui.rs` | Renderer: camera pan, base-grid styling, animations, crowd placement, emote frames, speech bubbles, door ambience, tutorial overlays, prop popovers, composer footer. |

## 3. The shared lobby (multiplayer contract)

- `crate::state::State.clubhouse_lobby` is the single process-global
  `SharedLobby`, threaded into each session through
  `SessionConfig.clubhouse_lobby` (like `active_users`). **Single-replica by
  design** (`infra/service-ssh.tf` runs 1 SSH replica); a second replica
  needs presence moved to a shared channel.
- Every active human (bots excluded via `fingerprint: None`, including this
  session's own user) is *parked* on a spot: a random free seat, then the
  first free standing spot, then the door stack (`map::DOOR_STACK` slots,
  `+N at the door` past that). Nobody is ever hidden; the headcount in the
  frame title is the full active count. There is no seat rotation anymore.
- The first movement key turns a parked user into a *walker*: the seat frees
  automatically (assignment skips walkers) and the avatar steps off the seat
  cell. Walkers persist until disconnect; door-stack patrons are promoted
  into freed seats on sync, oldest first.
- Sync cadence: sessions on the screen reconcile the lobby with
  `active_users` about once a second (`App::tick_clubhouse`) and clone a
  render snapshot every world tick. Sessions off the screen touch nothing.
- Emotes (`w` wave, `x` dance) and dog pets are lobby state with wall-clock
  windows (`EMOTE_MS`, `DOG_PET_MS`), so every session plays them.

## 4. Chat: bubbles, not a panel

- The old embedded `#lounge` chat panel is gone. `ui::draw` splits the area
  into the tavern plus the shared `ComposerBlockView` footer (same block the
  dashboard card uses; grows while typing, shows placeholder hints idle).
- `i` (or Enter in the open) composes into #lounge through the normal global
  composer pipeline; image paste works (Clubhouse is a
  `is_chat_composer_context` screen in `app::input`).
- Messages younger than ~10s render as bordered bubbles above their author's
  avatar (latest per author, up to 3 lines, width widens 28 -> 36 -> 44
  before truncating, reply-quote line stripped). Room tails are newest-first
  (`ChatState::push_message`); `fresh_bubble_messages` depends on that.
- The bartender does not bubble over his sprite: his freshest line pins as a
  camera-independent banner in the top-left corner (`draw_bartender_banner`,
  ~14s), so it never collides with patron bubbles at the bar and is visible
  from across the room. Graybeard bubbles normally.
  `App.clubhouse_bartender_id`/`clubhouse_graybeard_id` are captured from
  `active_users` during roster refresh.
- Message selection/reactions/scroll do not exist on this screen; Home owns
  them. The lounge is still pinned as the visible chat room for read cursors
  (`sync_visible_chat_room`).

## 5. First-visit tutorial

- Armed by `!extract_clubhouse_tutorial_done(user.settings)`
  (`users.settings.clubhouse_tutorial_done`, late-core). Fires once on the
  first clubhouse entry: spawns the player at the door (`Tutorial::Welcome`),
  advances to `GoToBar` on first step (bar sign pulses, small pinned hint),
  reaching the counter triggers `BarLesson` plus a one-shot @bartender
  greeting posted to #lounge (`App::send_clubhouse_bartender_greeting`):
  AI-generated in his voice when the AI service is up, falling back to a
  scripted line on disabled AI, errors, or a 6s timeout
  (`ghost::bartender_tutorial_greeting`); either way it must tell them to
  press `i`. Then `SendOff` lists the landmarks and Ctrl+O.
- Enter advances popups (`tutorial_capturing_keys`); Esc anywhere skips
  (arm in `dispatch_escape`). Completion persists once via
  `ProfileService::set_clubhouse_tutorial_done` (fire-and-forget, failure
  only logged: worst case the tour runs again next session).
- The Ctrl+O profile nudge lives here on purpose: the old
  "open settings on connect" behavior was removed in favor of this beat.

## 6. Gotchas

- Single-width glyphs only in the art and effects (no emoji-class chars).
- `MAP` is generated; hand-edits get clobbered by `gen_clubhouse_map.py`.
  New furniture/zones go into the generator, then re-sync the hand-written
  constants (`SEATS`, zones, `DOOR_STACK`, test probes) from its output.
- The lobby stores wall-clock `Instant`s; unit tests use
  `SharedLobby::with_seed` for deterministic seat draws.
- `walkable` allows standing ON the counter but never behind it; the flood
  fill tests in `map.rs` guard the bartender alley seal and seat
  reachability. `DOOR_STACK` slots must stay walkable.
- The tavern draws no widget chrome; headcount and key hints live in
  `app_frame_title` (`render.rs`). Update that line when keys change.
