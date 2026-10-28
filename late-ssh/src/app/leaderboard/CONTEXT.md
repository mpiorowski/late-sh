# Leaderboard Context

## Metadata
- Scope: `late-ssh/src/app/leaderboard`, the top-level Leaderboards page (screen `6`) and `LeaderboardService`, plus the roster-generated data model in `late-core/src/models/leaderboard.rs` and the monthly `profile_awards` snapshot machinery it drives (`AWARDS.md`).
- Purpose: local working context for everything leaderboard: the refresh service and its cost rules, the board rosters and queries, the page, and the local seed script.
- Parent context: `../../../../CONTEXT.md`

## Scope

This slice owns the Leaderboards page and the service feeding it. It reads
fact tables other domains own (daily-win tables, score tables, `chip_ledger`,
`mud_characters`, the door log-pipe tables `door_runs`/`door_milestones`) but
must not own those runtimes or write paths. The Shop/quest/aquarium surfaces
stay with `app/hub` (`hub/CONTEXT.md`); chip primitives stay in
`late-core/src/models/chips.rs`.

The door log pipe that writes those two fact tables lives in
`app/door/ingest/` and has its own context there. Monthly awards and milestone
badges are in [`AWARDS.md`](AWARDS.md).

## Source Map

- `state.rs`: `Board` (the closed page-board enum; page order: Top Drinkers, Top Chips, Arcade Wins, Late Time, then the game boards, the two Lateania snapshot boards then the per-door triples, then the daily/score rosters), `Standings` (one arm per window shape: `MonthlyOnly`, `AllTimeOnly`, `Snapshot`, `Paired`, `MonthlyYearly`; the renderer matches all, so a new shape cannot fall through to a wrong heading), titles/hints/value formatting, and the selection state.
- `input.rs`: rail navigation keys/clicks and pointer-targeted wheel scrolling; Ctrl+J/K scrolls the detail pane.
- `ui.rs`: the board rail (Boards group leading, then Games, Daily Wins, High Scores) and the detail pane with per-window standings columns and the around-you ellipsis tail.
- `svc.rs`: `LeaderboardService` — the refresh loop, subscriber gate, connect-triggered top-up, process-local online-time accumulator/five-minute batch writer, and the rollover-aware `profile_awards` snapshot loop.
- Data model: `late-core/src/models/leaderboard.rs` (rosters, queries, `LeaderboardData`); awards in `late-core/src/models/profile_award.rs`.
- Read-only from here: `app/door/ingest/` (the pipe filling `door_runs`/`door_milestones`; `door/ingest/CONTEXT.md`).

## Page navigation

- `j/k` and arrows select boards with keyboard wraparound. Clicking a visible board row selects it; headings, separators, and the rail divider are inert. The wheel over the rail selects one board per event and stops at either end.
- `Ctrl+J`/`Ctrl+K` scroll the detail pane down/up one row; the wheel over that pane scrolls three rows. Monthly and all-time share an offset, with each column stopping at its own bottom. Titles and window headings stay fixed. The Badge Guide scrolls by wrapped rows.
- Offset zero retains the leaders-and-your-rank summary when at least three rows fit; scrolling shows loaded ranks in order. Column widths and the value alignment inside them come from the whole loaded snapshot, so nothing shifts sideways while scrolling. Changing boards resets the offset; clicking the current board preserves it. No extra queries: the existing 500-rank snapshot depth and refresh cadence still apply.
- Hit regions come from the rendered rail lines, including its viewport offset. Resize and page rendering invalidate stale targets. Keyboard-only mode and overlying modals block page mouse actions.

## Refresh model

`LeaderboardService` refreshes `LeaderboardData` from DB every 5 minutes, and
only while at least one session is subscribed, publishing it through a
`watch::Receiver<Arc<LeaderboardData>>`. The cadence is deliberately coarse:
a 30s pass measured 13% of all DB execution time (SCALE.md, DB Cost
Ranking). The pass is **fifteen queries**: each board family is
one union query ranked with `PARTITION BY game`; the Lateania boards add two
(both O(players) over `mud_characters`) and the roguelike-door boards two
(one query per window over `door_runs`/`door_milestones`, all three families
ranked `PARTITION BY (family, game)`), Late Time adds one query over its
indexed all-time and current-month O(users) rollups, and Top Drinkers one
over the current year of `drink_pours` (indexed on `created`). Do not make it hot again
without re-reading that ranking.

Two rules keep the coarse cadence from reading as a broken screen:

- **Sessions seed, they do not wait.** `App::new` copies the currently published snapshot out of the receiver with `borrow()`. `watch::Sender::subscribe` marks the current value as already seen, so the `has_changed()` gate in `app/tick.rs` is false against a snapshot that is sitting right there — a session that only waited for the gate would render empty panels for up to a full `REFRESH_INTERVAL`. The seed deliberately does not touch `chip_balance`, which is loaded accurately at login and may be newer than the snapshot.
- **A connect can buy one refresh.** `subscribe` wakes the refresh loop through a `Notify`, and `should_refresh` (a pure function, unit-tested in `svc_test.rs`) grants the pass only when the published snapshot is already older than `REFRESH_INTERVAL`. This covers the quiet-server case, where the subscriber gate skipped every pass and the first session back would otherwise seed from whatever the last session left behind. The age bound is what keeps a connect storm on a busy server from putting the pass back on the hot path.

Refresh is polling-based, so Activity events can appear before the page
catches up: a score set at minute 0 shows on the board within 5 minutes, not
at once. The boards are never *empty*, just up to one interval behind; there
is no leaderboard notify path (quest/shop snapshots have one, this does not).
The one surface that does not wait is the Arcade lobby card's own-user
✓/✗ tier marks: `App.session_daily_wins` (`arcade/daily.rs::SessionDailyWins`)
is marked from the session's own `GameWon` Activity events, which the daily
services publish only after the win row commits, and the card ORs it with the
snapshot's `user_daily_statuses`. The same snapshot carries
`today_solver_names` (the username of every user with a mark today, from the
same query) for the Arcade card's `N solved` count and its `o` roster
(`arcade/solvers.rs`). Other players' standings still wait.

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

- `DailyPuzzle` (LeWord, RubiksCube, SlidingPuzzle, Sudoku, Nonogram, Minesweeper, Solitaire, Netwalk, ChessPuzzle, WordWheel, Quartets, the Arcade lobby's order, so the per-game boards on this page, the week card's Arcade section and the paper's tie-breaks follow the lobby): iterating `ALL` generates the per-puzzle monthly/all-time win-count boards, Arcade Wins points, today's champions, per-user daily completion statuses, and the month-end `arcade_wins` profile award snapshot (`profile_award.rs::snapshot_previous_month_profile_awards` builds its union from the roster with `points_sql`, so the persisted award scores the same games as the page). The one thing the roster cannot enforce: a new game's win-insert statement must compose `bump_daily_win_total_sql`, or its all-time board stays empty. Monthly boards count win rows in the month window; all-time reads the `daily_win_totals` rollup (migration 131; the bump rides the win insert's own statement, gated to fresh inserts, so same-day replays never double-count and the refresh stays O(players)).
- `ScoreGame` (TwentyFortyEight, Lateris, Snake, Traffic, the lobby's order too): monthly boards union `game_score_events` with legacy best-score rows whose best was set this month (`updated`); all-time boards read only the legacy tables of record. `updated` is when the best was set, not when the player last played: `HighScore::update_score_if_higher` (Lateris, 2048, Snake) moves it only when the submitted score beats the stored one, otherwise anyone who played this month would carry their all-time best onto the monthly board and into the month's awards. A best that no score event reached (one set before score events existed) has nothing to date it by, so its `updated` can be later than when it was set. Traffic is the exception: every track finish rewrites `traffic_high_scores` and records the aggregate total as the score event, so its monthly board lists the all-time total of whoever finished a track this month.
- `DoorGame` (Dcss, Nethack, Brogue): the uniform board triple over the log-pipe fact tables — Wins (all-time count of `DoorRunResult::WINS` results, single window by design, `Standings::AllTimeOnly`), Deepest Dive, and Top Score (monthly + all-time). `WINS` is win + mastery, so a Brogue escape and a Brogue mastery both count once each on its wins board. The dive board unions end-of-run depth with the depth snapshot on every tracked milestone line, because crawl stamps the *final* place on the logfile line and a winner ends at the surface; NetHack's `maxlvl` and Brogue's `deepestLevel` are already run maximums, so their milestone rows contribute nothing to the union (Brogue writes no milestone rows at all: it logs only at end of game).
- `LobbyGame` (the Lobby's thirteen correspondence games, by `daily_matches.game_kind`): no board of its own; it names the Lobby's games for the week. It mirrors late-ssh's `DailyGame`, and `lobby/daily/games_test.rs` fails until the two agree on kind and name.
- The week (`week_bests`, `WEEK_DAYS`): outside the page, one best per game over the seven UTC days ending on a day, generated from all four rosters (the most daily wins by puzzle date, the top score among the week's `game_score_events`, the most Lobby wins that paid among matches finished in the week, the top scoring door run that ended in it), nameless: `WeekBest` is the game and the value, with `shown_value` as the boards print it, and never who, since the card is public and nobody opted into being posted. Read by late-web's `/games/week/{last}/image.png` card, the herald's Sunday post (`late-ssh/src/app/herald/CONTEXT.md`).
- The day (`day_tallies`, `DayGame`, `DayTally`): one tally per game anyone played on a UTC day, plays and wins, from the same rosters counted instead of ranked: the daily puzzles' finishes, the score games' runs, the Lobby's finished matches, and the hub's doors by openings (`door_openings`, every `DoorCard`, with the roguelikes' won runs beside them; `DoorGame::card` ties the three to their cards). Read by the paper's YESTERDAY column (`late-ssh/src/app/paper/CONTEXT.md`).
- `Shelf` (Arcade, Lobby, Doors) is the one grouping of the rosters, `WeekGame::shelf` and `DayGame::shelf`: the sections of the week's card and the lines of the paper's column.
- `Top Chips` (monthly sum over `chip_ledger` of the moves whose `ChipMove::counts_as_earnings` is true, exclusions derived from `ChipMove::excluded_earning_reasons()`: only credits count, and of those the house tables (bets, payouts, floor restores), gifts, the starting stipend, the two referral payouts and every monthly prize (the gallery's `ArtboardPrize`, and any month-end prize added later) are out, so a colluding table cannot fold one seat to the top, nobody can be funnelled up, nobody is on the board for signing up, and no single invite or month-end win decides the board; gilds received, the pot, the bonsai drip, Super Snake winnings and every game prize count as they land; no debit ever counts, so spending cannot lower a place or push a player off the board; admin `/grant` chips never reach the ledger; the rule and its reasoning live on `counts_as_earnings` in `late-core/src/models/chips.rs`) and `Arcade Wins` are bespoke monthly-only boards. Arcade Wins weights come from `Difficulty::points` over each board's tier in `DailyPuzzle::tiers` (easy = 1, medium/draw-1 = 3, hard/draw-3 = 5; the three-tier games use all three, Le Word is a single Easy board, Rubik's a single Medium one), the same tier whose `chips()` is the daily-win payout, so points and payouts cannot drift apart. `points_sql` is generated from the tiers and pinned whole by `leaderboard_test.rs::points_sql_follows_each_puzzles_tiers`. Unknown difficulty keys score 0, never a default.
- `Late Time` is a bespoke paired board over the current-month and all-time online-time rollups. It ranks exact milliseconds and renders the largest two useful units. Last month's first place gets the rankless `LATE` badge (see Monthly profile awards); no chips are attached.
- The two Lateania boards are snapshot boards over the game-owned `mud_characters` JSONB blobs, not event tables: `lateania_adventurers` ranks living characters by level with experience as the tiebreak and carries the class in `RankedEntry.note` (the one board note in the system; the page renders it dim after the username and drops it when width runs short). Experience keeps accruing at the level cap, so past 100 the value keeps counting **paragon levels**, one per 75k xp beyond the cap's threshold (the summit's own per-level price), rendered `lvl 100 +37`; the curve numbers are restated in `leaderboard.rs` (`LATEANIA_LEVEL_CAP`, `LATEANIA_XP_AT_LEVEL_CAP`, `LATEANIA_XP_PER_PARAGON_LEVEL`) and pinned to the game's `xp_for_level` by `lateania/classes_test.rs`, so a rebalance that forgets them fails there. `lateania_pvp` ranks the blob's lifetime `pvp_kills` (rivals slain in the Wildbound Waste); characters with no kills stay off it. A reset character leaves both boards; the page shows one "right now" window (`Standings::Snapshot`).
- `Top Drinkers` is a bespoke monthly + yearly board (`Standings::MonthlyYearly`, the one board with a yearly window) over `drink_pours`, the per-drink log written by `UserDrinks::record_pour` in the same statement as the buzz (`late-core/src/models/drinks.rs`): the sum of each drink's buzz points in the UTC month and the UTC year. What counts is the drink taken, never the chips: a paid drink, a round credit cashed and the round buyer's own drink all count their points (a tavern round's 400, a Nightcap round's 100), buying for others counts nothing, and the newcomer's welcome pour is never logged. Points are the drink's full worth before the `MAX_DRUNK_POINTS` cap. Last month's first place gets the rankless `DRNK` badge (see Monthly profile awards); no chips, and the yearly window awards nothing.
- There is no Le Word win-streak board, by decision: its gaps-and-islands query was the most expensive in the pass.

Monthly windows use UTC calendar months. No refresh query scans full history.

## The door log pipe (what fills `door_runs`/`door_milestones`)

The roguelike-door boards read `door_runs`/`door_milestones`, which the door
log pipe in `app/door/ingest/` fills from host-written log files, never from
the terminal. Its contract (transport, cursors and idempotency, handle
identity, grant gates, feed gating, deploy order) and the settled badge
decisions are in `late-ssh/src/app/door/ingest/CONTEXT.md`.

## The page

Screen `6`, board rail + detail view. The rail leads with the Boards group
(Top Drinkers, Top Chips, Arcade Wins, Late Time), then the Games group (the Lateania boards,
then each door's board triple), Daily Wins, and High Scores, in roster order.
The first board, and the one selected when the page opens, is Top Drinkers. The detail pane shows
the selected board's window(s) with an around-you tail (the viewer's row
replaces the last two rows below the fold). Ctrl+J/K and the wheel scroll the
standings through every loaded rank (see Page navigation).

## Monthly profile awards

`LeaderboardService::start_profile_award_snapshot_loop` freezes last UTC
month's top 3 per awarded board into `profile_awards` (migrations 077, 081),
and every board is settled by the first pass that writes a row for it. The
gallery award pays chips; Late Time and Top Drinkers are
single-holder and rankless; one-time milestone badges share the table; the roll
is announced in #lounge once per month. **A new badge has to be added to the
Leaderboards guide (`app/profile_modal/badges.rs::guide_lines`) and the help
modal (`app/help_modal/data.rs`) by hand.** The full rules are in
[`AWARDS.md`](AWARDS.md).

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
- The pipe behind the door boards: `app/door/ingest/` tests (`door/ingest/CONTEXT.md`).
- Seed-on-connect behavior: `app/state_test.rs::leaderboard_seeds_from_the_already_published_snapshot`.

## Known gaps

- No notify-driven refresh; up to one `REFRESH_INTERVAL` of staleness by design (see Refresh model).
- Online-time crash loss is bounded by the last successful five-minute checkpoint while the DB is healthy; cross-pod overlap is intentionally not deduplicated.
