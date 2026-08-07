# Leaderboard Context

## Metadata
- Scope: `late-ssh/src/app/leaderboard` — the top-level Leaderboards page (screen `6`) and `LeaderboardService` — plus the roster-generated data model in `late-core/src/models/leaderboard.rs` and the monthly `profile_awards` snapshot machinery it drives.
- Last updated: 2026-08-07 (file created: the Leaderboard Data ownership moved here from `hub/CONTEXT.md`, and `LeaderboardService` moved from `app/hub/svc.rs` to `app/leaderboard/svc.rs` so the slice owns its service)
- Purpose: local working context for everything leaderboard: the refresh service and its cost rules, the board rosters and queries, the page, monthly profile awards, and the local seed script.
- Parent context: `../../../../CONTEXT.md`

## Scope

This slice owns the Leaderboards page and the service feeding it. It reads
fact tables other domains own (daily-win tables, score tables, `chip_ledger`,
`mud_characters`, the door log-pipe tables `door_runs`/`door_milestones`) but
must not own those runtimes or write paths. The Shop/quest/aquarium surfaces
stay with `app/hub` (`hub/CONTEXT.md`); chip primitives stay in
`late-core/src/models/chips.rs`; the ingestion pipe that fills the door
tables belongs to the doors (`app/door/ingest/` plus
`app/door/{dcss,nethack}/CONTEXT.md`).

## Source Map

- `state.rs`: `Board` (the closed page-board enum: two Lateania snapshot boards, the per-door triples, Top Chips, Arcade Wins, then the daily/score rosters), `Standings` (one arm per window shape: `MonthlyOnly`, `AllTimeOnly`, `Snapshot`, `Paired`; the renderer matches all, so a new shape cannot fall through to a wrong heading), titles/hints/value formatting, and the selection state.
- `input.rs`: rail navigation keys.
- `ui.rs`: the board rail (Games group leading, then Boards, Daily Wins, High Scores) and the detail pane with per-window standings columns and the around-you ellipsis tail.
- `svc.rs`: `LeaderboardService` — the refresh loop, subscriber gate, connect-triggered top-up, and the daily `profile_awards` snapshot loop.
- Data model: `late-core/src/models/leaderboard.rs` (rosters, queries, `LeaderboardData`); awards in `late-core/src/models/profile_award.rs`.

## Refresh model

`LeaderboardService` refreshes `LeaderboardData` from DB every 5 minutes, and
only while at least one session is subscribed, publishing it through a
`watch::Receiver<Arc<LeaderboardData>>`. The cadence is deliberately coarse:
the old fourteen-query pass was 13% of all DB execution time at 30s (SCALE.md
DB Cost Ranking). Today the pass is **thirteen queries**: each board family is
one union query ranked with `PARTITION BY game`; the Lateania boards add two
(both O(players) over `mud_characters`) and the roguelike-door boards two
(one query per window over `door_runs`/`door_milestones`, all three families
ranked `PARTITION BY (family, game)`). Do not make it hot again without
re-reading that ranking.

Two rules keep the coarse cadence from reading as a broken screen:

- **Sessions seed, they do not wait.** `App::new` copies the currently published snapshot out of the receiver with `borrow()`. `watch::Sender::subscribe` marks the current value as already seen, so the `has_changed()` gate in `app/tick.rs` is false against a snapshot that is sitting right there — a session that only waited for the gate would render empty panels for up to a full `REFRESH_INTERVAL`. The seed deliberately does not touch `chip_balance`, which is loaded accurately at login and may be newer than the snapshot.
- **A connect can buy one refresh.** `subscribe` wakes the refresh loop through a `Notify`, and `should_refresh` (a pure function, unit-tested in `svc_test.rs`) grants the pass only when the published snapshot is already older than `REFRESH_INTERVAL`. This covers the quiet-server case, where the subscriber gate skipped every pass and the first session back would otherwise seed from whatever the last session left behind. The age bound is what keeps a connect storm on a busy server from putting the pass back on the hot path.

Refresh is polling-based, so Activity events can appear before the page
catches up: a score set at minute 0 shows on the board within 5 minutes, not
at once. The boards are never *empty*, just up to one interval behind; there
is no leaderboard notify path (quest/shop snapshots have one, this does not).

## Data model (roster-generated)

`late-core/src/models/leaderboard.rs`: the `DailyPuzzle`, `ScoreGame`, and
`DoorGame` closed enums drive every derived surface, so a new game added to a
roster automatically joins its boards without a page change (the `roster!`
macro guarantees an `ALL` entry per variant, and `Board::all()` iterates the
rosters).

- `DailyPuzzle` (Sudoku, Nonogram, Minesweeper, Solitaire, LeWord, RubiksCube): iterating `ALL` generates the per-puzzle monthly/all-time win-count boards, Arcade Wins points, today's champions, and per-user daily completion statuses. The one thing the roster cannot enforce: a new game's win-insert statement must compose `bump_daily_win_total_sql`, or its all-time board stays empty. Monthly boards count win rows in the month window; all-time reads the `daily_win_totals` rollup (migration 131; the bump rides the win insert's own statement, gated to fresh inserts, so same-day replays never double-count and the refresh stays O(players)).
- `ScoreGame` (Lateris, TwentyFortyEight, Snake, Traffic): monthly boards union `game_score_events` with legacy best-score rows touched this month; all-time boards read only the legacy tables of record.
- `DoorGame` (Dcss, Nethack; Brogue joins in its `PLAN-ROGUELIKE-BOARDS.md` phase): the uniform board triple over the log-pipe fact tables — Wins (all-time count of `DoorRunResult::WINS` results, single window by design, `Standings::AllTimeOnly`), Deepest Dive, and Top Score (monthly + all-time). The dive board unions end-of-run depth with the depth snapshot on every tracked milestone line, because crawl stamps the *final* place on the logfile line and a winner ends at the surface; NetHack's `maxlvl` is already the run maximum, so its milestone rows simply contribute nothing to the union.
- `Top Chips` (monthly net chip delta from `chip_ledger`, exclusions derived from `ChipMove::excluded_earning_reasons()`) and `Arcade Wins` are bespoke monthly-only boards. Arcade Wins weights come from `Difficulty::points` (easy/draw-1 = 1, medium = 3, hard/draw-3 = 5; Le Word fixed Easy, Rubik's fixed Medium), the same enum whose `chips()` carries the daily-win payout tiers, so points and payouts cannot drift apart. Unknown difficulty keys score 0, never a default.
- The two Lateania boards are snapshot boards over the game-owned `mud_characters` JSONB blobs, not event tables: `lateania_adventurers` ranks living characters by level with experience as the tiebreak and carries the class in `RankedEntry.note` (the one board note in the system; the page renders it dim after the username and drops it when width runs short); `lateania_frontier` unnests each blob's visited-room array for the deepest Frontier zone (rooms 2000..=2999, 50 per zone, constants restated in `leaderboard.rs` with a pointer at `lateania/world.rs`). A reset character leaves both boards; the page shows one "right now" window (`Standings::Snapshot`).
- The Le Word win-streak board was deliberately dropped (the gaps-and-islands query was the most expensive in the pass).

Monthly windows use UTC calendar months. No refresh query scans full history.

## The page

Screen `6`, board rail + detail view. The rail leads with the Games group
(Lateania boards, then each door's triple), then Boards (Top Chips, Arcade
Wins), Daily Wins, and High Scores, in roster order. The detail pane shows
the selected board's window(s) with an around-you tail (the viewer's row
replaces the last two rows below the fold). There is no scrolling inside a
board's standings beyond that tail; a board deeper than the pane clips.

## Monthly profile awards

- Migration 077 adds `profile_awards`, one permanent row per user/category/month placement; 081 enforces top-3.
- `LeaderboardService::start_profile_award_snapshot_loop` runs once at startup and then daily as catch-up: it creates missing previous-UTC-month rows and leaves existing rows frozen. Awarded categories: `top_chips`, `arcade_wins`, `tetris` (renders as Lateris), `twenty_forty_eight`, `snake`, ranks 1-3.
- One-time rankless milestone awards share the table (granted immediately, shown regardless of award month): Lateania bosses (`LMG`, `LKN`, `LYS`, `LKA`), NetHack (`NHA` Amulet, `NHY` ascension), DCSS (`DCO` Orb pickup, `DCW` escape) — both door pairs granted by the log pipe's award sink — and Green Dragon (`GDS`). Chat author labels collapse a lesser badge when its superseding one is present (`user.rs::chat_profile_award_badges`); profile views show all, plus the always-appended `Badge Codes` legend.
- Chat author labels show top-3 last-completed-UTC-month award badges as one bracketed group; Top Chips badges render as `CHIP1`-`CHIP3`.

## Local seed data

An empty local database renders every panel as "no scores yet".
`make seed-leaderboard` (`scripts/seed_leaderboard_test_data.{sh,sql}`) fills
the Compose database with 48 synthetic players spread across every board,
including `mud_characters` blobs for the Lateania boards and DCSS
`door_runs`/`door_milestones` rows (winners, quits, spread depths, Orb
milestones carrying the dive depth; `seed:`-prefixed source files so the
idempotency key can never collide with real ingested lines). Local
development only: it owns the `seed:leaderboard:` fingerprints, prefixes
usernames with `lb_`, and rewrites their stats on every rerun. With no
argument it also gives the most recently active real user a representative
deep-rank row on every board (insert-only where real state could exist);
pass a username to target that enrichment explicitly.

## Testing guidance

- Board/rail/window layout: `state_test.rs`, `ui_test.rs` (pure).
- The refresh gate (`should_refresh`, pure channel state): `svc_test.rs` — the one sanctioned inert-`Db` test (it makes no DB calls; see the root Test Strategy exception).
- Query behavior: `late-core/src/models/leaderboard_test.rs` (DB-backed, fixtures through production constructors).
- Seed-on-connect behavior: `app/state_test.rs::leaderboard_seeds_from_the_already_published_snapshot`.

## Known gaps

- No notify-driven refresh; up to one `REFRESH_INTERVAL` of staleness by design (see Refresh model).
- No in-board scrolling beyond the around-you tail.
- Brogue has no boards yet; its ingestion phase is next in `devdocs/PLAN-ROGUELIKE-BOARDS.md`.
