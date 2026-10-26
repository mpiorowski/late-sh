# late.sh

A cozy command-line clubhouse for computer people. Chat, music, games, art, coding, and tech news. Connect with any SSH client!

```bash
ssh late.sh
```

`late.sh` is a terminal-first social app: real-time chat, music, games, news, profiles, and a shared always-on space you can enter from any SSH client.

## Status

This repository is the main codebase for `late.sh`.

- The project is open for source reading, local development, audits, and contributions.
- The public hosted `late.sh` service remains the canonical deployment.
- The code is source-available, not OSI open source, during the FSL protection period.

Read the details in [LICENSE](LICENSE), the plain-English policy in [LICENSING.md](LICENSING.md), and contribution rules in [CONTRIBUTING.md](CONTRIBUTING.md).

## What It Includes

- SSH TUI with dashboard, chat, profile, news, and arcade screens
- Mouse controls on Profiles (screen 5): click people, jobs, links, and form controls; wheel scrolls the hovered pane
- The events board on `7`: anyone posts an event, everyone sees it, `i` says I'm in; your own events sit beside it, private
- Real-time global chat and shared activity feed
- Audio streaming via Icecast/Liquidsoap, played by the paired CLI or the public `/listen` page
- Animated ASCII scenes in Zen tiles and away screensavers, with random and five-minute randomer modes
- Terminal games including 2048, Sudoku, Nonograms, Minesweeper, and Solitaire
- Zork Trilogy door with an autosave and a manual save per edition
- Web frontend for landing, profiles, and the token-less `/listen` page
- Companion CLI for local audio playback and synced visualizer data

IRC uses `#name` for public rooms and `##name` for private rooms. `/LIST`
shows both when you belong to the private room; `/join ##name` opens that
private room using your existing late.sh membership. Create private rooms
and invite members from the TUI. IRC JOIN only opens existing rooms.

IRC clients with `draft/chathistory` can retrieve joined-channel and existing
DM history, including missed conversations through `CHATHISTORY TARGETS`.
Requests support message IDs and timestamps, up to 300 stored messages per
page. Clients with `batch` and `server-time` that do not request
`draft/chathistory` receive the latest 25 messages after joining a channel.
Private membership and room bans apply to history as well as live chat.

## Workspace

The Rust workspace contains the main apps and standalone door hosts:

| Crate | Role |
|-------|------|
| `late-cli` | Companion CLI for local audio playback, paired controls, and visualizer sync |
| `late-core` | Shared domain code, database layer, migrations, and infrastructure helpers |
| `late-ssh` | SSH server and terminal UI application |
| `late-web` | Web server, landing page, profiles/gallery, stream proxy, and the public `/listen` page |
| `late-webview` | Helper process hosting the official YouTube IFrame Player for the CLI |
| `late-zork` | Private SSH/PTY host for the separately executed Frotz interpreter and Zork trilogy |

The other standalone door hosts are `late-bashquest`, `late-brogue`,
`late-codekeep`, `late-dcss`, `late-dopewars`, `late-nethack`, and `late-usurper`.

The stack is backed by PostgreSQL, Icecast, and Liquidsoap.

## Quick Start

Try the live service:

```bash
ssh late.sh
```

The SSH login name is discarded rather than used as a public handle. On the
first connection, a new account receives a random modifier-and-noun username;
the username can still be changed later in Settings.

Run it yourself (requires Docker):

```bash
git clone https://github.com/mpiorowski/late-sh
cd late-sh
make start
```

Then connect to your local instance:

```bash
ssh localhost -p 2222
```

That's it. Postgres, Icecast, and Liquidsoap all come up automatically.

## Companion CLI

Install the companion CLI for local audio playback and synced visualizer:

macOS / Linux / Termux:

```bash
curl -fsSL https://cli.late.sh/install.sh | bash
```

On Termux, the installer fetches the Android CLI build instead of the GNU/Linux one.

Windows PowerShell (x64):

```powershell
irm https://cli.late.sh/install.ps1 | iex
```

mise (from the GitHub Release archives):

```bash
mise use -g github:mpiorowski/late-sh
```

Nix / NixOS:

```bash
nix run github:mpiorowski/late-sh#late
```

Or build it from source:

```bash
mise install        # optional — sets up the expected Rust toolchain
cargo build --release --bin late
```

## Local Development

Everything runs in Docker Compose: the development profiles point at the
compose service names (`postgres`, `icecast`), so `late-ssh` and `late-web`
do not run outside it.

```bash
mise install        # rust, mold and cargo-nextest, as pinned in .mise.toml
make start          # copies .env.dev to .env and starts the stack
ssh localhost -p 2222
```

Personal keys (an AI key, a YouTube key) go in the gitignored `.env.local`,
which wins over `.env`. `make start-instance2` runs a second instance from
`.env.dev2`.

`late-ssh` runs the way production runs it, as a pod behind a copy of the
production SSH edge. A saved change that rebuilds the binary rolls out like a
deploy: a new pod starts, and once it is ready the old one drains. Your open
session stays on the old code behind the update banner until you reconnect
(the native CLI, `scripts/run_local_cli_native.sh`, moves it on its own after
30 s without input), and a change that does not compile leaves the old pod
serving.
`make fleet-rollout` rolls out by hand, `make fleet-ps` lists the pods, and
`make fleet-logs` follows them.

The Zork door is enabled in the development profile. Build its vendored asset
image once before building the Compose services (the door base images target
amd64):

```bash
docker build --platform linux/amd64 -f docker/doors/zork.Dockerfile \
  -t ghcr.io/mpiorowski/late-sh/door-zork:2.56pre-r1 .
make start
```

Fixtures for manual testing, each described in its domain's `CONTEXT.md`:
`make seed-artboard` (gallery pieces, applause and staff accounts),
`make seed-calendar` (the events board), `make seed-paper` (a long edition of
The Late Edition), `make seed-irc-history USERNAME=<you>` (IRC history for
Halloy and other clients), and `make seed-leaderboard`. The development
profile grants admin to every session; set `force_admin` to `false` in
`late-ssh/src/config.rs` to test role-based permissions.

## Verification

Run the local gate before opening a PR:

```bash
make check
```

This runs `cargo fmt --check`, then `cargo clippy` and `cargo nextest` across the
whole workspace with `--features otel`. It is the full pre-merge sweep and the
only place the otel (telemetry) build is exercised, since CI skips otel to stay
fast; otel breakage is caught here or at the release build, never in prod.
The local check starts a dedicated Compose Postgres project (`late-check`) on
port `55433` and points DB-backed tests at it via `TEST_DATABASE_URL`.
Override `CHECK_INSTANCE` or `CHECK_PG_HOST_PORT` if you need a parallel check
database.

## Contributing

Contributions are welcome, but read the project policy first:

- [CONTRIBUTING.md](CONTRIBUTING.md)
- [LICENSING.md](LICENSING.md)
- [LICENSE](LICENSE)

This repository uses DCO sign-off for commits:

```bash
git commit -s
```

If you distribute a fork, do not present it as the official `late.sh` service or use the project branding as your own.

## More Context

- [CONTEXT.md](CONTEXT.md): the map of the codebase, the rules that cross domains, and where each domain's own `CONTEXT.md` lives. Written for humans and LLMs alike; point your AI editor at it first.
- [CONTRIBUTING.md](CONTRIBUTING.md): workflow, test rules, module patterns, and AI-assisted development tips.
- [THEME.md](THEME.md): how to contribute a new built-in SSH theme via PR.
- [late-cli/README.md](late-cli/README.md): CLI-specific usage and behavior.
