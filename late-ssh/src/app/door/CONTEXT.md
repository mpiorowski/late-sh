# Lateania Screen Context

## Metadata
- Scope: `late-ssh/src/app/door`
- Last updated: 2026-06-08
- Purpose: local working context for the Lateania top-level screen and legacy door-game runtime.
- Parent context: `../../../../CONTEXT.md`

## Scope

`late-ssh/src/app/door` owns the top-level Lateania screen. The source domain still uses the legacy `door` namespace and `Screen::Lateania` enum variant, but the user-facing page is Lateania because this screen has one first-class persistent world rather than a generic door-game list.

Keep `mod.rs` declaration-only. Do not add `pub use` re-export layers.

## Lifecycle

- Top-level screen key is `4`, rendered as `Lateania`.
- Entering the Lateania screen shows the Lateania landing page. It does not auto-join the live world.
- `Enter` launches Lateania from the landing page.
- `d` opens a destructive confirmation prompt to delete the current user's saved Lateania character. `Enter`/`Y` confirms; `N`, `q`, or `Esc` cancels.
- Launching Lateania creates `lateania::state::State`, subscribes to the shared service snapshot, and joins the persistent world.
- Leaving the active Lateania world drops its per-session state. Lateania's `Drop` path sends the service leave event.
- Navigating away from the Lateania screen also drops active Lateania state.

## Input Contract

- The Lateania landing page behaves like the Arcade lobby: screen switching and global shortcuts remain available unless the landing page itself handles the key.
- Active Lateania captures ordinary key input, including number keys, `Tab`, `Shift+Tab`, `q`, and single-byte global shortcuts.
- Active Lateania still allows:
  - `Esc` to leave the active world and return to the Lateania landing page.
  - Reserved/global modal shortcuts that run before screen dispatch, including `Ctrl+O`, `Ctrl+G`, `Ctrl+/`, and other existing app-level modal paths.
  - `?` to open the global help modal.
- Lateania class selection owns `1-5` after launch. Those keys must not switch top-level screens while Lateania is active.
- Lateania after class selection uses `w/a/s/d` and arrows for cardinal movement, `y/u/n/m` for diagonals, `<`/`,` for up, `>`/`.` for down, `space`/`x`/`Enter` for attacks, `1-9` for abilities unless a list panel is open, and `z` to flee.
- Lateania side panels: `c` character, `v` abilities, `t` inventory, `b` shop when a merchant is present, `o` examine/interact, `j` quest journal, `k` titles. In list panels, `w/s` or up/down move the cursor, `1-9` jump/activate a row, `Enter` activates the selected row, and `x` sells the selected inventory item at a shop.
- Lateania world actions: `r` recalls the player to Embergate's Town Square when out of combat, and `f` toggles auto-following another adventurer in the same room.

## Lateania

Lateania lives under `lateania/`:
- `state.rs` owns the per-session client wrapper and local UI panel state.
- `input.rs` maps Lateania controls after launch.
- `ui.rs` renders the active game.
- `svc.rs` owns the shared persistent world service.

Lateania is not an Arcade game and should not use `App::is_playing_game`; the screen tracks active state by whether `App::lateania_state` is present.

Character persistence is durable through `late_core::models::mud_character` / `mud_characters`. The service loads on join and saves on leave, idle timeout, and a 60-second autosave loop while the character is present. The saved character blob includes class, XP/level, gold, HP, safe-room location, visited rooms, inventory, equipped items, rolled ability scores, earned title names/levels, active title selection, and completed Frontier quest indices.

Shared world runtime persistence is durable through `late_core::models::mud_world_state` / `mud_world_states`. The service loads the `lateania` row after startup and autosaves dirty world state every 15 seconds. The saved world blob includes mob HP/alive state, mob respawn timers, mob stuns, and mob damage-over-time stacks. Per-player combat targets, player cooldowns/effects, and pending activity events remain transient. Character reset deletes only the current user's character row and active effects owned by that user; it does not wipe the shared world row.

## Tests

- Pure lobby-order helpers can be unit-tested inline in `door/input.rs`.
- DB/service coverage for Lateania belongs under `late-ssh/tests/door/` and must use shared testcontainers helpers.
- Root test policy applies: agents do not run `cargo test`, `cargo nextest`, or `cargo clippy`.
