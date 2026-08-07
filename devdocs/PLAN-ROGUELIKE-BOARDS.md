# PLAN: Roguelike Door Boards, Badges, and the Log Pipe

Status: approved plan, not started. Written 2026-08-07 for a fresh-context
session. The Lateania Games boards on the Leaderboards page shipped separately
(see `late-ssh/src/app/hub/CONTEXT.md`, Leaderboard Data); this plan covers the
three external roguelike doors: NetHack, DCSS, Brogue.

Read first: root `CONTEXT.md`, `late-ssh/src/app/hub/CONTEXT.md`, and the three
door contexts (`late-ssh/src/app/door/{nethack,dcss,brogue}/CONTEXT.md`). Each
door context's §9/Deferred section already names the log files this plan builds
on.

## Decisions already made (owner-approved, do not re-litigate)

1. **Drop the NetHack vt100 scrape entirely.** Badges, chip payouts, and death
   feed events all move to the xlogfile. The Amulet badge grants at end of run
   instead of at pickup, and the mid-run "descended to level N" #lounge lines
   go away, unless the pinned NetHack 5.0.0 source has the LIVELOG compile
   option (check during Phase 2; if present, enable it and live events come
   back from a file).
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

- **NetHack:** confirm XLOGFILE is compiled into the 5.0.0 build (check the
  pinned tarball's config; distro-style builds usually enable it, ours is
  hand-built). If it needs a define, add it in the `nethack-build` stage of
  `docker/doors/nethack.Dockerfile` with a fail-closed grep, same pattern as
  the existing SAFERHANGUP assertion. While in there, check for LIVELOG
  (newer NetHacks have it; it live-logs notable moments and would restore live
  feed events and amulet-at-pickup timing).
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

## NetHack scrape removal (the cleaning)

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

1. **DCSS end-to-end** (no build changes needed, richest files): stats
   session on `late-dcss` + client task + cursors + `door_runs`/
   `door_milestones` + parsers + DCSS badges/payouts/feed events + the three
   DCSS boards on the page. This sets every pattern.
2. **NetHack**: XLOGFILE build assertion, xlogfile parser, badge grants move
   to ingestion, delete the scrape, death events from the log, LIVELOG
   investigation. The cleaning phase.
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

- Badge codes `DCO`/`DCW`/`BRE`/`BRM` fine, or different letters?
- Should backfilled historical wins grant badges/chips? (Recommended yes.)
- Ingress shape for the DCSS files: subdomain or path prefix?
- Do Rapid/Bullet Brogue variant games (same run history file) count on the
  Brogue boards, or filter to standard? (They share the player dir; the run
  history line should identify the variant, verify while reading the format.)
