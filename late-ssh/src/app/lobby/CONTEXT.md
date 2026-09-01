# Lobby Context

## Metadata
- Scope: `late-ssh/src/app/lobby` — the single front door for multiplayer play: the `Ctrl+G` modal and the two game domains it fronts (`daily/` async correspondence matches, `house/` live fixed tables).
- Last updated: 2026-09-01 (the backtick workspace cycle moved out to its own domain, `app/workspace/` with `workspace/CONTEXT.md`; the Lobby keeps its two legs' queries, `DailyState::my_turn_matches` and `HouseState::my_seated_tables`)
- Parent context: root `CONTEXT.md`. Sub-domain contexts: `daily/CONTEXT.md`, `house/CONTEXT.md` — this file owns only what spans both.
- Status: Active

## 1. Shape

The Lobby fronts two game domains that stay SEPARATE services (owner-locked): `DailyService` (DB-backed correspondence matches) and `HouseTableRegistry` (process-local singleton tables). There is no unifying trait and no `GameSurface` abstraction — the modal consumes both through plain exhaustive code (`LobbyEntry`); keep enums + exhaustive matches, no `_ =>` on roster enums.

Entry points:
- **`Ctrl+G` modal** (`modal_input.rs` / `modal_ui.rs`): one scrollable list — unseen results, your matches, open challenges, live games, then the fixed house-table block (stable chrome, live occupancy). Toggled from anywhere via the reserved global; opening calls `LobbyState::mark_seen`.
- **Sidebar panel** (`daily/panel.rs`): passive top-4 match view; content is daily-only so the panel stays in `daily/` (the `lobby` rule label itself is owned by `common/sidebar.rs`, glow bool passed via `SidebarProps.lobby_glow`).
- **Backtick** (`app/workspace/cycle.rs`, its own domain): hops Home chat → your-turn boards → seated house tables → unfinished Arcade dailies → live door games → Home, consuming this domain's `my_turn_matches` / `my_seated_tables`. See `workspace/CONTEXT.md`.
- **Screens**: `Screen::DailyMatch` (daily/board_*) and `Screen::HouseTable` (house/input+ui), both outside the Tab cycle, entered only from the modal or backtick; leaving restores the surface's `return_screen` and reopens the modal (except the backtick wrap home, which skips it).

## 2. Module map

| File | Responsibility |
|---|---|
| `mod.rs` | Declarations only. |
| `state.rs` | `LobbyState` (`App::lobby`): modal cursor + claim-confirm + unseen-challenge glow, and `LobbyEntry<'_>` — the modal's row enum over both domains. Entries are computed views: `entry_at`/`selected_entry` walk `DailyState`'s snapshot lists plus `HouseTable::ALL`. `sync(&DailyState)` runs every tick (idempotent) to pick up glow edges and clamp the cursor/claim against the moving snapshot. `move_selection` wraps at both ends through the pure `wrap_index` (unit-tested): the list is one flat index space, so wrapping backwards off the top lands on the last house table. |
| `modal_input.rs` | Modal key routing: `j`/`k` and the mouse wheel move (the modal owns input, so the wheel never reaches the global scroll fallback), Enter open/claim (confirm second-press), `c`/`C` challenge draft (draft state lives in `DailyState.challenge_draft` — it posts daily challenges), `x` cancel/dismiss, Esc peel (draft step → pending claim → close + mark seen). |
| `modal_ui.rs` | Modal renderer: near-fullscreen list with section rules, claim-confirm status line, footer keys, the challenge-draft overlay. When the list overflows, `visible_window_start` centers the selected line in the window (clamped to the first/last page) so there is always list visible below the cursor. |
| `daily/` | Correspondence domain: roster, service, board screens, panel. See `daily/CONTEXT.md`. |
| `house/` | Fixed house tables: roster, singleton registry, five runtimes, table screen. See `house/CONTEXT.md`. |

## 3. Invariants

- `LobbyState` owns presentation state only; the systems of record stay in `DailyService`'s snapshot and the house singletons' watch channels. Anything derivable is recomputed per call, not cached.
- `App::lobby.sync(&app.daily)` runs right after `app.daily.tick()` in `app/tick.rs`; nothing else mutates the glow.
- The modal is the only place a house table is entered from besides the backtick; both go through `HouseState::enter` with a preserved `return_screen`.
- `app/input.rs` owns the chat-surface gating for both screens (`screen_has_chat_pane` + `embedded_chat_room_id` rosters); the board/table input files never re-check composer/overlay state.
