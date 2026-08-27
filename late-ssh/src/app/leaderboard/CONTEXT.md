# Leaderboard Context

## Metadata
- Scope: `late-ssh/src/app/leaderboard` — the top-level Leaderboards page (screen `6`) and `LeaderboardService` — plus the roster-generated data model in `late-core/src/models/leaderboard.rs` and the monthly `profile_awards` snapshot machinery it drives.
- Last updated: 2026-08-26 (the crown joins the monthly snapshot: category `crown`, badge `CRWN`, granted to whoever held the crown when the UTC month ended. It is the first monthly award without a rank digit, so `profile_award.rs` now splits "rankless" (`is_rankless_award`) from "shown forever" (`MILESTONE_AWARD_CATEGORIES`))
- Purpose: local working context for everything leaderboard: the refresh service and its cost rules, the board rosters and queries, the door log pipe that fills the door boards, the page, monthly profile awards, and the local seed script.
- Parent context: `../../../../CONTEXT.md`

## Scope

This slice owns the Leaderboards page and the service feeding it. It reads
fact tables other domains own (daily-win tables, score tables, `chip_ledger`,
`mud_characters`, the door log-pipe tables `door_runs`/`door_milestones`) but
must not own those runtimes or write paths. The Shop/quest/aquarium surfaces
stay with `app/hub` (`hub/CONTEXT.md`); chip primitives stay in
`late-core/src/models/chips.rs`.

One documentation exception: the **door log pipe** writes those two fact
tables and lives in `app/door/ingest/`, but its contract is what the door
boards and badges are made of, so the cross-door half of it is documented
here (see "The door log pipe" below) rather than repeated in three door
files. Per-game log formats, build flags, and host internals stay in
`app/door/{dcss,nethack,brogue}/CONTEXT.md`; code changes to the pipe belong
to the doors.

## Source Map

- `state.rs`: `Board` (the closed page-board enum; page order: Top Chips, Arcade Wins, Late Time, then the game boards, the two Lateania snapshot boards then the per-door triples, then the daily/score rosters), `Standings` (one arm per window shape: `MonthlyOnly`, `AllTimeOnly`, `Snapshot`, `Paired`; the renderer matches all, so a new shape cannot fall through to a wrong heading), titles/hints/value formatting, and the selection state.
- `input.rs`: rail navigation keys.
- `ui.rs`: the board rail (Boards group leading, then Games, Daily Wins, High Scores) and the detail pane with per-window standings columns and the around-you ellipsis tail.
- `svc.rs`: `LeaderboardService` — the refresh loop, subscriber gate, connect-triggered top-up, process-local online-time accumulator/five-minute batch writer, and the daily `profile_awards` snapshot loop.
- Data model: `late-core/src/models/leaderboard.rs` (rosters, queries, `LeaderboardData`); awards in `late-core/src/models/profile_award.rs`.
- Read-only from here, documented below: `app/door/ingest/` (the pipe filling `door_runs`/`door_milestones`, models `late-core/src/models/{door_run,door_milestone,door_log_cursor}.rs`, migration `136_create_door_ingestion.sql`).

## Refresh model

`LeaderboardService` refreshes `LeaderboardData` from DB every 5 minutes, and
only while at least one session is subscribed, publishing it through a
`watch::Receiver<Arc<LeaderboardData>>`. The cadence is deliberately coarse:
the old refresh pass was 13% of all DB execution time at 30s (SCALE.md
DB Cost Ranking). Today the pass is **fourteen queries**: each board family is
one union query ranked with `PARTITION BY game`; the Lateania boards add two
(both O(players) over `mud_characters`) and the roguelike-door boards two
(one query per window over `door_runs`/`door_milestones`, all three families
ranked `PARTITION BY (family, game)`), and Late Time adds one query over its
indexed all-time and current-month O(users) rollups. Do not make it hot again
without re-reading that ranking.

Two rules keep the coarse cadence from reading as a broken screen:

- **Sessions seed, they do not wait.** `App::new` copies the currently published snapshot out of the receiver with `borrow()`. `watch::Sender::subscribe` marks the current value as already seen, so the `has_changed()` gate in `app/tick.rs` is false against a snapshot that is sitting right there — a session that only waited for the gate would render empty panels for up to a full `REFRESH_INTERVAL`. The seed deliberately does not touch `chip_balance`, which is loaded accurately at login and may be newer than the snapshot.
- **A connect can buy one refresh.** `subscribe` wakes the refresh loop through a `Notify`, and `should_refresh` (a pure function, unit-tested in `svc_test.rs`) grants the pass only when the published snapshot is already older than `REFRESH_INTERVAL`. This covers the quiet-server case, where the subscriber gate skipped every pass and the first session back would otherwise seed from whatever the last session left behind. The age bound is what keeps a connect storm on a busy server from putting the pass back on the hot path.

Refresh is polling-based, so Activity events can appear before the page
catches up: a score set at minute 0 shows on the board within 5 minutes, not
at once. The boards are never *empty*, just up to one interval behind; there
is no leaderboard notify path (quest/shop snapshots have one, this does not).

### Late Time persistence

Late Time counts authenticated human presence across SSH and IRC, including
idle/AFK time. `active_users` remains the authoritative cross-protocol
connection ref-count: only its 0→1 transition starts a monotonic `Instant`, and
only 1→0 stops it, so overlapping sessions for one user count once. Ghost users
never enter this path.

The accumulator checkpoints on the same five-minute timer as the leaderboard.
It performs no DB work on connect/disconnect and writes every changed user in
one `UNNEST` statement that atomically updates `user_online_time` (one all-time
row per user) and `user_online_time_monthly` (one row per user and UTC month);
no pending time means no statement. A retained flush UUID and month make an
uncertain retry idempotent, and shutdown performs a final serialized flush.
Tracking starts with migrations 142-143 and has no invented historical
backfill. A connected segment is attributed to the UTC month in which that
segment began; the regular checkpoint bounds month-boundary spill to five
minutes while Postgres is healthy. A hard process crash can lose the current
interval since the last checkpoint. Deduplication is process-local, matching
the one-replica service; a rolling deployment's brief old/new-pod overlap may
overcount.

## Data model (roster-generated)

`late-core/src/models/leaderboard.rs`: the `DailyPuzzle`, `ScoreGame`, and
`DoorGame` closed enums drive every derived surface, so a new game added to a
roster automatically joins its boards without a page change (the `roster!`
macro guarantees an `ALL` entry per variant, and `Board::all()` iterates the
rosters).

- `DailyPuzzle` (Sudoku, Nonogram, Minesweeper, Solitaire, LeWord, RubiksCube): iterating `ALL` generates the per-puzzle monthly/all-time win-count boards, Arcade Wins points, today's champions, and per-user daily completion statuses. The one thing the roster cannot enforce: a new game's win-insert statement must compose `bump_daily_win_total_sql`, or its all-time board stays empty. Monthly boards count win rows in the month window; all-time reads the `daily_win_totals` rollup (migration 131; the bump rides the win insert's own statement, gated to fresh inserts, so same-day replays never double-count and the refresh stays O(players)).
- `ScoreGame` (Lateris, TwentyFortyEight, Snake, Traffic): monthly boards union `game_score_events` with legacy best-score rows touched this month; all-time boards read only the legacy tables of record.
- `DoorGame` (Dcss, Nethack, Brogue): the uniform board triple over the log-pipe fact tables — Wins (all-time count of `DoorRunResult::WINS` results, single window by design, `Standings::AllTimeOnly`), Deepest Dive, and Top Score (monthly + all-time). `WINS` is win + mastery, so a Brogue escape and a Brogue mastery both count once each on its wins board. The dive board unions end-of-run depth with the depth snapshot on every tracked milestone line, because crawl stamps the *final* place on the logfile line and a winner ends at the surface; NetHack's `maxlvl` and Brogue's `deepestLevel` are already run maximums, so their milestone rows contribute nothing to the union (Brogue writes no milestone rows at all: it logs only at end of game).
- `Top Chips` (monthly net chip delta from `chip_ledger`, exclusions derived from `ChipMove::excluded_earning_reasons()`) and `Arcade Wins` are bespoke monthly-only boards. Arcade Wins weights come from `Difficulty::points` (easy/draw-1 = 1, medium = 3, hard/draw-3 = 5; Le Word fixed Easy, Rubik's fixed Medium), the same enum whose `chips()` carries the daily-win payout tiers, so points and payouts cannot drift apart. Unknown difficulty keys score 0, never a default.
- `Late Time` is a bespoke paired board over the current-month and all-time online-time rollups. It ranks exact milliseconds and renders the largest two useful units; no profile award or reward is attached.
- The two Lateania boards are snapshot boards over the game-owned `mud_characters` JSONB blobs, not event tables: `lateania_adventurers` ranks living characters by level with experience as the tiebreak and carries the class in `RankedEntry.note` (the one board note in the system; the page renders it dim after the username and drops it when width runs short); `lateania_frontier` unnests each blob's visited-room array for the deepest Frontier zone (rooms 2000..=2999, 50 per zone, constants restated in `leaderboard.rs` with a pointer at `lateania/world.rs`). A reset character leaves both boards; the page shows one "right now" window (`Standings::Snapshot`).
- The Le Word win-streak board was deliberately dropped (the gaps-and-islands query was the most expensive in the pass).

Monthly windows use UTC calendar months. No refresh query scans full history.

## The door log pipe (what fills `door_runs`/`door_milestones`)

The three external roguelike doors feed their boards, badges, chips, and feed
lines from **host-written log files, never from the terminal**. Shipped in four
phases over 2026-08-07..10 (DCSS, NetHack + scrape removal, Brogue, then the
DCSS file publishing); `devdocs/PLAN-ROGUELIKE-BOARDS.md` is the build record
that the migrations and a few source comments still point at.

- **Transport: a stats SSH session on the door host.** Each host reserves one
  SSH username, `late_stats` (inside the already-reserved `late_*` handle
  namespace, so no player can claim it). Instead of a game child it opens a log
  stream: the client pushes its per-file byte offsets in env requests
  (`LATE_DOOR_STATS_CURSORS`, `logfile:123,milestones:456`; a large cursor set,
  e.g. Brogue's one-file-per-player history, is split across several requests
  the host concatenates), the host streams
  one `<file-id>\t<next-offset>\t<line>` frame per complete line with tail -f
  semantics, and stays **stateless**: no cursor storage, no parsing, no DB. All
  parsing lives in late-ssh, so a parser fix never needs a door redeploy, and
  door pods never hold DB credentials. Chosen over a new HTTP ingest surface
  because it reuses the russh servers and shared secrets already there.
- **Client side.** `app/door/ingest/`: `svc.rs` orchestration (one
  connect-with-retry task per enabled door, spawned from `main.rs` behind that
  door's `LATE_*_ENABLED`), `stream.rs` the stats SSH client, `dcss.rs` /
  `nethack.rs` / `brogue.rs` pure parsers, `award.rs` the shared
  `DoorAwards`/`DoorBadge` sink. Cursors persist in `door_log_cursors`.
  Observability: `late_ssh_door_ingest_lines_total` and
  `late_ssh_door_ingest_session_failures_total` (both labeled by game), so a
  dead stats session or a poisoned frame is visible in monitoring, not just a
  30s-retry warn log.
- **Idempotency.** Unique `(game, source_file, source_offset)` on both fact
  tables, with the fact insert and the cursor advance committing in one
  transaction. Files are append-only and hosts single-replica, so offsets are
  stable; a fresh cursor starts at 0 and ingests whatever history is already on
  the PVC, which is why every door board launched non-empty. A file that shrinks
  (playground rebuilt) restarts from 0 and the idempotent inserts absorb the
  replay.
- **Identity.** The playname on every line is the account's **arcade handle**
  (NetHack `-u`, DCSS `-name`, Brogue's player directory name), mapped through
  `arcade_handles` (unique on `lower(handle)`). Handle rows outlive accounts
  with `user_id` NULL and those are skipped, as are the reserved `late`/`late_*`
  shapes (NetHack's legacy `late_<hex>` lines predate handles).
- **Grants are idempotent; the badge is once, the chips repeat.** Badges and
  chips fire from `award.rs` on every win or pickup. The badge is guarded by a
  `NOT EXISTS` award insert and lands once per account for life. The chips go
  through `credit_run_cooldown_reward_template`, an all-or-nothing grant behind
  two gates at once (SHOP.md Phase 6): the ingested line's own
  `(source_file, source_offset)` key, so a re-ingest or a crash between fact
  insert and grant settles to exactly one payout, and a 7-day per-account
  lockout per milestone, so a lucky week pays once. Both gates refusing writes
  nothing at all. That is what makes backfill safe.
- **Feed events are gated twice.** Deaths and wins post to #lounge only when the
  fact row is freshly inserted AND the event is inside a 10-minute recency
  window, so a backfill of years of history never floods the feed. "Started a
  game" stays connect-based in the client; it never was a scrape.
- **Never build boards or badges on a screen scrape.** NetHack's vt100 scrape
  was acceptable for cosmetic flair and was deleted in Phase 2; anything that
  pays chips or ranks a player reads the spoof-proof host files. Non-scoring
  games are excluded at the source, per door: wizard/explore xlogfile lines are
  flagged and skipped (and explore mode is locked off at the sysconf, since
  livelog lines carry no flag), crawl never logs wizard games, Brogue writes no
  run-history line for Easy or Wizard.
- **Deploy order matters.** The host half ships in that door's image-only
  release (`-dcss` / `-nethack` / `-brogue`), the client half with service-ssh:
  deploy the host first or together, or ingestion just retries against a host
  with no stats session. Manifest changes (a new initContainer file, a port, an
  ingress) ride `deploy_infra.yml` instead.
- **The DCSS files are also published outward.** The same `logfile`/`milestones`
  the pipe tails are served read-only over HTTP at `late.sh/crawl/...` for the
  public DCSS tooling (dcss-stats, Sequell), so their fetcher and this pipe read
  identical bytes and validate each other. Details in the DCSS CONTEXT §1;
  nothing on this page depends on it.

### Settled decisions (do not re-litigate)

- **Badge pairs, 20k/50k chips, one payout per run and one per week per
  milestone** (SHOP.md Phase 6; they were 10k/20k once per lifetime until
  migration 158), mirroring the original NetHack pair. DCSS's Orb *pickup* was chosen over first rune
  deliberately: it is the exact twin of the Amulet badge. **Every line grants
  only its own milestone.** A DCSS or NetHack win never back-grants the pickup
  it implies (decided 2026-08-27: the back-grant carried the win line's own key,
  so once the pickup's 7-day window had passed it paid the pickup a second
  time; and a pickup the milestone stream missed is an ingest bug to surface,
  not to patch from the win line). Brogue's Escaped/Mastered are alternative
  endings and grant only themselves for the same reason. The chat-label
  collapse is a display convention and implies nothing about granting.
- **Boards per door are uniform**: Wins (all-time), Deepest dive and Top score
  (monthly + all-time), joining the Games rail group. Adding a fourth door costs
  zero extra queries.
- **Backfilled historical wins grant** badges and chips (approved 2026-08-07);
  the idempotence above is what makes that safe.
- **Brogue variants do not count.** Rapid and Bullet Brogue write their own
  files beside the standard one and the host never opens them.
- **Badge codes** `DCO`/`DCW` (approved 2026-08-07) and `BRE`/`BRM` (approved
  2026-08-08).

## The page

Screen `6`, board rail + detail view. The rail leads with the Boards group
(Top Chips, Arcade Wins, Late Time), then the Games group (the Lateania boards,
then each door's board triple), Daily Wins, and High Scores, in roster order.
The first board, and the one selected when the page opens, is Top Chips. The detail pane shows
the selected board's window(s) with an around-you tail (the viewer's row
replaces the last two rows below the fold). There is no scrolling inside a
board's standings beyond that tail; a board deeper than the pane clips.

## Monthly profile awards

- Migration 077 adds `profile_awards`, one permanent row per user/category/month placement; 081 enforces top-3.
- `LeaderboardService::start_profile_award_snapshot_loop` runs once at startup and then daily as catch-up: it creates missing previous-UTC-month rows and leaves existing rows frozen. Awarded categories: `top_chips`, `arcade_wins`, `tetris` (renders as Lateris), `twenty_forty_eight`, `snake`, ranks 1-3, plus `crown`.
- **The crown (`CRWN`) is monthly but rankless.** The `crown_holder` CTE in `snapshot_previous_month_profile_awards` takes last month's latest `crown_reigns` row (by `taken_at`, whether or not it is still open, since the rollover is a read-time rule with no sweeper) and grants rank 1 with `paid_chips` as the score. One holder means a `#1` on the badge would be noise, so `award_badge` prints it bare. That splits two properties that used to coincide: `is_rankless_award` (no rank digit; milestones plus the crown) and `MILESTONE_AWARD_CATEGORIES` (shown in chat labels whatever month they were earned; milestones only). The crown shows for the month after and then makes way for the next holder's, like every other monthly badge. The full mechanic is `late-ssh/src/app/chat/CONTEXT.md` §9c.
- One-time rankless milestone awards share the table (granted immediately, shown regardless of award month): Lateania's four crowns (`LMG`, `LKN` 10k, `LYS`, `LKA` 20k), NetHack (`NHA` Amulet 20k, `NHY` ascension 50k), DCSS (`DCO` Orb pickup 20k, `DCW` escape 50k), Brogue (`BRE` escape 20k, `BRM` mastery 50k) — all three door pairs granted by the log pipe's award sink — Green Dragon (`GDS`, 10k), and A Dark Room (`ADE` the ascent won 15k, `ADB` the ascent won holding the fleet beacon off the ravaged battleship's command deck 20k; both granted by `door/darkroom/svc.rs::reward_escape`, migrations 143 and 145, claimed separately). **The badge is once per account; the chips repeat** (migration 158, SHOP.md Phase 6): the roguelike doors pay once per ingested run behind a 7-day per-milestone lockout, Lateania once per `mud_characters.id` behind the same lockout, Green Dragon for every kill (the kill resets the character), and A Dark Room for every run that gets out (the ending wipes the save). A badge row therefore says the feat happened, never how many times it paid; `game_payout_claims` and `chip_ledger` say that. The set of rankless milestone categories is `profile_award.rs::MILESTONE_AWARD_CATEGORIES`, which is what `award_badge` checks for the no-rank-suffix rule and what the chat-label query binds as a parameter rather than respelling in SQL. Chat author labels keep only the highest badge a player holds on each game's ladder (`profile_award.rs::BADGE_LADDERS` is the single ordering, applied by `user.rs::chat_profile_award_badges`); profile views show all, plus the always-appended `Badge Codes` legend. **A new badge has to be added to the Leaderboards guide (`app/profile_modal/badges.rs::guide_lines`) and the help modal (`app/help_modal/data.rs`) by hand** — those are authored prose, not generated, so `app/profile_modal/badges_test.rs` asserts both cover every milestone category (and the crown) and names any that a change forgot. Note the collapse rule is a display convention and does not imply a grant rule: `BRM` collapses `BRE` in chat labels, `NHY` collapses `NHA`, `DCW` collapses `DCO`, and `ADB` collapses `ADE`, but every one of those is granted only by its own line or ending; nothing back-grants the badge it collapses.
- Chat author labels show top-3 last-completed-UTC-month award badges as one bracketed group; Top Chips badges render as `CHIP1`-`CHIP3`.

## Local seed data

An empty local database renders every panel as "no scores yet".
`make seed-leaderboard` (`scripts/seed_leaderboard_test_data.{sh,sql}`) fills
the Compose database with 48 synthetic players spread across every board,
including online-time totals, `mud_characters` blobs for the Lateania boards,
and per-door `door_runs`/`door_milestones` rows for all three doors (winners, quits,
spread depths, DCSS Orb milestones carrying the dive depth, Brogue escapes
and masteries; each door's block is offset from the others so no two boards
mirror each other, and `seed:`-prefixed source files keep the idempotency key
from ever colliding with real ingested lines). Local
development only: it owns the `seed:leaderboard:` fingerprints, prefixes
usernames with `lb_`, and rewrites their stats on every rerun. With no
argument it also gives the most recently active real user a representative
deep-rank row on every board without lowering or overwriting real state;
pass a username to target that enrichment explicitly.

## Testing guidance

- Board/rail/window layout: `state_test.rs`, `ui_test.rs` (pure).
- The refresh gate (`should_refresh`, pure channel state): `svc_test.rs` — the one sanctioned inert-`Db` test (it makes no DB calls; see the root Test Strategy exception).
- Query behavior: `late-core/src/models/leaderboard_test.rs` (DB-backed, fixtures through production constructors).
- Online-time accumulator/retry behavior: `svc_test.rs`; SSH and IRC lifecycle hooks: `ssh_test.rs` and `ircd/serve_test.rs`.
- The pipe behind the door boards: `app/door/ingest/{dcss,nethack,brogue}_test.rs` (pure parsers against real captured lines, including unknown fields, dead and reserved handles, and truncated last lines), `stream_test.rs` (the stats client against a stub SSH host), `svc_test.rs` (DB-backed: replay idempotency, skipped names, the per-run payout and its 7-day lockout, a run past the lockout paying again with no second badge, cursor advancement).
- Seed-on-connect behavior: `app/state_test.rs::leaderboard_seeds_from_the_already_published_snapshot`.

## Known gaps

- No notify-driven refresh; up to one `REFRESH_INTERVAL` of staleness by design (see Refresh model).
- No in-board scrolling beyond the around-you tail.
- Online-time crash loss is bounded by the last successful five-minute checkpoint while the DB is healthy; cross-pod overlap is intentionally not deduplicated.
