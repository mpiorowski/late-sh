# Plan: the first hour, then the ladder, then the post

Status: nothing built. The diagnosis and every number behind this plan
live in `AUDIENCE.md` (sections 3 to 5); this file holds the steps, in
order, and the gate between them. As each step lands, move its behavior
into the domain's `CONTEXT.md` and delete it here; delete the file when
nothing is left.

## Goal

A newcomer's first session ends with one daily board won and tomorrow
named, and the next weekly cohort's two-day return moves off 5%. Then the
streak that session started is something they can see and others can
see. Then, and only then, a launch post.

The order is the evidence. Of 628 signups in four weeks, the 7% who won a
daily on day one came back on two days at five times the rate of the
rest, and the 3% who completed a daily quest at seven times. The tour
loses 54% of newcomers before its end and lets none of them play a
board. The July front-page post poured about 100 sessions into that leak
and settled back to 40 within days.

| | Now | Target after step 1 |
|---|---|---|
| Newcomers who win a daily on day one | 7% | 25% |
| Back on two or more days in the first week | 5% | 9% |
| Tour completion | 46% | 80% |

## Step 1: the first session, built around the daily board

Two domains, so two tasks: the Arcade first, then the tour.

**1a. The Arcade lends the tour today's board.** Where:
`late-ssh/src/app/arcade/` (`netwalk/state.rs` already has `open_daily`,
`is_solved`, `ensure_current_daily`; the lobby's own open path is the
reference).

- One public function on the Arcade surface that opens today's Netwalk
  daily at its easiest difficulty for this session and reports solved,
  used by the tour and nothing else. The win it produces is a real one:
  `record_win_task` runs, the chips land, the quest ticks when Netwalk is
  drawn, the solvers roster gains the name. No practice mode, no fake
  board.
- Netwalk because it has no loss state, takes two to three minutes, and
  was the second most-won daily within a week of shipping. Le Word is the
  board people talk about, but a newcomer can lose it in six guesses and
  a lost first board is a worse send-off than none.
- Tests: `netwalk/state_test.rs`, the lent board is today's, the easiest
  difficulty, and `is_solved` flips on the last rotation.

**1b. The tour.** Where: `late-ssh/src/app/clubhouse/` (`state.rs`:
`Tutorial`, `TourStep`, `TourMove`; `ui.rs`: the stop copy; `fight.rs`
goes), `late-ssh/src/app/input.rs` (`handle_tour_gate`, `tour_advance`,
`start_tour`), `late-ssh/src/app/render.rs` (the held surface),
`late-ssh/src/metrics.rs` (`tour_stop_label`).

- Four stops: `Welcome`, `VisitChat`, `VisitBoard`, `Homecoming`. Every
  other variant goes, and the exhaustive matches on `Tutorial` find each
  site that drew, gated, counted or moved them. The Stations stop goes
  outright rather than behind a client check: 84% of newcomers are on
  plain ssh with nothing to hear, and the radio keeps its place in the
  `?` guide.
- `VisitBoard` holds the lent board under the tour header the way
  `VisitTable` held the practice table: the gate hands the board's keys
  to the Arcade, keeps every other key, and Enter moves on only once
  `is_solved`. A terminal the board does not fit gets the table's answer:
  the header says so and Enter walks on. Nobody is held hostage by a
  board; nobody skips it on a terminal that can show it.
- The copy, kept plain. Welcome: a late-night clubhouse in a terminal,
  real people, one board a day. Chat: three lines, `i` to write, where
  Mentions live, that everyone here is a person. Board: everyone gets
  this same board today; solve it, then we go. Homecoming names
  tomorrow and nothing else: a new board at 00:00 UTC with the hours
  left, the streak at day one, the paper at the door, three keys.
- The practice table (`lobby/daily/state.rs`: `open_practice_table`,
  `practice_break`, `practice_played`, `clubhouse::ui::table_stop`,
  `TableStop`) and `fight.rs` have no caller outside the tour. They go in
  the same change, which touches `lobby/daily` for the deletion only.
- `/onboard` reruns the same four stops.
- Telemetry: `tour_stop_label` shrinks to the four stops plus `done`;
  the Newcomers panel reads `sum by (stop)` and needs no change. The
  drop into `board` against `done` is the board's cost.
- Tests: `clubhouse/state_test.rs` drives Enter from `Welcome` to `Done`
  and asserts the whole state after each step (the route is a pure
  state machine). The gate: a board key reaches the Arcade at
  `VisitBoard`, Enter before `is_solved` stays, Enter after moves on.
- Docs: `clubhouse/CONTEXT.md` §5 rewritten; `arcade/CONTEXT.md` gains
  the lent board; `lobby/daily/CONTEXT.md` loses the practice table;
  `late-ssh/CONTEXT.md` Telemetry lists the stops; `AUDIENCE.md` turns
  the targets above into measurements.

**Open decisions**, recommendation first:

- Where it ends. The tour ends in the tavern today and 40% of day-one
  sessions end on that screen; the people are in #lounge. End on Home.
- Chat as a required step. Keep it a stop, not a requirement: forced
  hellos spam #lounge, and chatting on day one predicts return at half
  the board's rate.
- The streak. The first session's win should be day one of a streak even
  when Netwalk is not a drawn quest, which means "any daily win advances
  the streak" (step 2) ships here, in late-core `quest.rs` beside
  `record_daily_quest_streak_if_complete`, called from the Arcade's daily
  win path. Do it here.

**Measure, then gate.** Deploy, wait seven days for a full weekly cohort,
wait seven more for its first week, then run the funnel queries in
`AUDIENCE.md` section 6 and read the Newcomers row. Step 2 can start
while that cohort runs. Step 3 waits for two-day return at 9% or more on
one cohort, or two cohorts at 8%.

## Step 2: the ladder on the dailies

Half of it is built (`late-ssh/src/app/hub/CONTEXT.md`): two daily
quests and a weekly, `user_daily_quest_streaks`, a six-slot streak meter
with a chip bonus at the top of the Arcade lobby. It holds 52 to 64
completers a week, flat since September, and 35 live streaks. The gap is
visibility and the day-one handshake, not more quest kinds.

- **Any daily win advances the streak**, if step 1 did not already do
  it. The quest slots stay as the chip-paying layer on top.
- **The streak is visible.** On the profile; beside the name in the
  Arcade solvers roster (`arcade/solvers.rs`); on the share card, where
  "day 12" is the Wordle line and travels outside the house for free.
  People want to be seen (`AUDIENCE.md` section 4): standing in the
  message line and on the profile gets used, a meter on one page does
  not.
- **The keypress-return moment says what it is at.** When a session
  wakes after hours idle (the AFK edge in `app/state.rs`), one private
  line: the streak's day and that today's boards are open. Private, so
  the feed budget does not apply.
- **The reward is standing, not chips.** The streak bonus pays about
  46,000 chips a week to 33 to 36 people, a faucet into an economy whose
  median balance is the stipend; with every daily win counting it would
  pay 85. Decision: stop the chip bonus when the mark exists, or cap it
  at the first week. Chips cannot be what the ladder is for.
- **"Did you know", one line per login.** Selling the house to someone
  who came back, not to a stranger. On the second login and the few
  after it, one dim sentence in a place the screen already has, chosen
  by login count and shown once each, never a modal: the paper already
  owns the login modal and a second box behind it is the one people
  learn to Esc through. The order follows what predicts return: login
  two, a new board is up and the streak is at one; login three, the
  radio plays in the `late` CLI, one line to install, or in a browser at
  `late.sh/listen`; login four, the people, the map, the profile; later
  the booth, the Artboard, the Nightcap. Never two lines at once.
- **The radio nudge where the hand already is.** A plain-ssh session
  that opens the Stations modal or presses the music keys is told right
  there that nothing plays over plain ssh and where it can. This beats
  any schedule: the person is already reaching for it. Measured by the
  share of second-week newcomers on the CLI (16% today) and the listen
  page's views (147 in 25 days). Before the radio line is written,
  read whether newcomers who arrive on the CLI come back more than the
  plain-ssh ones (logs keep a week, so a rough read on the latest
  cohort); if they do not, the line moves later in the series.
- **Not built:** XP, levels, leagues. A league needs cohorts of similar
  players; 85 weekly players is one league.
- Measure: live streaks (35, six past 30 days) and weekly distinct daily
  players (85). The ladder has worked if both move, not if wins per
  player moves alone.

## Step 3: the post

- **Gate:** step 1 measured and met, step 2's streak visible, so the
  first thing a newcomer meets is the hook and the first thing they can
  show is the streak.
- **Capacity:** `SCALE.md`, about 225 sessions on the node as shared
  against the July peak of 100; check replicas and the screensaver byte
  share the day before. Nothing to build.
- **Where:** Show HN with `ssh late.sh`; the paper at `/paper` is the
  landing for anyone who reads before they connect; the herald posts the
  same day on both accounts. Then, a week apart, the smaller rooms:
  lobste.rs, tildeverse, the terminal subreddits.
- **Watch during:** concurrency, signups a day, the Newcomers row, the
  tour stops live.
- **Read after:** the post's cohort through the same funnel queries
  against the organic cohorts beside it. That number decides whether to
  post again, and where.

## Not in this plan

- deadchannel: the gates stay, the game is the prestige track and fills
  as the house fills; arena and bets wait on runners present
  (`late-ssh/src/app/deadchannel/GAME.md`).
- The roguelike screen-reading helper, big new games, TradeWars: the
  reasons are in `AUDIENCE.md` section 5 and `devdocs/DOOR-CANDIDATES.md`.
- New Arcade dailies: welcome at any time as hours left open and as
  day-one boards, counted as depth, not as this plan's lever.
