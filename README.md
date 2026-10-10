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
- Real-time global chat and shared activity feed
- Audio streaming via Icecast/Liquidsoap, played by the paired CLI or the public `/listen` page
- Terminal games including 2048, Sudoku, Nonograms, Minesweeper, and Solitaire
- Zork Trilogy door with an autosave and a manual save per edition
- Web frontend for landing, profiles, and the token-less `/listen` page
- Companion CLI for local audio playback and synced visualizer data

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

For development without Docker wrapping the Rust builds, you can run the
infrastructure in Docker and the apps natively:

```bash
docker compose up -d postgres icecast liquidsoap
cargo run -p late-ssh
cargo run -p late-web
```

Local host development can use Cargo's normal defaults, including the standard
repo-local `target/` directory. The `/app/target` path is only for Docker/dev
containers.

The Zork door is enabled in the development profile. Build its vendored asset
image once before building the Compose services (the current door base images
target amd64):

```bash
docker build --platform linux/amd64 -f docker/doors/zork.Dockerfile \
  -t ghcr.io/mpiorowski/late-sh/door-zork:2.56pre-r1 .
make start
```

Open Games (`3`) → **Zork Trilogy** → choose an edition. Each has automatic
progress and one deliberate `SAVE`; the six slots belong to your account.
`SAVE` accepts an optional description, `RESTORE` asks before returning to the
edition menu, and backtick steps out while the current game stays open.
After 20 minutes without game input, Continue resumes its last prompt.
See the [Zork component context](late-ssh/src/app/door/zork/CONTEXT.md) for the
save contract, standalone host configuration, container tests, and rollout order.

```bash
export CARGO_HOME=$HOME/.cargo
```

Use `mise install` to get the expected Rust toolchain, `mold` linker, and
`cargo-nextest`.

To test newspaper scrolling in the Docker stack, run `make seed-paper`, then
open `/paper` in the TUI. This replaces today's Reading and Outside sections
with clearly marked sample text and enables the paper. Use
`make seed-paper PAPER_PARAGRAPHS=5` for a shorter edition (default: 100;
range: 1–1000). Close and reopen `/paper` after reseeding; no restart is needed.

To test the Artboard gallery, run `make seed-artboard` after the current stack
has applied its migrations. It creates 11 test accounts and 12 distinguishable,
numbered ASCII pieces with applause and community/owner/moderator/admin rating
scenarios, and enables the gallery. The first three have enough applause for
monthly awards. Pieces 01–11 predate today UTC and qualify for the daily splash
queue. Piece 01 is selected for today, unless a non-fixture piece owns the day.
Use `make seed-artboard ART_SPLASH_PIECE=3` for an NSFW-rated splash (the drawing
itself is harmless), or choose any piece 1–11. Restart the SSH service with
`docker compose restart service-ssh` and reconnect to refresh its splash cache.

Open Artboard (`4`), choose **Newest** in the gallery rail, and press `n` on a
piece to vote. Fixture SSH identities are retained in the gitignored
`tmp/artboard-seed-keys/` directory. For example:

```bash
ssh -o IdentitiesOnly=yes -i tmp/artboard-seed-keys/art_artist1 -p 2222 localhost
```

Artboard opens behind a content disclaimer because the art may be NSFW.
Press `V` (**View**) for this visit, `A` (**Always View**) to turn off future
reminders, or `B` (**Back to Chat**) to return to Home (screen `1`). The default-on **Artboard
content disclaimer** toggle in Settings → Tweaks can turn reminders off or on.
During the newcomer tour the dialog does not appear: the tour box at the
Artboard stop offers `S` to show the art for that visit or Enter to skip to
Profiles, and neither changes the preference.

Accounts are `art_artist1`–`art_artist3`, `art_voter1`–`art_voter4`, `art_mod1`/
`art_mod2`, and `art_admin1`/`art_admin2`, each with a matching key filename.
Their tutorial is already marked completed, including when reseeding existing
fixture accounts.
They join the normal public auto-join rooms, including `#lounge`. From Home,
select `#lounge` and enter `/mod` in its composer to open the staff console.
Moderators and admins can also press `m` in a gallery list or full-piece view
to open the console with art safety help and that piece's safety record.
Rerunning restores these fixture pieces, applause, votes, marks, and staff
roles; it preserves account preferences and all other users and art. Changes
made while testing these pieces are reset by the next seed. Existing
leaderboard seeding is separate, so art seeding does not rewrite game scores.
The dev profile grants admin privileges to every session; set `force_admin`
to `false` in `late-ssh/src/config.rs` when testing role-based permissions.
Plain safety commands write moderator marks even for admin accounts; the
`admin` keyword explicitly selects the admin tier.

Inside the `/mod` console:

```text
artboard safety help
artboard safety view
artboard safety view @art_artist1
artboard safety view 0e77a39a
artboard safety nsfw 0e77a39a
artboard safety admin sfw 0e77a39a reviewed
artboard safety admin none 0e77a39a
artboard safety none 0e77a39a by @art_mod1
```

`view` shows a summary and review candidates; `@user` lists their hanging art,
and an ID shows one piece's full safety record. `none` clears your mark at the
selected tier. The per-piece record prints the full UUID on an `Art id:` line
for copying into commands. Removing another account's mark, at either tier, with `by`
is admin-only. Each account has one staff mark per piece; marking again
replaces its mark and tier.

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

- [CONTEXT.md](CONTEXT.md) — architecture, invariants, and working context. Written for LLMs — feed this to your AI editor for best results.
- [CONTRIBUTING.md](CONTRIBUTING.md) — workflow, test rules, module patterns, and AI-assisted development tips.
- [THEME.md](THEME.md) — how to contribute a new built-in SSH theme via PR.
- [late-cli/README.md](late-cli/README.md) — CLI-specific usage and behavior.
