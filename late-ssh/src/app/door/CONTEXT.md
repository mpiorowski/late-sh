# Door Games Context

## Metadata
- Scope: `late-ssh/src/app/door`
- Last updated: 2026-06-06
- Purpose: local working context for the Door Games screen and BBS-style persistent worlds.
- Parent context: `../../../../CONTEXT.md`

## Scope

`late-ssh/src/app/door` owns the top-level Door Games screen. It is a lobby-first shell for BBS-style games. Lateania is currently the only game, but the module is intentionally structured as a game list plus per-game runtime.

Keep `mod.rs` declaration-only. Do not add `pub use` re-export layers.

## Lifecycle

- Top-level screen key is `4`, rendered as `Door Games`.
- Entering Door Games shows the Door lobby. It does not auto-join Lateania.
- The lobby uses `j/k` or up/down arrows to move selection and `Enter` to launch the selected game.
- When Lateania is selected in the lobby, `d` opens a destructive confirmation prompt to delete the current user's saved Lateania character. `Enter`/`Y` confirms; `N`, `q`, or `Esc` cancels.
- Launching Lateania creates `lateania::state::State`, subscribes to the shared service snapshot, and joins the persistent world.
- Leaving an active Door game drops its per-session state. Lateania's `Drop` path sends the service leave event.
- Navigating away from Door Games also drops active Lateania state.

## Input Contract

- Door lobby input behaves like the Arcade lobby: screen switching and global shortcuts remain available unless the lobby itself handles the key.
- Active Door games capture ordinary key input, including number keys, `Tab`, `Shift+Tab`, `q`, and single-byte global shortcuts.
- Active Door games still allow:
  - `Esc` to leave the active game and return to the Door lobby.
  - Reserved/global modal shortcuts that run before screen dispatch, including `Ctrl+O`, `Ctrl+G`, `Ctrl+/`, and other existing app-level modal paths.
  - `?` to open the global help modal.
- Lateania class selection owns `1-5` after launch. Those keys must not switch top-level screens while a Door game is active.

## Lateania

Lateania lives under `lateania/`:
- `state.rs` owns the per-session client wrapper and local UI panel state.
- `input.rs` maps Lateania controls after launch.
- `ui.rs` renders the active game.
- `svc.rs` owns the shared persistent world service.

Lateania is not an Arcade game and should not use `App::is_playing_game`; Door Games tracks active state by whether `App::lateania_state` is present.

Character persistence is durable through `late_core::models::mud_character` / `mud_characters`. The service loads on join and saves on leave, idle timeout, and a 60-second autosave loop while the character is present. The saved character blob includes class, XP/level, gold, HP, safe-room location, inventory, and equipped items.

Shared world runtime persistence is durable through `late_core::models::mud_world_state` / `mud_world_states`. The service loads the `lateania` row after startup and autosaves dirty world state every 15 seconds. The saved world blob includes mob HP/alive state, mob respawn timers, mob stuns, and mob damage-over-time stacks. Per-player combat targets, player cooldowns/effects, and pending activity events remain transient. Character reset deletes only the current user's character row and active effects owned by that user; it does not wipe the shared world row.

## Tests

- Pure lobby-order helpers can be unit-tested inline in `door/input.rs`.
- DB/service coverage for Lateania belongs under `late-ssh/tests/door/` and must use shared testcontainers helpers.
- Root test policy applies: agents do not run `cargo test`, `cargo nextest`, or `cargo clippy`.
