# PLAN: Roguelike Door Boards, Badges, and the Log Pipe

Status: Phase 1 (DCSS end-to-end) IMPLEMENTED 2026-08-07; Phase 2 (NetHack +
scrape removal) IMPLEMENTED 2026-08-08, uncommitted on branch
`mateu/roguelikes_leaderboards`; Phases 3-4 not started. This plan covers the
three external roguelike doors: NetHack, DCSS, Brogue. (The Lateania Games
boards shipped separately.)

Read first: root `CONTEXT.md`, `late-ssh/src/app/leaderboard/CONTEXT.md`
(created during Phase 1: the whole leaderboard domain, incl. the door board
model), and the door contexts for the phase at hand
(`late-ssh/src/app/door/{nethack,dcss,brogue}/CONTEXT.md`; the dcss and
nethack ones document their shipped log pipes, brogue names its run-history
file in §9/Deferred).

## Phase 2 handoff (NetHack, 2026-08-08)

Green under targeted `make test-llm` runs (late-nethack, ingest, leaderboard,
nethack door suites); `make check` not run (human-owned). Everything mirrors
the Phase 1 patterns; the notable findings and deltas:

- **LIVELOG panned out.** 5.0.0 compiles it in via the `NHL_SANDBOX` +
  `CHRONICLE` chain (config.h force-defines it for `--loglua`), but
  `sysopt.livelog` defaults to `LL_NONE`, so the Dockerfile appends
  `LIVELOG=0x0002` (LL_ACHIEVE) to sysconf. Live Amulet-at-pickup badges and
  achievement feed flavor come from the `livelog` file; XLOGFILE is defined
  unconditionally. Both plus `CHRONICLE`/`NHL_SANDBOX` are grep-asserted
  fail-closed; `door-nethack` image tag bumped to `5.0.0-r2`.
- **Explore mode is locked off** (sysconf `EXPLORERS=` blanked, grep-asserted):
  shipped `EXPLORERS=*` let any player enter non-scoring explore mode, and
  livelog lines carry no mode flag, so an explore Amulet pickup would have
  spoofed the 10k grant. xlogfile lines DO flag wizard/explore games
  (`flags` 0x1/0x2) and the parser+service skip them wholesale.
- **xlog dialect correction to this plan:** NetHack's xlogfile is
  TAB-separated (`XLOG_SEP '\t'`), not space-separated; `endtime` is unix
  epoch; the Amulet is `achieve` bit 0x20 (ACH_AMUL=6, bit value-1);
  `death=` is exactly `ascended`/`escaped`/`quit` for the non-death ends.
  `maxlvl` is already the run maximum, so the dive board needs no milestone
  union for NetHack (livelog has no depth field at all).
- **Implementation map:** host `late-nethack/src/stats.rs` + `server.rs`
  SessionHost split + `config.var_dir` (`LATE_NETHACK_VAR_DIR`, the
  VAR_PLAYGROUND the logs live in — NOT data_dir/HOME); client
  `app/door/ingest/nethack.rs` (+`nethack_test.rs`), `DoorKind` in
  `ingest/svc.rs` sharing the session plumbing across doors,
  `DoorBadge::{NethackAmulet, NethackAscension}` on the existing
  `nethack_amulet`/`nethack_ascension` reward templates,
  `DoorGame::Nethack` in the roster (page arms exhaustive-matched),
  `DoorMilestoneKind::Amulet`, main.rs task behind `LATE_NETHACK_ENABLED`,
  seed script NetHack rows, infra initContainer now touches `livelog`.
- **The scrape is gone:** `door/nethack/{milestone,status,award}.rs` + tests
  deleted, `scan_screen` + debounce fields removed from its `state.rs`; the
  connect-based "started a NetHack game" event moved to a plain
  `ActivityPublisher` (`SessionConfig.nethack_activity`, ex `nethack_awards`).
  Descent feed lines are gone by owner decision (#1 below).
- **Prod bring-up order matters, same as Phase 1:** deploy the new
  `late-nethack` host (a `-nethack` release, which also rebuilds the r2 game
  image) before or with the service-ssh release. The initContainer change
  (touch `livelog`) is a manifest change: it ships through `deploy_infra.yml`,
  not the image-only `deploy_nethack.yml`. First connect backfills the whole
  xlogfile history (boards launch non-empty; historical ascensions back-grant
  badges by owner decision). Legacy `late_<hex>` lines in the backfill are
  skipped by the reserved-name rule.

## Phase 1 handoff (what exists now, for a fresh session)

Everything below is implemented, formatted, and green under targeted
`make test-llm` runs (ingest, leaderboard, late-dcss, late-core door/user
suites). NOT run: `make check` (the human-owned full gate).

Implementation map:

- **Host stats session**: `late-dcss/src/stats.rs` (+ `server.rs` branch).
  Reserved username `late_stats` streams `<file-id>\t<next-offset>\t<line>`
  frames from `$HOME/.crawl/{logfile,milestones}` with tail -f semantics;
  client pushes cursors via one `LATE_DOOR_STATS_CURSORS` env request
  (`logfile:123,milestones:456`); host stays stateless. Constants mirrored
  client-side in `late-ssh/src/app/door/ingest/stream.rs`.
- **Build**: `docker/doors/dcss.Dockerfile` now passes
  `EXTERNAL_DEFINES="-DDGL_MILESTONES -DTIME_FN=gmtime
  -DDGL_EXTENDED_LOGFILES"`, each asserted fail-closed via the `-version`
  CFLAGS grep; door-dcss image tag bumped to `0.34.1-r2` (root Dockerfile +
  `dcss.yml`). Correction to this plan's assumption: crawl writes the
  milestones file ONLY under `DGL_MILESTONES` — "DCSS: nothing to build" was
  wrong. Field names/timestamps verified against the pinned 0.34.1 tarball
  (xlog months are 0-based; `absdepth` is the END-of-run depth, which is why
  the dive board unions milestone depth marks — a winner ends at depth 1).
- **Data**: migration `136_create_door_ingestion.sql` (`door_runs`,
  `door_milestones`, `door_log_cursors`, dcss_orb/dcss_win reward templates);
  models `late-core/src/models/{door_run,door_milestone,door_log_cursor}.rs`
  with closed `DoorRunResult`/`DoorMilestoneKind` enums. Idempotency: unique
  `(game, source_file, source_offset)`; fact insert + cursor advance commit
  in one transaction.
- **Ingestion slice**: `late-ssh/src/app/door/ingest/` — `dcss.rs` pure
  parsers, `stream.rs` stats SSH client, `svc.rs` orchestration (spawned in
  main.rs behind `LATE_DCSS_ENABLED`, connect-with-retry every 30s),
  `award.rs` the shared `DoorAwards`/`DoorBadge` sink Phases 2-3 extend.
  Reserved `late`/`late_*` names and dead handles skipped
  (`ArcadeHandle::find_user_by_handle`). Feed events gated on insert
  freshness AND a 10-minute recency window (backfill never floods #lounge);
  awards fire on every win/orb sighting (lifetime-idempotent, heals a crash
  between insert and grant); win back-grants the Orb badge.
- **Badges**: `DCO`/`DCW` categories in `profile_award.rs`, chat-label
  collapse in `user.rs` (DCO collapses into DCW), legend in
  `profile_modal/badges.rs`, `ChipMove::DcssOrbFound/DcssOrbEscape`.
- **Boards**: `DoorGame` roster + `fetch_door_boards` (one query per window,
  three families ranked `PARTITION BY (family, game)`; pass is 13 queries)
  in `late-core/src/models/leaderboard.rs`; page variants
  `DoorWins/DoorDepth/DoorScore` + `Standings::AllTimeOnly` in
  `app/leaderboard/state.rs`. Seed script grows DCSS rows
  (`seed:`-prefixed source files avoid key collisions with real lines).
- **Refactor rider**: `LeaderboardService` moved `app/hub/svc.rs` →
  `app/leaderboard/svc.rs`; leaderboard domain has its own
  `app/leaderboard/CONTEXT.md`; hub/root contexts trimmed to pointers.

**Prod bring-up order matters:** deploy the new `late-dcss` host (a `-dcss`
release, which also rebuilds the r2 game binary) before or with the
service-ssh release, or ingestion just retries against a host without the
stats session. First connect backfills the whole PVC history (boards launch
non-empty; historical wins grant badges/chips by owner decision).

## Decisions already made (owner-approved, do not re-litigate)

1. **Drop the NetHack vt100 scrape entirely.** Badges, chip payouts, and death
   feed events all move to the xlogfile. The Amulet badge grants at end of run
   instead of at pickup, and the mid-run "descended to level N" #lounge lines
   go away, unless the pinned NetHack 5.0.0 source has the LIVELOG compile
   option (check during Phase 2; if present, enable it and live events come
   back from a file). *Resolved in Phase 2: LIVELOG is compiled in and now
   enabled, so the Amulet grants at pickup; the descent lines are gone.*
2. **Badge pairs, 10k/20k chips, once per lifetime, per game**, mirroring the
   existing NetHack pair (NHA 10k, NHY 20k):
   - DCSS: Orb of Zot pickup 10k, escape with the Orb (win) 20k. Orb pickup
     was chosen over first rune deliberately: it is the exact twin of the
     Amulet badge.
   - Brogue: Escaped 10k, Mastered 20k. Brogue's run history only records
     end-of-run results (Killed / Escaped / Mastered), so the pair falls out
     of the data source; there is no mid-game milestone log.
3. **Boards per door, uniform:** Wins (all-time count), Deepest dive (monthly
   + all-time), Top score (monthly + all-time). They join the existing "Games"
   section on the Leaderboards page, which leads the board rail.
4. **Never build boards or badges on the vt100 scrape.** The screen scrape was
   acceptable for cosmetic flair; boards and chips need the spoof-proof
   host-side files.
5. **Transport: an SSH stats session on the existing door hosts** (option 1
   from the design discussion), not a new HTTP ingest surface. Reuses the
   existing russh servers and shared secrets; door pods stay free of DB
   credentials.
6. **Publish the DCSS server files over HTTP for dcss-stats.com** as part of
   this work (Phase 4). Mat is contacting the maintainer.

## Source files per game (all on each host's PVC, all append-only)

| Game | File | Written | Contains |
|---|---|---|---|
| NetHack | xlogfile in `VAR_PLAYGROUND` (`/var/games/nethack-var`) | at game end | `key=value` fields: `name`, `death`, `points`, `deathlev`, `maxlvl`, `turns`, `achieve` bitmask (includes "obtained the Amulet"), ascension as the death/end reason |
| DCSS | `$HOME/.crawl/logfile` | at game end | colon-delimited `key=value`: `name`, `sc` (score), `xl`, `place`, `ktyp` (`winning` = escaped with the Orb), `urune`, `turn` |
| DCSS | `$HOME/.crawl/milestones` | live, mid-run | same format, `type=` field: rune pickups, `br.enter`, Zot entry, `orb` (Orb pickup) |
| Brogue | run history file in each player dir (`players/<handle>/`) | at game end | seed, date, result (Killed / Escaped / Mastered), killer, score, gold, lumenstones, deepest level, turns |

Verification steps before trusting them:

- **NetHack:** VERIFIED in Phase 2 against the pinned tarball: XLOGFILE is
  defined unconditionally; LIVELOG is compiled in (via `NHL_SANDBOX` +
  `CHRONICLE`) but silent until a sysconf `LIVELOG=` mask is set. The
  Dockerfile asserts the defines fail-closed and sets the mask (details in
  the Phase 2 handoff).
- **DCSS:** nothing to build; the files exist today. Confirm exact field names
  against the pinned 0.34.1 source in the `dcss-build` stage or a local run.
- **Brogue:** upstream 1.15.1 has the victory-logging condition inverted
  (`mode != EASY && mode != NORMAL` on the victory path, so only wizard-mode
  victories land in the run history; documented in the brogue CONTEXT §9).
  Extend our patch set (we already carry `scripts/brogue_hangup_save.patch`)
  with a one-line fix restoring normal-mode victory logging, applied with a
  fail-closed grep in the `brogue-build` stage. Upstream already skips
  easy/wizard deaths in the file, which conveniently keeps cheat modes off the
  boards. AGPL note: the patch lives in this repo like the hangup patch, which
  is our section 13 source offer. Read the exact run-history write path in
  `upstream-brogue/` (re-pin if stale) before writing the parser.

## Identity mapping

The playname in every log line is the account's **arcade handle** (NetHack
`-u`, DCSS `-name`, Brogue's player directory name). Map handle to account via
`arcade_handles` (unique on `lower(handle)`; rows outlive accounts with
`user_id` set NULL, skip those). Legacy `late_<hex>` NetHack lines predate
handles: skip anything matching the reserved `late`/`late_*` shapes.

## Transport: the stats session

Each door host (`late-nethack`, `late-dcss`, `late-brogue`) is already a russh
server authenticated by a derived key from `LATE_<DOOR>_SECRET`. Add one
reserved SSH username, `late_stats` (inside the already-reserved `late_*`
handle namespace, so no player can claim it), which instead of spawning a game
child opens a log-streaming session:

- The client (late-ssh) sends its cursor per file (byte offset) at session
  open; env-request or a first-line handshake, either is fine, keep it dumb.
- The host streams file content from that offset and keeps following the file
  as it grows (tail -f semantics). Frame as `<file-id>\t<offset>\t<line>` so
  the client can persist cursors per line.
- The host stays stateless: no cursor storage, no parsing, no DB. All parsing
  lives in late-ssh, so parser fixes never need a door redeploy.
- late-ssh runs one connect-with-retry task per enabled door (main.rs,
  orchestration layer owns the loop, a plain module owns parsing). Cursors
  persist in a small `door_log_cursors` table (game, file, offset).
- Files are append-only and hosts are single-replica, so offsets are stable.
  Backfill is free: a fresh cursor starts at 0 and ingests the history already
  on the PVC, so boards launch non-empty.
- Idempotency: unique `(game, source_file, source_offset)` on the fact tables;
  re-ingest after a cursor reset is a no-op.

## Data model

- `door_runs`: one row per finished game. `game` (text key: `nethack`, `dcss`,
  `brogue`), `user_id`, `ended_at`, `result` (text from a closed Rust enum:
  `death`, `quit`, `win`, `mastery`, ...), `score`, `depth`, `turns`, `raw`
  jsonb (the parsed line, for boards we have not invented yet),
  `source_file`, `source_offset`.
- `door_milestones`: DCSS's live milestone stream (and NetHack's livelog if it
  pans out). `game`, `user_id`, `kind` (text from a closed enum: `orb`,
  `rune`, `zot_enter`, ...), `occurred_at`, `raw`, `source_file`,
  `source_offset`. Orb-pickup badges grant from here so they land at pickup.
- Both tables get their model module in `late-core/src/models/` (one place per
  table), UUID v7 ids, migrations forward-only.
- Roster: add `DoorGame: Nethack, Dcss, Brogue` via the existing `roster!`
  macro in `late-core/src/models/leaderboard.rs`. Three board families over
  `door_runs`, one union query per window ranked `PARTITION BY game`, exactly
  like `fetch_score_boards`:
  - Wins: `COUNT(*) WHERE result IN (win, mastery)`, all-time only.
  - Deepest dive: `MAX(depth)`, monthly + all-time.
  - Top score: `MAX(score)`, monthly + all-time.
- Page: extend `leaderboard/state.rs::Board` with the door boards inside the
  existing "Games" rail group, after the Lateania boards. The `Standings`
  enum already models single-window boards (Wins is all-time only; give it its
  own arm or reuse `Snapshot` with an honest heading, decide in-session).
  `format_value` per board ("N wins", "depth N", raw score).
- The leaderboard pass grows by a known number of queries: update the count in
  `hub/svc.rs` comments, `hub/CONTEXT.md`, and root `CONTEXT.md` (currently
  eleven).

## Badges and payouts

Mirror the NetHack award machinery (`door/nethack/award.rs`), generalized into
a shared door-award sink fed by ingestion instead of per-door screen scrapes:

- Reward templates (migration, lifetime claim policy like
  `nethack_amulet`/`nethack_ascension`): `dcss_orb` 10,000, `dcss_win` 20,000,
  `brogue_escape` 10,000, `brogue_mastery` 20,000.
- Profile award categories + badge codes in `late-core`
  (`profile_award.rs`, chat label logic in `user.rs`): DCSS `DCO` (Orb),
  `DCW` (win); Brogue `BRE` (Escaped), `BRM` (Mastered). Same collapse rule as
  NHA/NHY: the lesser badge collapses in chat labels when the greater is
  present, profile views show both. Update the Badge Codes legend.
- Grants are once per lifetime per account: lifetime payout claim + the
  `NOT EXISTS` award insert, both idempotent, so re-wins and re-ingests pay
  nothing. NetHack keeps NHA/NHY codes and history; existing holders are
  naturally protected by the same idempotent inserts when the xlogfile
  backfill replays their old games. Decide explicitly whether backfilled
  historical wins should grant (recommended: yes, it is the same achievement;
  the idempotence makes it safe).
- Feed events from ingestion: deaths (with depth) and wins post
  `ActivityKind::GameEvent`/`GameWon` lines like the current NetHack scrape
  does, now for all three games. "Started a game" stays connect-based in the
  client (it never was a scrape). Latency is seconds (tail push), fine for
  #lounge.

## NetHack scrape removal (the cleaning) — DONE in Phase 2

Once xlogfile ingestion grants badges and posts death events:

- Delete `door/nethack/milestone.rs`, `door/nethack/status.rs`, their tests,
  and the `scan_screen` scrape path + debounce flags in
  `door/nethack/state.rs`. `award.rs` either becomes the shared door sink or
  dies in favor of one.
- Root `CONTEXT.md` and the nethack CONTEXT both describe the scrape as "the
  one exception to no late.sh persistence"; rewrite those sections (§1, §7 of
  the nethack CONTEXT) around the log pipe.
- The descent feed lines are gone unless LIVELOG exists (Phase 2 checks). If
  LIVELOG exists, its achievement moments (quest artifact, Gehennom entry,
  amulet pickup) are better flavor than the level-band lines anyway.

## dcss-stats.com publishing (Phase 4)

Goal: late.sh joins the public-server ecosystem, whose tooling (dcss-stats,
Sequell) ingests servers by fetching logfile/milestones over HTTP and links
morgue dumps per game.

- Serve `$HOME/.crawl/logfile`, `milestones`, and `morgue/` read-only over
  HTTP from the `late-dcss` pod: a small axum listener in the crate (second
  port), path-sanitized, directory listing for morgues. Ingress route TBD
  with Mat (`crawl.late.sh` or `late.sh/crawl/*`), TLS via existing ingress.
- Coordinate with the dcss-stats maintainer before building URL shapes: what
  layout their fetcher expects, whether 0.34.1 is fine, whether the server
  needs registering in Sequell's server list first.
- Player names are arcade handles, public by design; no privacy gate needed.
- Nice property: their fetcher and our ingestion read the same files, so each
  validates the other.

## Phases (each shippable alone)

1. **DCSS end-to-end** — DONE (see Status above; one build change was needed
   after all: the `DGL_MILESTONES` define). Patterns set for the later
   phases: host stats session (`late-dcss/src/stats.rs`), client slice
   (`late-ssh/src/app/door/ingest/`), shared award sink
   (`ingest/award.rs`, `DoorBadge`), `DoorGame` roster boards.
2. **NetHack** — DONE (see the Phase 2 handoff above; LIVELOG panned out, so
   live Amulet-at-pickup timing and achievement flavor came back from a
   file, and the scrape is deleted).
3. **Brogue**: victory-logging patch + isolation-script-style fail-closed
   grep, run-history parser (per-player files: the tailer walks
   `players/*/`), badges, boards.
4. **dcss-stats HTTP publishing** after maintainer contact.

## Testing guidance (repo policy applies, tests adjacent to code)

- Parsers are pure: feed them real captured lines (grab samples from the dev
  hosts), assert the full parsed struct. Include the hostile cases: unknown
  fields, dead-handle names, `late_*` legacy names, truncated last line.
- Ingestion idempotency: DB test replaying the same lines twice, assert one
  row and one badge.
- Badge grants: once-per-lifetime NOT EXISTS behavior, collapse rules in
  `user.rs`/`profile_award.rs` tests (NHA/NHY tests are the template).
- Board queries: extend `late-core/src/models/leaderboard_test.rs`.
- Transport: a listener smoke test against a stub host, patterned on
  `door/rebels/proxy_test.rs`.
- Run targeted tests via `ARGS="..." make test-llm` (env-var form, not a make
  argument).

## Open questions for Mat (ask before the phase that needs them)

- ~~Badge codes `DCO`/`DCW` fine?~~ Approved 2026-08-07 (Phase 1). ~~`BRE`/`BRM`?~~
  Approved 2026-08-08 (Phase 3).
- ~~Should backfilled historical wins grant badges/chips?~~ Yes, approved
  2026-08-07.
- ~~Ingress shape for the DCSS files?~~ Path prefix (`late.sh/crawl/*`),
  decided 2026-08-08: the axum listener serves under `/crawl/...` natively (no
  rewrite annotation), a `Prefix` path rule on the `var.DOMAIN` host out-ranks
  late-web's `/` catch-all by longest-match, reusing the existing DNS record
  and cert. Confirm with the dcss-stats maintainer that a path-prefixed base
  URL suits their fetcher before building (Phase 4).
- ~~Do Rapid/Bullet Brogue variant games count on the Brogue boards?~~ No,
  filter to standard Brogue only. Approved 2026-08-08. (They share the player
  dir; verify how the run history line identifies the variant while reading
  the format.)
