# Contributing

Thanks for wanting to improve `late.sh`.

## Before you push

Run `make check` locally before opening a PR. CI is expensive — don't let the
pipeline catch what your machine could have.

## Ground rules

- Read [`LICENSE`](LICENSE) and [`LICENSING.md`](LICENSING.md) before
  contributing.
- Contributions are accepted under the repository's license terms unless we
  explicitly agree otherwise in writing.
- Do not submit code, assets, or content that you do not have the right to
  contribute.

## DCO sign-off required

By submitting a contribution to this repository, you certify that you have the
right to submit it under the repository's license terms and agree to the
[Developer Certificate of Origin (DCO v1.1)](https://developercertificate.org/).

Sign off every commit:

```bash
git commit -s
```

This adds a `Signed-off-by:` line to your commit message.

## Getting started

### Tooling

The repo includes `.mise.toml` with `rust`, `mold`, and `cargo-nextest`. Run
`mise install` to get the expected toolchain.

### Running locally

```bash
make start          # docker compose: ssh, web, postgres, icecast, liquidsoap
ssh localhost -p 2222   # connect to your local instance
```

That's it. Postgres, Icecast, and Liquidsoap all come up automatically. No
extra setup needed.

### Contributing themes

If you want to add a built-in SSH theme, read [`THEME.md`](THEME.md) before
opening a PR. It covers the required code changes, stable `theme_id` rules, and
theme-specific review expectations.

## Project structure

### Domain modules

Each feature area in `late-ssh/src/app/` follows a flat module pattern:

```
app/<domain>/
  mod.rs        # pub mod declarations only — no pub use re-exports
  state.rs      # sync UI state, drained from channels each tick
  input.rs      # key routing and mode guards
  ui.rs         # pure ratatui draw functions
  svc.rs        # async service — DB, broadcast, background tasks
  model.rs      # DB-backed types (when domain-specific)
```

Not every domain needs every file — only add what you use. Sub-domains are fine
(e.g. `chat/news/`, `games/minesweeper/`).

### How the pieces fit together

The TUI runs a sync render loop at 15 fps. The boundary between sync and async
is strict:

1. **`svc.rs`** — async work. Owns the DB pool, spawns Tokio tasks, pushes
   results into `watch` (snapshots) and `broadcast` (events) channels.
2. **`state.rs`** — sync work. Holds the UI state in plain memory. On every
   tick, drains the channels from the service and updates local state. No
   `.await` ever.
3. **`input.rs`** — sync. Maps keypresses to state mutations. When an action
   needs I/O (save, send, vote), it calls a fire-and-forget method on the
   service. The result arrives through the channel on a future tick.
4. **`ui.rs`** — sync. Reads state, draws ratatui widgets. Pure rendering.

The tick loop (`app/tick.rs`) calls `tick()` on all states every 66ms, then
`render()` paints the frame. This is the heartbeat of the app — understand it
and you understand late.sh.

### Snapshots and events

Services expose two channel types:

- **`watch` (snapshots):** Latest full state. Receivers always see the most
  recent value. Used for things like vote tallies, room lists, leaderboard
  data.
- **`broadcast` (events):** Transient notifications. Used for new messages,
  vote errors, activity feed callouts.

State structs subscribe to both on init and drain them in `tick()`.

## Test rules

Tests are required for all changes. The boundary between unit and integration
tests is strict.

### Unit tests — inline in source files

```rust
#[cfg(test)]
mod tests {
    use super::*;
    // ...
}
```

- Pure logic only: no database, no services, no network, no async runtime.
- Good for: state transitions, input routing, formatting, validation, math.
- Live inside the source file they test — do NOT create `src/.../tests/`
  directories.

### Integration tests — in `tests/`

The test directory mirrors the source domain structure:

```
late-ssh/tests/
  helpers/mod.rs              # shared setup: test DB, app state, test users
  vote/
    main.rs                   # mirrors app/vote/
    svc.rs
  profile/
    main.rs                   # mirrors app/profile/
    svc.rs
  chat/
    main.rs                   # mirrors app/chat/
    svc.rs
    state.rs
    news.rs
  bonsai/
    main.rs                   # mirrors app/bonsai/
    svc.rs
  games/
    main.rs                   # mirrors app/games/ — all game tests compile here
    minesweeper/
      mod.rs
      svc.rs
    nonogram.rs
    sudoku.rs
    solitaire.rs
    twenty_forty_eight.rs
  app_smoke.rs                # app-wide smoke tests (top-level, not domain-specific)
  ssh_smoke.rs
  ws_smoke.rs

late-core/tests/
  db.rs                       # infrastructure
  model_macro.rs              # infrastructure
  user.rs                     # mirrors models/user.rs
  vote.rs                     # mirrors models/vote.rs
  article.rs                  # mirrors models/article.rs
  bonsai.rs                   # mirrors models/bonsai.rs
  minesweeper.rs              # mirrors models/minesweeper.rs
  chat/
    main.rs                   # mirrors models/chat_*.rs group
    room.rs
    member.rs
    message.rs
```

- Anything that touches the database, services, or cross-module orchestration.
- Always use `helpers::new_test_db()` (testcontainers) — never hardcoded
  connection strings.
- Mirror the domain structure: `tests/<domain>/svc.rs` tests `app/<domain>/svc.rs`.

### Quick rule of thumb

If you need a `Db`, `Service`, or any I/O — it's an integration test. Move it
to `tests/`.

## Using AI to contribute

This codebase was largely built with AI assistance and is set up for that
workflow.

[`CONTEXT.md`](CONTEXT.md) is the main file to feed your LLM. It contains
architecture, invariants, test strategy, module layout, and current work
context — everything an agent needs to make good decisions without reading every
source file first. Think of it as a project brief written for LLMs.

If you use an editor with AI integration (Cursor, Claude Code, Copilot, etc.),
point it at `CONTEXT.md` and `CONTRIBUTING.md` as initial context. The
combination covers both the "what" (architecture, constraints) and the "how"
(workflow, test rules, module patterns).

When your AI-assisted changes alter behavior covered in `CONTEXT.md`, update
that file too — it's a living document meant to stay in sync with the code.

## Picking what to work on

### New to Rust?

- Pick **small, well-scoped features**: a new input keybind, a UI tweak, a
  state transition fix.
- Using AI to help write Rust is encouraged — this codebase was largely built
  that way.
- Always add tests. Even a small inline `#[cfg(test)]` block for a new state
  transition is valuable.
- Look at existing domains like `sudoku` or `minesweeper` for patterns to
  follow.

### Comfortable with Rust?

- Larger features welcome: new game domains, service additions, new screens.
- Follow the domain module pattern above.
- Integration tests expected for anything touching the DB or services.
- Read `CONTEXT.md` for architecture details, invariants, and gotchas before
  diving in.

## Practical notes

- Keep changes focused.
- Preserve copyright notices and license notices.
- If you distribute a fork, do not present it as the official `late.sh`
  service.
