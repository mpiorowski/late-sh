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

Tests are required for all changes. One rule: tests live next to the code they
exercise.

### Adjacent test files

Tests for `src/.../foo.rs` go in `foo_test.rs` beside it, wired from the
parent module file:

```rust
// in app/<domain>/mod.rs
#[cfg(test)]
mod svc_test;
```

```
app/<domain>/
  mod.rs          # pub mod declarations (+ cfg-gated test mods)
  svc.rs
  svc_test.rs     # tests for svc.rs — DB-backed tests included
  state.rs
  state_test.rs   # tests for state.rs
```

- This applies to every test kind, pure and DB-backed alike.
- Small pure unit tests may stay inline in the source file's own
  `#[cfg(test)] mod tests` block instead. Do NOT create `src/.../tests/`
  directories.
- DB access always goes through `late_core::test_utils::test_db()` and
  `create_test_user()`; never hardcoded connection strings.
- In `late-ssh`, the shared app-level harness lives in
  `src/test_helpers.rs` (test DB, app state, `make_app`, render/wait
  helpers); import it as `crate::test_helpers`.
- Whole-App flow tests live in `late-ssh/src/app/*_test.rs`
  (`smoke_test.rs`, `input_flow_test.rs`, `dashboard_flow_test.rs`, ...).
- In `late-core`, model tests sit next to the model:
  `src/models/user.rs` / `src/models/user_test.rs`.

### Quick rule of thumb

Every test goes next to the file it tests, external-boundary smoke tests
included: `src/ssh_test.rs` (real SSH client against a spawned server),
`src/api_test.rs` (real WebSocket pairing), `src/ircd/serve_test.rs` (real
IRC client over TCP), `src/app/door/rebels/proxy_test.rs` (stub SSH door
server). No crate has a `tests/` directory.

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
- DB-backed tests in adjacent `_test.rs` files expected for anything touching
  the DB or services.
- Read `CONTEXT.md` for architecture details, invariants, and gotchas before
  diving in.

## Practical notes

- Keep changes focused.
- Preserve copyright notices and license notices.
- If you distribute a fork, do not present it as the official `late.sh`
  service.
