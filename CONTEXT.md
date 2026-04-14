# late.sh Context

## Metadata
- Domain: late.sh - Terminal Clubhouse for Developers
- Primary audience: LLM agents working on this codebase, human contributors
- Last updated: 2026-04-13 (Blackjack moved to service-owned shared snapshot/event model; admin-gated while unfinished)
- Status: Active
- Stability note: Sections marked `[STABLE]` should change rarely. Sections marked `[VOLATILE]` are expected to change often.

---

## 0. Context Maintenance Protocol (LLM-First) [STABLE]

This file is the primary working context for the entire late.sh project.

- LLM agents should treat this as a living document and update it whenever meaningful behavior changes.
- If code and this file diverge, prefer updating this file quickly so future work stays reliable.
- Temporary or branch-specific behavior should be documented here with clear cleanup notes.

### Quick update checklist
- Refresh `Last updated` date
- Review `Current Work` and `Future Work`
- Validate `Critical Invariants`
- Update telemetry references if operation/event names changed
- Remove obsolete notes

### Freshness target
- Re-review this file regularly (every 2 weeks) to prevent context drift.

---

## 1. Summary [STABLE]

> A cozy terminal clubhouse for developers. Lofi beats, casual games, chat, and tech news - all via SSH.

`ssh late.sh` and you're in. Zero friction, terminal-first, always-on vibes.

The system is a Rust workspace with four crates (`late-cli`, `late-core`, `late-ssh`, `late-web`) backed by PostgreSQL, Icecast audio streaming, and Liquidsoap playlist management.

- **Primary entry points:** SSH server (russh on port 2222), HTTP API (axum on port 4000), Web server (axum on port 3000)
- **Main responsibilities:** Multi-screen TUI over SSH (Dashboard, Chat, News, Profile, The Arcade), genre voting, paired browser/CLI audio control plus visualizer, real-time global chat, link and YouTube sharing with AI summaries/ASCII thumbnails, interactive terminal games (2048, Sudoku, Nonograms, Minesweeper, Solitaire, admin-gated Blackjack), and a persistent bonsai tree in the sidebar.
- **Highest-risk areas:** SSH render loop backpressure, connection limiting, chat sync consistency, paired-client WS routing/state drift

---

## Test Strategy [STABLE]

### Scope and intent

- Cover both runtime apps: `late-ssh` and `late-web`.
- Keep most tests close to code under change (small, deterministic, focused).
- Use integration/smoke tests for boundary behavior across crates/services.

### Strict test boundary rules (required)

**Unit tests (`#[cfg(test)] mod tests` inside `src/` files):**
- MUST be pure logic only: no database, no services, no network, no async runtime required.
- Test input/output transformations, state transitions, parsing, formatting, validation math.
- If you need a `Db`, `Service`, `State`, or any I/O — it is NOT a unit test. Move it to `tests/`.
- Good examples: `rate_limit.rs` (in-memory limiter logic), `state.rs` (enum transitions), `input.rs` (key → action mapping).
- Preferred source layout for a domain is `src/.../<domain>/mod.rs` plus adjacent `state.rs`, `input.rs`, `ui.rs`, `svc.rs` as needed. `mod.rs` files must only contain `pub mod` declarations — never `pub use` re-exports.
- Keep pure unit tests inline in those source files. Do NOT create `src/.../<domain>/tests/` folders just to split unit tests.

**Integration tests (`late-ssh/tests/`, `late-web/tests/`, `late-core/tests/`):**
- MUST use testcontainers for database access — always go through `late_core::test_utils::test_db()` (or the `helpers::new_test_db()` wrapper in `late-ssh`).
- NEVER use `Db::new(&DbConfig::default())` or hardcoded connection strings in integration tests.
- `late-core::test_utils` owns shared test infrastructure: `test_db()`, `create_test_user()` (creates user + profile). Use these everywhere instead of rolling per-test user creation — except in `late-core` model tests that are testing `User::create` itself or don't need a profile.
- `late-ssh/tests/helpers/mod.rs` re-exports `create_test_user` from `late-core` and adds ssh-specific helpers (`test_config`, `test_app_state`, `make_app`, etc.). Domain test directories access these via `#[path = "../helpers/mod.rs"] mod helpers;` in their `main.rs`.
- Any test that touches DB, services, network, or cross-module orchestration belongs here.
- Preferred integration layout is domain-oriented under crate `tests/`, mirroring the source structure: `tests/<domain>/main.rs` with sibling `svc.rs`, `state.rs`, etc. as needed. `late-core` tests are named after their domain (`user.rs`, `vote.rs`, `chat/`).

**LLM enforcement:**
- On every code change, check: does this need a test? If yes, classify it strictly as unit or integration per the rules above.
- LLM agents must NOT run `cargo test`, `cargo nextest`, or `cargo clippy` in this repo. The human owner runs verification manually because those commands are too blocking in normal agent workflows.
- Do NOT put integration-flavored tests (DB calls, service interactions, spawning tasks) inside `#[cfg(test)]` module blocks in `src/` files.
- Do NOT invent extra source-side test directory structure when inline `#[cfg(test)] mod tests` is sufficient; reserve directory splits for crate-level integration tests under `tests/`.
- If a test is intentionally deferred (WIP/incomplete dependency), document the gap and cleanup plan in PR/context notes.

### Preferred test pyramid for this repo

1. Unit tests in module files — pure logic only, no I/O (`state.rs`, `input.rs`, `ui.rs`, `rate_limit.rs`).
2. Integration tests in `late-ssh/tests/` and `late-web/tests/` — real DB via testcontainers, shared helpers.
3. Workspace-wide checks before merge (`fmt`, `clippy`, `nextest`).

### Per-app guidance

For `late-ssh`:

- `app/*/state.rs`: unit tests for transition rules, event drains, selection/filter logic (includes profile field navigation).
- `app/*/input.rs`: unit tests for key routing and mode guards.
- `app/*/ui.rs`: unit tests for pure formatting/layout helpers only; avoid brittle pixel snapshots.
- `app/*/{mod,state,input,ui,svc,model}.rs`: keep the domain module flat and predictable; add pure unit tests inline in the relevant file instead of under `src/app/*/tests/`.
- `app/render.rs` / `app/tick.rs`: integration tests for orchestration (needs services/DB → goes in `tests/`).
- `app/*/svc.rs`: integration tests in `tests/<domain>/svc.rs` (needs real DB).
- Integration test directories mirror the source domain structure: `tests/<domain>/main.rs` with split files like `svc.rs`, `state.rs` as needed. Game tests live under `tests/games/<game>.rs`.
- `ssh.rs` / `api.rs`: smoke tests in `tests/ssh_smoke.rs` / `tests/ws_smoke.rs`.

For `late-web`:

- Handler/route behavior in `late-web/tests/*` with request/response assertions.
- Page/model transformations as unit tests under `src/pages/*` (pure logic only).
- Error mapping tests in `src/error.rs` for stable status/body behavior (pure logic only).

### Command policy

- LLM agents must not run tests or lint gates locally. Do not run `cargo test`, `cargo nextest`, or `cargo clippy`; leave all verification to the human owner.
- If code changes would normally merit verification, note the expected command(s) in handoff instead of running them.
- The human owner may still use the full CI-equivalent gate locally:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --workspace --all-targets
```

### Known environment caveats

- Some integration/smoke tests require Docker/testcontainers and may fail in restricted sandboxes.
- If a feature area is intentionally WIP, temporary lint/test gaps are acceptable only when explicitly documented and tracked for cleanup.
- **Tool bootstrap:** The repo now includes `.mise.toml` with `rust`, `mold`, and `cargo-nextest`. Prefer `mise install` before local development so the expected toolchain and test runner are available.
- **Cargo environment setup:** For local host development, use Cargo's normal defaults, including the standard repo-local `target/` directory. Docker/dev containers still use `/app/target` via container configuration. `CARGO_HOME=$HOME/.cargo` remains a valid override when an environment needs it, but it is not a repo-wide requirement.
- **`LATE_FORCE_ADMIN=1`** — dev-only escape hatch: OR'd with `users.is_admin` at session init (`late-ssh/src/ssh.rs`), so every SSH session lands as admin. Must stay `0` in prod — enforced by `required_bool` and hardcoded to `"0"` in `infra/service-ssh.tf`.

---

## 2. Architecture (with Graphs) [STABLE]

### 2.1 Component map

```mermaid
flowchart LR
    subgraph Server["late.sh Server"]
        SSH["SSH Server<br/>(russh)"]
        API["HTTP API<br/>(axum)"]
        WEB["Web Server<br/>(axum)"]
        IC["Icecast<br/>audio stream"]
        LS["Liquidsoap<br/>playlist mgr"]
        PG[(PostgreSQL)]
        SR["SessionRegistry<br/>token → mpsc"]
        PCR["PairedClientRegistry<br/>token → WS sender + state"]
    end

    SSH --> App["TUI App<br/>(ratatui)"]
    API --> SR
    API --> PCR
    SSH --> PG
    WEB --> API
    LS --> IC
    App --> SR
    App --> PCR

    Browser["Browser<br/>/connect/{token}"] <-->|"WS viz + control + state"| API
    Browser -->|"audio stream"| IC
    CLI["late CLI<br/>local audio"] <-->|"WS viz + control + state"| API
    Terminal["User Terminal<br/>(SSH client)"] <-->|"SSH channel"| SSH
    Terminal <-->|"opens URL"| Browser
```

### 2.2 SSH session lifecycle

```mermaid
sequenceDiagram
    participant T as Terminal
    participant S as SSH Server
    participant A as App (TUI)
    participant R as SessionRegistry
    participant B as Paired Client
    participant DB as PostgreSQL

    T->>S: SSH connect
    S->>S: Check conn limits (global + per-IP)
    S->>DB: Find/create user by fingerprint
    S->>S: Subscribe activity_feed (broadcast)
    S->>S: Publish login ActivityEvent
    S->>A: Create App with SessionConfig (is_new_user, activity_feed_rx)
    S->>R: Register(token, mpsc::tx)
    S->>T: Alt screen + render loop (15fps, splash screen + welcome overlay shown for every session)
    T->>A: Keyboard input
    A->>DB: Service calls (vote/chat/news)
    B->>R: WS /api/ws/pair?token=...
    B->>R: Viz frames + client_state
    R->>A: mpsc → VizFrame
    A->>B: mute / volume control
    A->>T: Rendered frame bytes
```

### 2.3 Paired client control + visualizer flow

```mermaid
flowchart TD
    B["Browser / CLI paired client"] -->|"viz + client_state"| WS["WebSocket<br/>/api/ws/pair"]
    WS -->|"SessionMessage::Viz"| SR["SessionRegistry"]
    WS -->|"client state"| PCR["PairedClientRegistry"]
    SR -->|"mpsc channel"| APP["App.tick()"]
    APP --> VIZ["Visualizer.update()"]
    APP -->|"m / +/-"| PCR
    PCR -->|"toggle_mute / volume_up / volume_down"| WS
    VIZ --> RENDER["Sidebar render<br/>thin cyan bars"]
```

### 2.4 Service pub/sub model

```mermaid
flowchart LR
    VS["VoteService"] -->|"watch"| VSS["VoteSnapshot"]
    VS -->|"broadcast"| VSE["VoteEvent"]
    CS["ChatService"] -->|"watch"| CSS["ChatSnapshot"]
    CS -->|"broadcast"| CSE["ChatEvent"]
    AS["ArticleService"] -->|"watch"| ASS["ArticleSnapshot"]
    AS -->|"broadcast"| ASE["ArticleEvent"]
    NS["NotificationService"] -->|"watch"| NSS["NotificationSnapshot"]
    NS -->|"broadcast"| NSE["NotificationEvent"]
    CS -->|"holds"| NS
    PS["ProfileService"] -->|"watch"| PSS["ProfileSnapshot"]
    PS -->|"broadcast"| PSE["ProfileEvent"]
    BJS["BlackjackService"] -->|"watch"| BJSS["BlackjackSnapshot"]
    BJS -->|"broadcast"| BJSE["BlackjackEvent"]
    AF["Activity Feed"] -->|"broadcast"| AFE["ActivityEvent"]
    LB["LeaderboardService"] -->|"watch"| LBS["Arc&lt;LeaderboardData&gt;"]

    VSS --> APP["App TUI<br/>mixed: global + per-user subscriptions"]
    VSE --> APP
    CSS --> APP
    CSE --> APP
    ASS --> APP
    ASE --> APP
    NSS --> APP
    NSE --> APP
    PSS --> APP
    PSE --> APP
    BJSS --> APP
    BJSE --> APP
    AFE --> APP
    LBS --> APP
```

- `VoteService` (in `app/vote/svc.rs`), `ChatService` (in `app/chat/svc.rs`), `ArticleService` (in `app/chat/news/svc.rs`), and `NotificationService` (in `app/chat/notification_svc.rs`) expose shared `watch` snapshots (`subscribe_state()` / `subscribe_snapshot()`).
- `ProfileService` (in `app/profile/svc.rs`) exposes per-user `watch` snapshots backed by service-owned maps (`subscribe_snapshot(user_id)`).
- `LeaderboardService` exposes a shared `watch::Receiver<Arc<LeaderboardData>>` refreshed from DB every 30s. Contains today's champions, streak leaders, per-user streak map (used for chat badges and profile achievements), all-time high scores (Tetris + 2048), and chip leaders (top balances).
- `ChipService` (in `app/games/chips/svc.rs`) manages the Late Chips economy: `ensure_chips(user_id)` grants the daily 500-chip stipend on login, `grant_daily_bonus_task(user_id, difficulty_key)` awards 50/100/150 chips on daily puzzle completion. All 4 daily game services hold a `ChipService` clone and call it in `record_win_task()`.
- `BlackjackService` (in `app/games/blackjack/svc.rs`) owns the current shared blackjack table in-memory, publishes `BlackjackSnapshot` via `watch`, and emits per-user `BlackjackEvent` messages via `broadcast`. The SSH app's blackjack state is now a thin client wrapper, not the game authority.
- Events remain `broadcast` for all subscribers; targeted variants carry `user_id` and are filtered in UI state.

### 2.5 TUI Rendering and State Architecture (Sync vs Async Boundary)

To maintain a buttery-smooth 15-60 FPS over SSH, the architecture strictly separates synchronous UI rendering from asynchronous business logic:

1. **The Setup (`ssh.rs` / `main.rs`)**
   When a new SSH client connects, a `SessionConfig` is built containing global *Services* (like `VoteService`, `ArticleService`, which hold DB pools and API keys).
2. **The Initialization (`app/state.rs`)**
   Inside `App::new()`, these services are used to create the *UI States* (e.g., `ChatState` which owns the `news::State` and `notifications::State`). Each UI State stores its `user_id`, subscribes to service channels, and spawns a per-user background refresh task (aborted on `Drop`).
3. **The Sync Loop (`app/tick.rs`)**
   Every 66ms, `App::tick()` runs. It calls `tick()` on all UI states. This:
   - Drains the channels to instantly update local memory state (e.g., `Vec<Article>`). User-targeted events are filtered by `self.user_id`.
4. **The Paint Job (`app/render.rs` -> `ui.rs`)**
   Immediately after the tick, `App::render()` runs. It passes the purely synchronous UI state directly to the draw functions. The UI just reads local memory and draws boxes. No `.await`, no freezing.
5. **The User Action (`app/input.rs`)**
   Input handlers modify the synchronous UI State (like moving cursors). When an action requires I/O (like hitting `Enter` to save), the input handler fires a fire-and-forget method on the Service. The Service spawns a Tokio task to do the DB/API work, pushes the result to the channel, and the UI catches it on the next 66ms tick.

### 2.6 Audio infrastructure

```mermaid
flowchart LR
    LOCAL["Local .m3u<br/>CC0/CC-BY music"] -->|"playlist"| LS
    LS["Liquidsoap<br/>port 1234 telnet"] -->|"MP3 128kbps"| IC["Icecast<br/>port 8000"]
    IC -->|"/stream"| WEBSTREAM["late-web<br/>/stream proxy"]
    WEBSTREAM -->|"stable MP3 stream"| B["Browser / CLI audio"]
    IC -->|"/status-json.xsl"| FETCH["NowPlaying fetcher<br/>(10s poll)"]
    FETCH -->|"watch channel"| APP["App sidebar"]
    VS["VoteService"] -->|"vibe.set genre"| LS
```

#### Music licensing strategy [VOLATILE]

The audio stack is local-playlist-only. Liquidsoap reads curated local `.m3u` playlists backed by files in `/music`, then streams the result through Icecast. There are no third-party live radio upstreams in the current design.

#### Source priority

Genres now use `mksafe(local_playlist)` only. Each playlist uses `mode="randomize"` + `loop=true` to shuffle all tracks and play through before re-shuffling, with `check_next` guards against back-to-back repeats at loop boundaries.

**Migration status (April 2026):**
- Lofi: **DONE** — 50 tracks, all CC0/CC-BY
- Ambient: **DONE** — 20 curated CC-BY 4.0 tracks
- Classical: **DONE** — 40 curated public-domain Musopen tracks
- Jazz: local-only for now; still the thinnest genre and a likely removal candidate

There are no live upstream radio sources in `radio.liq`.

#### Current local music library [VOLATILE]

Music binaries live in Cloudflare R2 (bucket configured via `MUSIC_BUCKET` GitHub var), synced to the Liquidsoap PVC at `/music/` during infra deploys by the `sync_music` job in `deploy_infra.yml`. Playlists are `.m3u` files in `infra/liquidsoap/` using Liquidsoap `annotate:` format and remain in git.

#### Music library [VOLATILE]

All music is CC0 or CC-BY licensed. CC-BY tracks require attribution — handled automatically via `annotate:` metadata in `.m3u` files flowing through ICY metadata to the sidebar "now playing" display.

Detailed track lists and source URLs live in [`MUSIC.md`](MUSIC.md).

- Lofi: done, 50 tracks, mixed `CC0` and `CC-BY 4.0`
- Ambient: done, 20 curated `CC-BY 4.0` tracks from Amarent, Ketsa, and The Imperfectionist
- Classical: done, 40 curated public-domain tracks from Musopen / Internet Archive
- Jazz: planned, source targets are HoliznaCC0, Kevin MacLeod, and Ketsa

Playlist generation uses curated manifests in `scripts/fetch_cc_music.py`, preserves `duration` in `annotate:` metadata, and can intentionally limit a playlist to the curated set even if older files still exist on disk.

#### Future music sources [VOLATILE]

**High-potential (verified CC0/CC-BY, not yet downloaded):**
- HoliznaCC0: 571 total tracks across ~50+ albums, all CC0. Full discography: https://freemusicarchive.org/music/holiznacc0/discography
- Ketsa: large catalog (lofi, jazz, soul, ambient, downtempo), CC-BY. Album "CC BY: FREE TO USE FOR ANYTHING" has 70 tracks: https://freemusicarchive.org/music/Ketsa/cc-by-free-to-use-for-anything
- John Bartmann: "Public Domain Soundtrack Music: Album One" (CC0) on Bandcamp
- Kevin MacLeod: 359 tracks (CC-BY): https://kevinmacleod.bandcamp.com/album/complete-collection-creative-commons
- FMA public domain search (9,000+ tracks): https://freemusicarchive.org/search?adv=1&music-filter-public-domain=1

**Not selected for the local library:**
- **Pixabay:** custom license, not ideal for a standalone music stream
- **Chad Crouch:** CC BY-NC + commercial licensing split
- **Blue Dot Sessions:** CC BY-NC only
- **Kai Engel:** mixed CC-BY/CC-BY-NC catalog, licensing instability after July 2025
- **Classicals.de:** license terms unclear

#### Music storage [STABLE]

Music binaries live in Cloudflare R2, synced to the Liquidsoap PVC during infra deploys (`sync_music` job in `deploy_infra.yml`). Git is the source of truth for playlists, licenses, and source URLs — not for binaries. ConfigMap changes (playlists, radio.liq, icecast.xml) trigger automatic rollouts via `config_hash` annotations on deployment templates — no explicit restart job needed.

#### Download tooling

- `scripts/fetch_cc_music.py` — Downloads from Bandcamp (via yt-dlp) and Internet Archive (via urllib), generates `.m3u` playlists with ffprobe metadata. Supports `--genre` and `--m3u-only` flags.
- Ambient uses a curated FMA manifest inside `scripts/fetch_cc_music.py` instead of the older broad-source ambient target.
- FMA CDN scrape pattern: FMA pages embed `fileUrl` in HTML as `https://files.freemusicarchive.org/storage-freemusicarchive-org/tracks/{hash}.mp3`. These are direct-downloadable without authentication. Extract with regex on the page source (see `/tmp/fetch_fma_tracks.py` for reference).
- Dependencies: `yt-dlp` (installed via pipx), `ffmpeg`, `ffprobe`, `python3`.

#### Metadata handling

Local playlist files retain full annotated metadata including duration (when present in ID3 tags). The `rewrite_np_metadata` function in `radio.liq` formats "now playing" as `Artist - Title | Duration` for the sidebar. Internet streams provided ICY metadata with no duration; local files may or may not have duration depending on the source.

### 2.7 Nonogram Generation and Runtime Split

Nonograms intentionally use an offline generation pipeline instead of generating puzzles during SSH sessions.

1. **Offline generation (`late-core`)**
   `late-core/src/bin/gen_nonograms.rs` generates puzzle banks by size (`10x10`, `15x15`, `20x20`), applies per-size difficulty profiles (`10x10` easy, `15x15` medium, `20x20` hard), validates every accepted candidate with `number-loom`, regenerates until each pack reaches the requested count, and writes only the final JSON assets (validation scratch files are cleaned up automatically).
2. **Shared schema (`late-core`)**
   `late-core/src/nonogram.rs` owns the portable JSON contract (`NonogramPuzzle`, `NonogramPack`, `NonogramPackIndex`), clue derivation, pack validation, and deterministic daily puzzle selection by date.
3. **Static assets (`late-ssh/assets/nonograms/`)**
   Generated packs live under `late-ssh/assets/nonograms/` with one `index.json` plus one pack file per size (`10x10.json`, `15x15.json`, `20x20.json`).
4. **Runtime loading (`late-ssh`)**
   `late-ssh/src/app/games/nonogram/state.rs` loads packs at server startup. SSH sessions only read the already-generated bank; they do not invoke `number-loom` or generate puzzles on demand.
5. **Daily selection**
   The runtime picks one puzzle per size deterministically from the prebuilt bank using the UTC date and the pack `size_key`. This keeps the "daily" experience stable without storing generator state in Postgres.
6. **Runtime persistence**
   `late-ssh` now persists one `daily` and one `personal` slot per user and `size_key` in `nonogram_games`. `d` restores the date-based daily puzzle for the selected size, `p` restores that size's saved personal board, and `n` regenerates a fresh personal puzzle from the current pack.
7. **Daily completion tracking**
   `late-ssh` also records a binary daily completion fact per user, size, and UTC date in `nonogram_daily_wins`. This is intentionally separate from board state and does not track score or time.

Current invariant:
- `late-ssh` is runtime-only for nonograms: read JSON assets, select a puzzle, render/play it, and persist per-user progress. Generation belongs in `late-core/src/bin/gen_nonograms.rs`, not in the SSH hot path.

### 2.8 Local CLI MVP

`late-cli/src/main.rs` is the standalone local launcher (companion CLI).

1. **Standalone crate** — `late-cli` has zero dependency on `late-core`. `AnalyzerConfig` is inlined so the crate can be built independently.
2. **Single-process audio path** — It opens the audio stream once, decodes MP3 locally with symphonia, feeds local playback via `cpal`, and derives visualizer/analyzer data from that same decoded stream. Native output sample rate is preferred; otherwise it chooses the nearest supported device rate and applies in-process linear resampling.
3. **PTY + token handshake** — It launches the normal SSH TUI through a pty, intercepts a one-line `LATE_SESSION_TOKEN=<base64url-uuid-v7>` banner emitted only for `LATE_CLI_MODE=1` sessions, and keeps stdin blocked until that banner phase is complete so pre-handshake keystrokes do not leak into the app. The token is an unpadded URL-safe base64 encoding of the UUID bytes, which keeps QR/connect URLs shorter than canonical UUID text.
4. **Identity + pairing** — Client identity remains SSH-key based, with an interactive fallback that can generate `~/.ssh/id_late_sh_ed25519` when no local key exists. The CLI then uses `/api/ws/pair` to forward analyzer frames, accept paired control commands, and report `client_state { client_kind, muted, volume_percent }`.
5. **Distribution + platform notes** — The landing page advertises both `curl -fsSL https://cli.late.sh/install.sh | bash` and a source build path. CLI releases go through `deploy_cli`. The launcher is currently Unix-first (`ssh` + `cpal`), supports Linux/macOS/likely WSL, and surfaces targeted WSL audio hints by checking `DISPLAY`, `WAYLAND_DISPLAY`, and `PULSE_SERVER` when startup fails.

Current invariants:
- The installer defaults to `https://cli.late.sh`, and the CLI supports `-v` / `--verbose` for stderr debug logging.
- Browser and CLI share the same paired-client protocol, so the TUI can show target kind plus live mute/volume state in the sidebar.
- The CLI currently requires a working local audio output device to fully start; native Windows support would require replacing the external `ssh` launch path.

---

## 3. File Tree (Curated) [STABLE]

```text
late-sh/
├── Cargo.toml                  # Workspace: late-cli, late-core, late-ssh, late-web
├── CONTEXT.md                  # This file
├── OPEN_README.md              # README for the public mirror repo
├── docker-compose.yml          # Dev stack: ssh, web, postgres, icecast, liquidsoap
├── Makefile / Dockerfile       # Local dev + image build entry points
├── scripts/                    # Seed helpers, local CLI runner, CLI artifact builder
├── late-core/
│   └── src/
│       ├── db.rs               # DB pool + migrations
│       ├── model.rs            # model! + user_scoped_model! macros
│       ├── models/             # Core DB-backed domain entities
│       ├── nonogram.rs         # Shared pack schema, clue derivation, daily selection
│       ├── rate_limit.rs       # Sliding-window per-IP limiter
│       └── test_utils.rs       # testcontainers DB helpers
├── late-ssh/
│   ├── src/
│   │   ├── main.rs             # Starts SSH + API + background loops
│   │   ├── ssh.rs              # russh server + render loop
│   │   ├── api.rs              # /api/* + /api/ws/pair
│   │   ├── session.rs          # SessionRegistry + PairedClientRegistry
│   │   ├── state.rs            # Shared app state, activity, presence
│   │   └── app/
│   │       ├── ai/             # AI services: bot/graybeard + summarization
│   │       ├── bonsai/         # Persistent bonsai tree state, service, and UI
│   │       ├── chat/           # Rooms, messages, mentions, and embedded news feed
│   │       ├── dashboard/      # Landing screen layout + shortcuts
│   │       ├── games/          # Arcade hub, leaderboards, and game subdomains
│   │       ├── profile/        # Username/profile settings and stats
│   │       └── vote/           # Genre vote state, service, and Liquidsoap control
│   ├── assets/nonograms/       # Prebuilt puzzle packs
│   └── tests/                  # Integration/smoke tests grouped by feature
├── late-cli/
│   └── src/main.rs             # Standalone CLI: audio + analyzer + SSH + WS pairing
├── late-web/
│   ├── src/
│   │   ├── main.rs / lib.rs    # Web entrypoint + router
│   │   ├── config.rs           # Web config
│   │   ├── error.rs            # App error mapping
│   │   └── pages/              # Landing, connect flow, stream proxy, dashboard
│   └── static/                 # Tailwind output/source
└── infra/
    ├── icecast/icecast.xml     # Icecast config
    └── liquidsoap/             # Radio config + local fallback playlists
```

---

## 4. Core Contracts [STABLE]

### 4.1 Public/API contracts

**SSH API (late-ssh, port 4000):**
- `GET /api/health` - DB health check
- `GET /api/now-playing` → `NowPlayingResponse { current_track, listeners_count, started_at_ts }`
- `GET /api/status` → `StatusResponse { online, message, version }`
- `GET /api/ws/pair?token={token}` - WebSocket upgrade for paired browser/CLI control + viz

**WS payloads (client → server):**
- `{ "event": "heartbeat" }`
- `{ "event": "viz", "position_ms": u64, "bands": [f32; 8], "rms": f32 }`
- `{ "event": "client_state", "client_kind": "browser" | "cli", "muted": bool, "volume_percent": u8 }`

**WS payloads (server → client):**
- `{ "event": "toggle_mute" }`
- `{ "event": "volume_up" }`
- `{ "event": "volume_down" }`

**Web routes (late-web, port 3000):**
- `GET /` - Landing page: late.sh branding, `ssh late.sh` CTA (click-to-copy), feature list, now-playing/listeners via htmx
- `GET /{token}` - Audio pairing page: WS connection to terminal session, local audio playback, paired mute/volume control, Web Audio analyzer for TUI visualizer, now-playing via htmx
- `GET /status?pairing={bool}` - htmx fragment: now-playing track + listener count (fetched from SSH API internally). `pairing=false` for landing footer, `pairing=true` for pairing detail view. Polled every 5s.
- `GET /dashboard` - Live metrics page (HTMX, internal/testing)
- `GET /test` - Error simulation endpoint
- All other routes → redirect to `/`

**Service stream contracts (internal):**
- `VoteService::subscribe_state()` (in `app::vote::svc`), `ChatService::subscribe_state()` (in `app::chat::svc`), `ArticleService::subscribe_snapshot()` (in `app::chat::news::svc`), `NotificationService::subscribe_snapshot()` (in `app::chat::notification_svc`) → shared `watch::Receiver<...Snapshot>` (durable latest state)
- `ProfileService::subscribe_snapshot(user_id)` → per-user `watch::Receiver<...Snapshot>` (durable latest state)
- `ProfileService::prune_user_snapshot_channel(user_id)` → explicit cleanup hook called from UI state `Drop`; removes idle per-user snapshot senders
- `LeaderboardService::subscribe()` → `watch::Receiver<Arc<LeaderboardData>>` (shared, refreshed every 30s from DB; contains today's champions, streak leaders, per-user streak map for badge computation)
- `subscribe_events() → broadcast::Receiver<...Event>` - transient events/notices

### 4.2 Auth and scope model

- **Identity:** SSH key fingerprint → `users` table (`User::find_by_fingerprint`)
- **Open access:** `LATE_SSH_OPEN=true` enables auth, but only public-key auth is accepted; password and keyboard-interactive are always rejected
- **User scoping:** Votes are scoped to `user_id` (FK to `users.id`)
- **Chat scoping:** Rooms visible via membership (`ChatRoom::list_for_user`, `ChatRoomMember`)
- **Auto-join:** Public rooms with `auto_join=true` are joined on first connection (`auto_join_public_rooms`); `/create` makes auto-join rooms everyone can leave, `/create-room` (admin) makes permanent rooms nobody can leave
- **Multi-tenant isolation:** All user data queries filter by `user_id`; no cross-user reads

### 4.3 Data model and key enums

**Entities (all use UUID v7 PKs, `id`/`created`/`updated` built into `model!` macro, lists default to `ORDER BY created DESC`):**

| Entity | Table | Key constraints |
|--------|-------|----------------|
| User | `users` | `fingerprint` UNIQUE, `settings` JSONB (holds `ignored_user_ids: [uuid]` — keyed by id, not username, so renames don't drop ignores) |
| Vote | `votes` | `user_id` UNIQUE (one vote per user per round) |
| ChatRoom | `chat_rooms` | `kind` IN (general, language, dm, topic), complex constraints |
| ChatRoomMember | `chat_room_members` | PK `(room_id, user_id)`, `last_read_at` |
| ChatMessage | `chat_messages` | `body` 1-2000 chars |
| Article | `articles` | `url` UNIQUE, `user_id` FK |
| ArticleFeedRead | `article_feed_reads` | `user_id` PK/FK, per-user news read checkpoint |
| Notification | `notifications` | `user_id`+`actor_id` FK to users, `message_id` FK to chat_messages, `room_id` FK to chat_rooms, `read_at` nullable, CHECK(user_id<>actor_id) |
| Profile | `profiles` | `user_id` UNIQUE, username (trimmed length 1-32, case-insensitive UNIQUE, canonical public handle), dashboard widget toggles |
| SudokuDailyWin | `sudoku_daily_wins` | `UNIQUE(user_id, difficulty_key, puzzle_date)`, score tracked |
| NonogramDailyWin | `nonogram_daily_wins` | `UNIQUE(user_id, size_key, puzzle_date)`, binary completion |
| MinesweeperGame | `minesweeper_games` | `UNIQUE(user_id, difficulty_key, mode)`, stores seeded mine_map + player_grid + lives (3-life system) |
| MinesweeperDailyWin | `minesweeper_daily_wins` | `UNIQUE(user_id, difficulty_key, puzzle_date)`, best score (lives remaining) retained |
| SolitaireGame | `solitaire_games` | `UNIQUE(user_id, difficulty_key, mode)`, stores seeded stock/waste/foundations/tableau |
| SolitaireDailyWin | `solitaire_daily_wins` | `UNIQUE(user_id, difficulty_key, puzzle_date)`, best score retained |
| BonsaiTree | `bonsai_trees` | `user_id` UNIQUE, growth_points, last_watered DATE, seed BIGINT, is_alive BOOLEAN |
| BonsaiGrave | `bonsai_graveyard` | `user_id` FK (not unique — multiple deaths), survived_days, died_at |
| UserChips | `user_chips` | `user_id` PK/FK, `balance` BIGINT (floor=100), `last_stipend_date` DATE |

**Key enums:**
- `Genre`: `Lofi`, `Classic`, `Ambient`, `Jazz` (vote/service/liquidsoap)
- `Screen`: `Dashboard`, `Chat`, `Games`, `Profile` (cycle: `Dashboard -> Chat -> Games -> Profile -> Dashboard`; News and Mentions are synthetic room-like entries within Chat, not separate screens, each with their own persisted unread state)
- `ChatRoom.kind`: `general` (slug=general), `language` (slug=lang-{code}), `topic` (user/admin created), `dm` (canonical user pair)
- `ChatRoom.visibility`: `public`, `private`, `dm`

### 4.4 Error model

- **Service errors:** Propagated via `anyhow::Result`, surfaced as `VoteEvent` / `ChatEvent` error variants
- **Chat:** `SendSucceeded` / `SendFailed` with `request_id` for composer feedback
- **Votes:** `VoteEvent::Error { user_id, message }` for unknown user
- **SSH:** Connection rejected on limit exceeded; render frame drops logged
- **Web:** `AppError::Internal` / `AppError::Render` → HTTP 500 with template fallback

---

## 5. Telemetry and Observability [STABLE]

- **Architecture:** 100% native OpenTelemetry (OTLP) pipeline powered by `opentelemetry` and `tracing` crates, routed through an OpenTelemetry Collector into a pure VictoriaMetrics backend.
- **Traces (`VictoriaTraces`):** Distributed tracing spans generated via `#[tracing::instrument]`. The Collector automatically generates RED metrics (Rate, Errors, Duration) from these spans using the `spanmetrics` connector.
- **Service graph requirement:** VictoriaTraces must run with `--servicegraph.enableTask=true` for the Grafana service graph / dependencies view to populate from trace relationships.
- **Logs (`VictoriaLogs`):** Structured JSON logs bypassing stdout completely via `opentelemetry-appender-tracing`. Trace IDs and Span IDs are natively embedded for full cross-correlation in Grafana.
- **Metrics (`VictoriaMetrics`):** Custom metrics (e.g., counters) pushed directly via OTLP PeriodicReader, alongside the RED metrics generated by the Collector.
- **HTTP server spans:** `late-web` wraps the router with request middleware that emits `otel.kind=server` spans and records `http.request.method`, `http.route`, `url.path`, and `http.response.status_code`; 5xx responses set `otel.status_code=ERROR`.
- **Trace propagation:** `late-core::telemetry::init_telemetry()` installs the W3C Trace Context propagator. `late-web` injects trace headers on outbound `/api/now-playing` requests, and `late-ssh` extracts incoming headers on API requests so cross-service traces can form real parent/child relationships.
- **Dashboard playground metric:** The interactive dashboard counter posts to `/dashboard/counter`, which emits `dashboard_counter_actions_total{action=...}` for Grafana testing.
- **Grafana provisioning invariant:** The metrics datasource uses the stable UID `victoriametrics`; provisioned dashboards must reference that UID instead of Grafana-generated datasource IDs.
- **Console Output:** Local dev uses `tracing_subscriber::fmt` with `RUST_LOG=info,late_web=debug,late_ssh=debug,late_core=debug`.
- **DB health:** `GET /api/health` endpoint, `Db::health()` method
- **Connection counts:** Per-IP tracking in `State.conn_counts`, global via semaphore. When `LATE_SSH_PROXY_PROTOCOL=true`, SSH per-IP limits use the client IP from PROXY protocol.
- **Presence/listener count source:** TUI sidebar online/users and `/api/now-playing.listeners_count` both use `State.active_users`.

---

## 6. Current Work [VOLATILE]

In progress:
- Blackjack now uses service-owned shared state with `watch` snapshots and `broadcast` events, but it is still a single shared table with one active player at a time.
- Blackjack remains admin-gated in the arcade until multi-seat/table semantics land.
- See PLAN.md for the concrete multiplayer follow-up roadmap.

Future:
- **Nonograms (v2)**: Replace random generation with pixel-art-to-nonogram pipeline or bulk-curate from webpbn.com.
---

## 7. Future Work & Roadmap [VOLATILE]

1. Chat upgrades: DMs/private rooms, invites/moderation, better backlog pagination

Known gaps/risks:
- Online/listener metrics are app-level presence (`active_users`, includes @bot and @graybeard), not true Icecast listener analytics
- Time remaining is approximate (up to 5s polling delay on track change)
- No external metrics or alerting system
- **Single-replica assumption:** Several structures are purely in-memory and not shared across processes (see multi-replica notes below)
- **Stateful VT parsing in `late-ssh/src/app/input.rs`:** SSH input now runs through a persistent `vte::Parser`, so CSI/SS3 sequences and bracketed paste survive split russh reads instead of assuming the whole escape sequence lands in one chunk. That removes the old split-paste failure where `[200~` / `[201~` residue or embedded newlines could leak through as live keystrokes. The app still keeps two pragmatic layers on top: `is_likely_paste` heuristically treats large printable unmarked chunks as paste for terminals without bracketed paste, and `sanitize_paste_markers`/`strip_paste_markers` still scrub stored residue defensively when copying URLs from older polluted state. Standalone `Esc` is resolved on a short tick delay so split escape sequences are not mistaken for cancel keys.

Roadmap ideas:
1. Nail one addictive loop: join -> listen -> chat -> vote -> return tomorrow.
2. Pick a clear ICP first: solo devs at night vs remote teams during work hours.
3. ~~Add one "reason to come back" mechanic~~ ✓ Daily streaks + badge tiers + leaderboard. Next: daily room rituals, timed events.
4. Keep friction near zero: ssh late.sh + optional browser pairing only when wanted.
5. Measure retention early: D1/D7 return, session length, messages/user, votes/session.

### The Arcade Pipeline [VOLATILE]

**Shipped:**
- ~~Tetris (Ascii Drop)~~ ✓ Endless falling-block arcade, 15fps gravity, persisted runs, per-user high scores.
- ~~Minesweeper~~ ✓ Classic logic puzzle with daily seeded boards and personal infinite play.
- ~~2048~~ ✓ ~~Sudoku~~ ✓ ~~Nonograms~~ ✓ ~~Solitaire~~ ✓

**Table Games (active buildout):**
- **Blackjack:** Shared-table implementation is live behind admin gate. Service owns truth; clients subscribe to snapshots/events. Still missing seats, turn rotation, table identity, and room wiring.
- **Texas Hold'em Poker (PvP):** The ultimate late-night clubhouse game. Table-scoped chat, robust turn state.

**Async 1v1:**
- **Chess:** Correspondence style — make moves at your own pace over hours/days.
- **Battleship:** Fire a shot and check back tomorrow.

**Real-time Multiplayer:**
- **Tron (Lightbikes):** 15fps grid-based survival arena.

**Card Games:**
- **Cribbage / Bridge / Thousand (Tysiąc):** Cozy trick-taking games, deep strategy.

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
- Higher complexity — build after Blackjack validates the chip system

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

### Persistent Multiplayer World (Big Bet) [VOLATILE]

An always-running game where every connected SSH session is automatically a participant. The world ticks forward whether you're watching or not — drop in, make moves, drop out, come back tomorrow.

**Direction:** 4X / trading / economy game. Think simplified space traders or terminal-scale Civilization — explore, expand, exploit, trade. Every connected user is a player in the same persistent world.

**Why it fits late.sh:**
- Always-on matches the clubhouse vibe — the world is alive when you SSH in
- Scales naturally with player count (more players = richer economy/politics)
- Gives a strong "check back tomorrow" retention loop
- Integrates with Late Chips economy
- Chat becomes strategic (alliances, trade negotiation, trash talk)

**Open design questions:**
- Turn-based (ticks every N minutes) vs real-time with rate-limited actions?
- How much can happen while you're offline? (auto-trade, passive income, vulnerability to raids?)
- Map topology: shared grid, star map, abstract network?
- Win conditions or endless sandbox?

### Bonsai Tree Enhancements
- Seasonal color shifts (real-world date), profile display for visitors, graveyard rendering on profile.
- Fancier renderer — possibly port/adapt `cbonsai` (https://github.com/mhzawadi/homebrew-cbonsai) for richer growth animation and branching.

### GitHub Notifications Widget
- Read-only dashboard widget showing PR reviews, mentions, issue updates via PAT.
- Gives solo devs a productivity reason to keep the terminal open.

### Other Ideas
- Daily/weekly rituals (lo-fi standup, shipped rollup, weekend recap)
- Ambient presence (quiet hours, listening since, typing indicator)
- Micro-collab tools (shared scratchpad, snippet paste, pairing ping)
- Cozy utilities (pomodoro, focus playlists, now-playing shoutouts)
- Community texture (rotating shoutout board, wall of thanks)
- Events (coffee breaks, AMAs, mini coding jams)
- Personalization (accent color, favorite vibe, custom tagline)

### Multi-replica readiness (future)

Currently the SSH app assumes a single process. These in-memory structures would need to be externalized (Redis / Postgres) for multiple replicas:

| Structure | Location | Current | To externalize |
|-----------|----------|---------|----------------|
| `current_genre` / `round_id` | `VoteService::ServiceState` | In-memory, resets to Lofi on restart | Persist to DB; only one replica runs the switch timer (leader election or DB lock). During pod drain today, the old pod cancels the vote loop immediately so only the new pod keeps mutating rounds/Liquidsoap. |
| `active_users` / `conn_counts` | `State` | In-memory counters | Shared store (Redis or DB) |
| `SessionRegistry` | `session.rs` | In-memory `token → mpsc` | Stays local — sticky sessions route SSH + WS to same replica |
| Vote/Chat/Article events + snapshots, Profile per-user snapshots | `broadcast` / `watch` channels | In-process only | Postgres `LISTEN/NOTIFY` or Redis pub/sub for cross-replica fan-out |
| @bot + @graybeard chat | `GhostService` | Always-on presence + AI chat tasks; both are dedicated DB users with fixed fingerprints | Single-leader to avoid duplicate chat responses. During pod drain today, the old pod cancels bot tasks immediately. |
| Leaderboard data | `LeaderboardService` | DB-backed `watch` channel, 30s refresh | Already DB-backed; each replica runs its own refresh loop — duplicate work but no write conflict |

**Approach:** Sticky sessions (LB routes by source IP) so each SSH connection lives on one replica. Shared data via DB/Redis. Not needed yet — single replica handles thousands of concurrent SSH sessions.

---

## 8. Critical Invariants and Tricky Flows [STABLE]

### 8.1 Security/scoping invariants

- All user-data queries MUST filter by `user_id` - enforced by `user_scoped_model!` macro and explicit `_by_user` method variants
- `model!` macro hardcodes `id: Uuid`, `created: DateTime<Utc>`, `updated: DateTime<Utc>` — do NOT duplicate these in `@generated`; use `@generated` only for extra fields (e.g., `last_seen` on User)
- Chat room visibility enforced via `ChatRoom::list_for_user` (membership join) - never expose rooms user hasn't joined
- `#announcements` is read-joinable like other permanent public rooms, but only admins may post there; enforce this in the chat service send path, not only in the UI
- DM rooms canonicalize user IDs (`dm_user_a < dm_user_b` text order) to prevent duplicate DM pairs
- Profile usernames are the canonical public handles for chat/DM lookup; SSH login ensures a profile row exists immediately, seeds it from the SSH username, and adds `-N` suffixes in Rust when needed to stay unique
- @bot and @graybeard bootstrap on app startup: ensure DB user + profile, sync `profiles.username`, join public rooms, and insert into `active_users` (always online). Both are dedicated users with fixed fingerprints (`bot-fp-000`, `graybeard-fp-000`)
- Connection limits (global semaphore + per-IP counter) plus SSH attempt rate limit (sliding window) MUST be enforced before any auth (effective client IP is resolved from PROXY protocol when enabled)
- Chat message deletes are hard deletes; any moderation/delete path must remove rows directly rather than relying on tombstones

### 8.2 Data integrity invariants

- UUID v7 PKs (`uuidv7()` default) for time-ordered IDs across all tables
- All foreign keys use `ON DELETE CASCADE` - deleting a user cascades to all their data
- Vote table has `UNIQUE(user_id)` - one vote per user, upsert on conflict
- Chat room constraints: general must have `slug='general'`, language must have `language_code`, DM must have both user IDs with correct ordering
- `auto_join` can only be `true` for public rooms

### 8.3 High-risk end-to-end flows

**Paired client control + visualizer:**
1. Trigger: SSH PTY request creates a session token plus the inbound `SessionRegistry` route.
2. Processing: Browser or CLI connects `GET /api/ws/pair?token=...`; API registers an outbound paired-client sender/state slot in `PairedClientRegistry`.
3. Side effects: Paired client sends viz frames (66ms-ish) plus `client_state`; viz frames route through `SessionRegistry` to `App.tick()`, while `client_state` updates paired kind/mute/volume metadata in `PairedClientRegistry`.
4. Side effects: TUI `m`, `+`, and `-` send `toggle_mute`, `volume_up`, and `volume_down` back over the same WS to only the paired client for that token.
5. Failure: If the paired client disconnects, visualizer decays (rms * 0.96 per tick) and paired state disappears. If SSH disconnects, the session token unregisters on drop.

**Chat send flow:**
1. Trigger: User presses Enter in composer with non-empty text (supports multiline via Alt+Enter)
2. Processing: `ChatService::send_message_task` spawned with `request_id` → DB insert → `MessageCreated` broadcast → `SendSucceeded` targeted to sender
3. Side effects: All sessions receive `MessageCreated`, sender gets banner feedback. Messages containing `@username` show a golden `│` gutter on the left for the mentioned user.
4. Failure: Non-member send → `SendFailed` event with message. DB error → `SendFailed`.

**Chat ignore flow:**
1. Trigger: User submits `/ignore @user` or `/unignore @user` in the composer
2. Processing: `ChatService::ignore_user_task` / `unignore_user_task` resolves the username via `User::find_by_username`, then calls `User::add_ignored_user_id` / `remove_ignored_user_id` (settings JSONB write keyed on `target.id`, not the username string)
3. Side effects: Service emits `ChatEvent::IgnoreListUpdated { user_id, ignored_user_ids, message }`. `ChatState` updates its local `HashSet<Uuid>`, calls `refilter_local_messages()` to drop already-stored messages from any newly-ignored author from `general_messages` and every non-DM room in place (no DB refetch), and shows a success banner.
4. Exemptions: DM rooms are never filtered — leaving the DM room is the way to dismiss it. `push_message` skips the ignore check for DM rooms so 1:1 messages always land.
5. Display path: `ChatState::ignore_list_lines()` resolves stored UUIDs back to `@username` via the snapshot's full `usernames` map (`User::list_all_username_map`), falling back to `@<unknown:…>` when a user isn't in the cache.
6. Failure: `IgnoreFailed { user_id, message }` for self-target, unknown username, already-ignored, or not-currently-ignored — surfaced as a red banner.

**Chat reply flow:**
1. Trigger: User selects a message (`j`/`k`) and presses `r`
2. Processing: `ChatState::begin_reply_to_selected()` captures the target author plus a short preview, enters compose mode, and shows a reply-specific composer title
3. Side effects: Submit prepends a quoted first line (`> @user: preview`) to the stored message body; rendering peels that first line back out and draws it as faint reply context above the new body
4. Failure: No selected message → `r` is a no-op

**Chat @mention autocomplete:**
1. Trigger: User types `@` in composer (at start or after space)
2. Processing: `ChatState::update_autocomplete()` filters `all_usernames` (loaded from profiles DB via `ChatSnapshot`) case-insensitively by the query after `@`
3. Interaction: Arrow keys navigate matches, Tab/Enter confirms (inserts `@username `), Esc dismisses popup without leaving compose mode
4. Rendering: `draw_mention_autocomplete()` renders a popup above the composer with up to 8 filtered matches; confirm must also move `composer_cursor` to the end of the inserted mention including the trailing space

**Vote round switch:**
1. Trigger: VoteService background tick (5s) detects switch interval (default 60 min) elapsed since last switch
2. Processing: `switch_to_winner()` → pick genre with most votes (or keep current) → clear all votes → increment `round_id` → send `vibe.set <genre>` to Liquidsoap
3. Side effects: All clients detect `round_id` change → clear `my_vote`. Liquidsoap switches playlist.
4. Failure: Liquidsoap TCP failure logged but round still switches locally.

### 8.4 Easy-to-break gotchas

- **Ignore is keyed by user id, not username:** `users.settings.ignored_user_ids` stores UUIDs, so a `/ignore @alice` survives @alice renaming herself to @alice2. Storing usernames there would silently break on rename and could re-attach a stale ignore to a different person if usernames are ever reused.
- **Ignore re-filter is local-only:** `ChatEvent::IgnoreListUpdated` triggers an in-place retain over `general_messages` and every non-DM room — no `request_list()` refetch. Side effect: `unignore` does **not** retroactively re-fetch already-dropped messages; they reappear on the next natural snapshot/refresh.
- **Snapshot merge for empty rooms:** `ChatState::merge_rooms` preserves cached messages when snapshot arrives with empty message list for a room - prevents flash-clear on out-of-order snapshots
- **Unread count merge:** `merge_unread_counts` tracks `pending_read_rooms` to suppress stale unread counts after marking read (avoids flicker)
- **Render loop missed ticks:** 66ms interval with `MissedTickBehavior::Skip` - if a frame takes too long, next ticks are skipped rather than queued (prevents snowball lag)
- **SSH data timeout:** `handle.data` has 50ms timeout to avoid blocking render loop on backpressure
- **SSH send failure is terminal for render task:** if `handle.data` returns `Err` (closed/broken channel), `render_once` now returns an error so the render loop stops and closes channel once, instead of logging warnings every 66ms forever
- **Message ordering:** Full history is `ORDER BY created DESC, id DESC` (newest first), delta sync is `(created, id) > cursor ASC` - mixing these up breaks chat display. Chat rendering reverses messages to oldest-first for row-based display, with newest at the bottom.
- **Chat message navigation is selection-first:** `selected_message_id` is the source of truth for dashboard/general chat and the full chat screen. Mouse wheel, arrows, paging, and `j/k` all move selection; when no message is selected, the viewport falls back to newest-at-bottom.
- **Chat wrapping is word-aware:** Shared wrapping prefers breaking on whitespace for regular messages, reply quote lines, news-card text, and the composer. Hard splits are only valid for single words longer than the available width.
- **Chat room list order is UI-defined:** The chat sidebar order is hardcoded as `core` (`general`, `announcements`, `suggestions`, any other permanent rooms, then synthetic `news`) → `public` → `private` → `dm`, with divider rows rendered in the UI. The synthetic `news` row now carries its own unread badge sourced from `article_feed_reads`, not `chat_room_members`.
- **Transcript render cost is cache-sensitive:** `late-ssh` fetches up to 1000 messages for the selected room and up to 1000 for general. The chat UI therefore caches wrapped transcript rows for the dashboard general card and the active room; invalidation must track width, message content/order, usernames, badges, and bonsai glyphs.
- **Composer render cost is cache-sensitive:** The chat composer caches wrapped `ComposerRow`s in `ChatState`; any change to composer text or width must invalidate that cache before render/cursor-up/down.
- **@mention detection:** Uses simple `@username` substring match in message body. The `all_usernames` field on `ChatSnapshot` is loaded from profiles DB every refresh (10s) via `User::list_all_usernames` — includes all users, not just online ones.
- **Reply persistence is currently body-encoded:** There is no reply DB column yet. Replies are stored as a quoted first line in `body` and rendered back out in the TUI. If/when true threaded metadata is added, both send and render paths need coordinated migration.
- **All services are singletons** shared across SSH sessions. `ProfileService` snapshots are per-user channels keyed by `user_id`; events still require `user_id` filtering in UI state. Per-user background refresh tasks are spawned on session init and aborted on `Drop`, and profile snapshot channels are pruned when receivers go away.
- **Chat room `updated` timestamp:** Intentionally NOT used for room ordering to keep stable sort - rooms ordered by type (general → language → private → dm)
- **Web Audio `createMediaElementSource` is one-shot:** Can only be called once per `<audio>` element. AudioContext + source node must be created once and reused across play/pause cycles. Disconnect suspends the context (`audioCtx.suspend()`), replay resumes it — never close and recreate.
- **Browser audio pairing status must not be stomped by WS:** WS `onclose`/`onerror` must check `status !== 'playing'` before setting `'disconnected'`, otherwise a WS drop kills the "streaming" UI while audio is still playing fine
- **Paired-client control routing is latest-wins per token:** `PairedClientRegistry` stores one outbound sender/state entry per session token. If multiple browser/CLI clients pair against the same token, the most recent registration owns control/state until it disconnects.
- **CLI visualizer follows audible output now:** The CLI analyzer ring must record post-mute/post-volume samples, not raw decoded samples, otherwise the TUI visualizer drifts from what the user actually hears.
- **CLI output sample-rate fallback is device-driven:** Prefer native `44.1 kHz` playback when the output backend supports it; only resample when the chosen `cpal` device config requires a different rate such as `48 kHz`.
- **Web/CLI Audio and WS Resiliency:** Both paired clients use a 10-attempt retry loop (2s delay) for WebSocket disconnections and audio stream failures. Web Audio reconstructs elements with cache-busting `?t=` URLs, and CLI re-probes `SymphoniaStreamDecoder` in place, so both recover seamlessly from `ERR_NETWORK_CHANGED` without dropping the session.
- **Browser and CLI viz payloads share schema, not implementation:** Both paired clients send `{ event: "viz", position_ms, bands, rms }`, but the browser uses Web Audio `AnalyserNode` while the CLI uses an in-process Rust FFT over playback samples. Expect similar behavior, not identical numbers.
- **CLI binary must forward local terminal resizes:** The `late` binary runs SSH inside a local PTY and must propagate `SIGWINCH` size changes into that PTY so side-by-side panes and split windows reflow correctly after startup.
- **CLI must suppress pre-banner input:** The local `late` binary must not forward stdin into the SSH PTY until the server's `LATE_SESSION_TOKEN=` banner has been captured. Any keys typed during handshake/welcome race windows are intentionally discarded, and pending terminal input is flushed immediately before forwarding starts.
- **Activity feed broadcast timing:** `broadcast::Receiver` only sees messages sent AFTER subscription. The receiver must be created in `auth_publickey` (before login event is sent), stored on `ClientHandler`, then `.take()`'d into `SessionConfig` in `pty_request`. Creating the receiver later misses the user's own login event.
- **Leaderboard refresh is async, badges are eventually consistent:** `LeaderboardService` refreshes every 30s. A new daily win won't appear in the leaderboard or chat badges until the next refresh cycle. Activity feed callouts are immediate (fire-and-forget from `record_win_task`).
- **Streak SQL uses gaps-and-islands:** A streak is "current" if its last day is today or yesterday. This means a user who hasn't played today still keeps their streak visible until midnight UTC tomorrow. The `UNION` across `sudoku_daily_wins` and `nonogram_daily_wins` deduplicates dates so playing both games on the same day counts as one streak day.
- **Game services hold `activity_feed` sender:** `SudokuService` and `NonogramService` both hold a clone of the `broadcast::Sender<ActivityEvent>` for win callouts. The username is looked up from `profiles` table inside the fire-and-forget task, not passed from the caller.
- **Bonsai death check runs on login:** `BonsaiService::ensure_tree()` checks `last_watered` against UTC today on every SSH session start. If 7+ days have passed, the tree is killed and a graveyard record is created. This means death is only detected when the user reconnects, not while offline.
- **Bonsai passive growth is per-session:** The tick counter in `BonsaiState` grants 1 growth point every ~9000 ticks (~10 min at 15fps). If a user has multiple sessions, each grants growth independently. This is acceptable — it rewards being connected, not gaming the system.
- **Bonsai chat glyph is current-user only:** The bonsai stage glyph (· ⚘ 🌲 🌳 🌸) is only shown next to the current user's own messages. Other users' bonsai stages are not queried or displayed in chat (would require a new cross-user lookup).
- **Bonsai cut changes seed optimistically:** The `cut()` method updates `self.seed` in memory immediately and fires a background DB task. If the DB write fails, the in-memory seed diverges from persisted until next login.
- **Help modal (`?`) intercepts all input:** When `show_help` is true, the input handler dismisses the modal on any keypress before any other input processing. This includes `?` itself (toggle off) and `Esc`.

---

## 9. Quick Reference APIs [STABLE]

```rust
// === Database ===
let db = Db::from_env().await?;
let client = db.get().await?;
db.migrate().await?;

// === User identity ===
let user = User::find_by_fingerprint(&client, &fingerprint).await?;
user.update_last_seen(&client).await?;

// === Vote ===
Vote::upsert(&client, user_id, "lofi").await?;
let (lofi, classic, ambient, focus, jazz) = Vote::tally(&client).await?;

// === Chat ===
let rooms = ChatRoom::list_for_user(&client, user_id).await?;
ChatRoomMember::auto_join_public_rooms(&client, user_id).await?;
ChatRoomMember::mark_read_now(&client, room_id, user_id).await?;
let unread = ChatRoomMember::unread_counts_for_user(&client, user_id).await?;
let messages = ChatMessage::list_recent(&client, room_id, 50).await?;

// === Services (subscribe pattern) ===
let vote_rx = vote_service.subscribe_state();   // watch::Receiver<VoteSnapshot>
let vote_ev = vote_service.subscribe_events();  // broadcast::Receiver<VoteEvent>
vote_service.cast_vote_task(user_id, Genre::Lofi);

// === Profile ===
let profile = Profile::find_or_create_by_user(&client, user_id).await?;
Profile::update_by_user_id(&client, user_id, id, ProfileParams { .. }).await?;

// === Leaderboard ===
let lb_rx = leaderboard_service.subscribe();        // watch::Receiver<Arc<LeaderboardData>>
let data = lb_rx.borrow();                          // today_champions, streak_leaders, user_streaks
let badge = BadgeTier::from_streak(streak);          // None | Bronze(3+) | Silver(7+) | Gold(14+)

// === Icecast ===
let track = late_core::icecast::fetch_track(&icecast_url)?;  // blocking

// === Liquidsoap ===
late_ssh::app::vote::liquidsoap::send_command(&addr, "vibe.set lofi").await?;
```

---

## 10. Runbook [VOLATILE]

### 10.1 Local development

```bash
# Start full dev stack
docker compose up -d

# Or run services individually:
# Postgres + Icecast + Liquidsoap via docker, Rust services via cargo
docker compose up -d postgres icecast liquidsoap
cargo run -p late-ssh   # Needs LATE_* env vars
cargo run -p late-web   # Needs LATE_WEB_* env vars
```

### 10.2 Database

```bash
# Quick connectivity check
PGPASSWORD=postgres psql -h localhost -p 5432 -U postgres -d postgres -c "select 1;"

# Seed data
sh scripts/seed_chat_rooms.sh
sh scripts/seed_chat_messages.sh
sh scripts/seed_notes.sh

# Inspect chat messages
PGPASSWORD=postgres psql -h localhost -p 5432 -U postgres -d postgres -c "
SELECT r.slug, u.username, m.body, m.created
FROM chat_messages m
JOIN chat_rooms r ON r.id = m.room_id
JOIN users u ON u.id = m.user_id
ORDER BY m.created DESC
LIMIT 20;"
```

### 10.3 Testing

```bash
# Human-only verification commands. LLM agents should not run these.
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --workspace --all-targets
```

Use narrower crate-specific `cargo test` / `cargo nextest run` commands ad hoc while iterating, but keep the workspace gate above as the canonical repo-level check.

### 10.4 Debugging checklist

1. SSH won't connect → Check `LATE_SSH_OPEN`, connection limits/rate limits, SSH key path
2. No audio → Check Icecast container, Liquidsoap container, `LATE_AUDIO_URL`. If streams are down, verify fallback music exists on the PVC (see below)
3. Visualizer not updating → Check browser WS connection, token mismatch, SessionRegistry
4. Votes not switching → Check Liquidsoap telnet reachability (`LATE_LIQUIDSOAP_ADDR`), background tick running
5. Chat not syncing → Check DB connectivity, 10s refresh cadence, snapshot/event channels
6. Now-playing shows "Unknown" → Check Icecast `/status-json.xsl`, metadata format: `"Artist - Title | Duration"` (duration is absent for internet streams — this is expected)
7. Liquidsoap debugging → `docker run --rm savonet/liquidsoap:v2.4.0 liquidsoap -h <topic>`
8. Music missing from PVC → Re-run infra deploy to trigger `sync_music` job (syncs from R2). For manual recovery: `aws s3 sync s3://$MUSIC_BUCKET/ ./music/ --endpoint-url $S3_ENDPOINT` then `kubectl cp` each genre dir individually into the pod.

## 11. TUI Screens Reference [STABLE]

### Screen overview

| Screen | Key | Status | Description |
|--------|-----|--------|-------------|
| **Dashboard** | 1 | Active | Stream URL + now playing + voting + dashboard chat (The Lounge Hub) |
| **Chat** | 2 | Active | Full room-list chat screen (`/dm @user`, `/join #room`, `/create #room`, `/leave`, `/ignore [@user]`, `/unignore [@user]`, `/help`) with grouped room sections and a synthetic `news` entry in the room list |
| **Games** | 3 | Active | The Arcade Lobby + leaderboard sidebar (champions, streaks, all-time high scores, chip leaders, info): persisted high-score games (`2048`, `Tetris`), daily games (`Sudoku`, `Nonograms`, `Minesweeper`, `Solitaire`), and admin-gated shared-table Blackjack. Game list auto-scrolls (top-third anchor); ASCII header hides on small screens |
| **Profile** | 4 | Active | User profile: username, Your Stats (streak + badge, chips, high scores), @bot/@graybeard info, chat colors |

### Layout

```
┌─ late.sh ──────────────────────────────────────────────────────────┐
│                                            │ ┌─ Visualizer ──────┐ │
│            Main Content Area               │ │ █ █ █ █ █ █ █ █ │ │
│         (screen-dependent)                 │ └───────────────────┘ │
│                                            │ ┌─ Now Playing ─────┐ │
│                                            │ │ Artist - Title    │ │
│                                            │ │ 0:57 ──●──── 3:15 │ │
│                                            │ └───────────────────┘ │
│                                            │ ┌─ Activity ────────┐ │
│                                            │ │ ● 12 online ? keys│ │
│                                            │ │ @user  2m         │ │
│ ┌──────────────────────────────────────┐   │ │ joined chat       │ │
│ │ ✓ Voted for Lofi                     │   │ └───────────────────┘ │
│ └──────────────────────────────────────┘   │ ┌─ Bonsai (42d) ───┐ │
│                                            │ │    .@@@.          │ │
│                                            │ │  .@@@@@@@.        │ │
│                                            │ │   /   \           │ │
│                                            │ │   |   |           │ │
│                                            │ │   .|.             │ │
│                                            │ │  [===]            │ │
│                                            │ │   Sapling         │ │
│                                            │ │   w water         │ │
│                                            │ └───────────────────┘ │
└────────────────────────────────────────────────────────────────────┘
```

Toast notification is hidden by default (0 rows). When active, it appears as a 3-row bordered block (green for success, red for error) at the **top-right** of the content area. Welcome overlay renders on top of the toast.

### Keyboard shortcuts

| Key | Context | Action |
|-----|---------|--------|
| `q` / `Q` / `Ctrl+C` | Global | Quit |
| `?` | Global (not composing) | Open keybindings help modal (scrollable with j/k/arrows/mouse) |
| `Tab` | Global | Cycle screens |
| `1` | Global | Jump to Dashboard |
| `2` | Global | Jump to Chat |
| `3` | Global | Jump to Games |
| `4` | Global | Jump to Profile |
| `m` | Global | Toggle mute on paired client |
| `+` / `=` | Global | Volume up on paired client |
| `-` / `_` | Global | Volume down on paired client |
| `w` | Global (not composing, games override) | Water bonsai / replant dead tree |
| `x` | Global (not composing, games override) | Prune bonsai (reshape, costs 20% growth) |
| `L` / `C` / `A` / `Z` | Dashboard | Vote genre |
| `s` | Dashboard | Copy bonsai ASCII snippet to clipboard |
| `j` / `k` / arrows | Dashboard | Scroll chat |
| `r` | Dashboard chat selection | Reply to selected dashboard/general chat message |
| `Enter` | Games lobby | Launch selected game |
| `Esc` | Active game | Exit back to Arcade lobby |
| `h` / `j` / `k` / `l` / arrows | 2048 | Move tiles |
| `r` | 2048 game over | Start a fresh 2048 board |
| `h` / `l` / arrows | Tetris | Move active piece left / right |
| `j` / down arrow | Tetris | Soft drop |
| `k` / up arrow | Tetris | Rotate clockwise |
| `Space` | Tetris | Hard drop |
| `p` | Tetris | Pause / resume |
| `r` | Tetris | Start a fresh run |
| `r` | Sudoku (unsolved) | Reset board (clears non-fixed cells) |
| `r` | Nonograms (unsolved) | Reset board (clears all cells) |
| `h` / `j` / `k` / `l` / arrows | Sudoku | Move cursor |
| `1`-`9` | Sudoku | Fill selected cell |
| `0` / `Backspace` | Sudoku | Clear selected cell |
| `d` | Sudoku | Restore today's daily board |
| `p` | Sudoku | Open saved personal board |
| `n` | Sudoku | Generate a fresh personal board |
| `[` / `]` | Sudoku | Switch difficulty (easy / medium / hard) |
| `h` / `j` / `k` / `l` / arrows | Nonograms | Move cursor |
| `Space` / `x` | Nonograms | Toggle selected cell |
| `0` / `Backspace` / `c` | Nonograms | Clear selected cell |
| `d` | Nonograms | Restore today's daily puzzle for the current size |
| `p` | Nonograms | Open saved personal puzzle for the current size |
| `n` | Nonograms | Generate a fresh personal puzzle for the current size |
| `[` / `]` | Nonograms | Switch puzzle size pack |
| `Esc` | Nonograms | Exit back to Arcade lobby |
| `h` / `l` | Chat | Switch room selection, including the synthetic `news` entry. Aliases: `Ctrl+N` next room, `Ctrl+P` previous room; room switching wraps around. |
| `j` / `k` / arrows | Chat (`news` selected) | Navigate news list |
| `i` | Chat (`news` selected) | Start composing/pasting URL |
| `Enter` | Chat (`news` selected) | Copy selected link / submit URL |
| `d` | Chat (`news` selected) | Delete own article |
| `Esc` | Chat (`news` composing) | Cancel URL compose |
| `i` / `Enter` | Dashboard | Start composing chat |
| `j` / `k` | Chat | Move message selection newer/older |
| `r` | Chat | Reply to selected message |
| `/help` | Chat composer | Open scrollable chat help overlay (commands + all chat keys) |
| `/ignore [@user]` | Chat composer | Mute a user, or list muted users when no arg |
| `/unignore [@user]` | Chat composer | Remove a user from your ignore list |
| `j` / `k` / arrows | Chat overlay (`/help`, ignore list) | Scroll overlay |
| `q` / `Esc` | Chat overlay (`/help`, ignore list) | Close overlay |
| `j` / `k` / arrows | Profile | Navigate fields |
| `Space` / `Enter` | Profile | Toggle checkbox / edit username |
| `e` | Profile | Edit username |
| `Esc` | Any modal | Close/cancel |
| `c` | Chat (not composing) | Open web chat QR link |

### Keybinding change checklist

When modifying any keybinding, update **all** of the following:

1. **Input handler** — the actual `match byte` in the relevant `input.rs` (screen-specific or `app/input.rs` for globals)
2. **Help modal** — `app/render.rs` `draw_help_overlay()`, two-column keybinding list
3. **Welcome modal** — `app/render.rs` `draw_welcome_overlay()`, if the key is mentioned in the getting-started section
4. **Sidebar hints** — `app/common/sidebar.rs`, e.g. the volume/mute hint line in Now Playing
5. **Game guard** — `app/input.rs` `handle_global_key()` line ~540, the `!matches!(byte, ...)` allowlist for keys that pass through during active games
6. **This table** — the keyboard shortcuts table above in CONTEXT.md
7. **Game info panels** — per-game UI panels that show controls (check each game's `ui.rs`)

---

---

## References

- russh: https://github.com/Eugeny/russh
- ratatui: https://ratatui.rs/
- Icecast: https://icecast.org/
- Alpine.js: https://alpinejs.dev/
- HTMX: https://htmx.org/
- Liquidsoap: https://www.liquidsoap.info/
