# Plan: the first hour, then the ladder, then the post

Status: step 1 built, not yet deployed or measured. The diagnosis and
every number behind this plan live in `AUDIENCE.md` (sections 3 to 5);
this file holds the steps, in order, and the gate between them. As each
step lands, move its behavior into the domain's `CONTEXT.md` and delete
it here; delete the file when nothing is left.

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

Built as the tour's `Board` route, beside the old `House` route;
`clubhouse::state::TOUR_ROUTE` picks the one in force and flipping it is
the whole revert. What it is lives in
`late-ssh/src/app/clubhouse/CONTEXT.md` §5 (the routes, the gate, the
board stop) and `late-ssh/src/app/arcade/CONTEXT.md` (the lent board).
The design reasoning behind it, every decision and the ones turned
down, is in Claude Code session `2fc40c39-0365-400e-b8c0-dd75cbf98223`
(`claude --resume` in this repo). What is left of the step:

- **The streak and the star.** The first session's win is day 1 of the
  streak, now that any daily win keeps the day, and the send-off names
  tomorrow's rung as the first star (step 2's first two pieces, built).
- **Measure, then gate.** Deploy, wait seven days for a full weekly
  cohort, wait seven more for its first week, then run the funnel
  queries in `AUDIENCE.md` section 6 and read the Newcomers row
  (`welcome`, `chat`, `table`, `board`, `week`, `done`; `board` against
  `week` is the board's cost). Step 2 can start while that cohort runs. Step 3
  waits for two-day return at 9% or more on one cohort, or two cohorts
  at 8%. If the cohort says the board loses more than it holds, the
  revert is the one line above, and the `House` route's old numbers are
  in `AUDIENCE.md` section 3 to compare against.

## Step 2: the ladder on the dailies

Half of it is built (`late-ssh/src/app/hub/CONTEXT.md`): two daily
quests and a weekly, `user_daily_quest_streaks`, a six-slot streak meter
with a chip bonus at the top of the Arcade lobby. It holds 52 to 64
completers a week, flat since September, and 35 live streaks. The gap is
visibility and the day-one handshake, not more quest kinds. The quest
slots, their templates and their chips stay as they are; the ladder is
built beside them, and the one thing it takes from them is the streak.
Decided, in the order it ships; each piece has its own number to read:

1. **The streak: any daily win keeps the day, no chips.** Built; what
   it is lives in `late-ssh/src/app/hub/CONTEXT.md` (Quests). The
   floor is deliberately one minute: a streak's job is tomorrow, not
   depth, and a bad night at 23:50 has to be able to keep the day. The
   tour's Netwalk win is day 1 with no tour code knowing. The
   `bonus_level` column drop sits in `late-core/migrations/deferred/`
   until the release after this one ships. Measure: live streaks (35,
   six past 30 days), two-day return.
2. **The rung: one board a day, the same for everyone, and the star.**
   Built; what it is lives in `late-ssh/src/app/hub/CONTEXT.md`
   (Quests), the lobby's line in `arcade/CONTEXT.md`, the send-off in
   `clubhouse/CONTEXT.md` §5. The rotation is keyed to the calendar,
   never the person, so the rung's solvers roster is the day's crowd
   and the chat has one board to argue about; the knob is time, not
   the difficulty label. The tour's easy Netwalk is not a star, on
   purpose: "tomorrow is your first star: medium Sudoku" is tomorrow
   named with a prize instead of a time. Measure: the rung's roster
   size against the other boards (`AUDIENCE.md` section 6), active
   minutes on rung days.
3. **The marks beside the name.** Built; what it is lives in
   `late-ssh/src/app/chat/RENDERING.md` (the label) and
   `late-ssh/src/app/hub/CONTEXT.md` (the moon and the star). Measure:
   messages per day from people carrying a mark against the rest, and
   the rung's roster on days a star was visible in the lounge
   (`AUDIENCE.md` section 6).
4. **The week, the rung's surface.** Built; what it is lives in
   `late-ssh/src/app/arcade/CONTEXT.md` (the week) and
   `late-ssh/src/app/clubhouse/CONTEXT.md` §5 (the week stop, funnel
   label `week`). Measure: `week` against `done` in the tour funnel
   and two-day return against the step 1 cohort (`AUDIENCE.md`
   section 6).
5. **The keypress-return moment says what it is at.** Built; what it is
   lives in `late-ssh/src/app/hub/CONTEXT.md` (the push). It is the empty
   composer's row, not a chat line: the chat's AFK divider sits where the
   person stopped reading, out of view after a busy night, and the
   composer is where the return keypress lands. Measure: rung wins in
   the hour after `late_ssh_return_pushes_total{rung="open"}` against
   the hour after any other return.
6. **"Did you know", one line per login.** Built; what it is lives in
   `late-ssh/CONTEXT.md` (the help modal bullet) and
   `late-ssh/src/app/common/splash_tips.rs`. Selling the house to someone
   who came back, not to a stranger. On the second login and the few
   after it, the splash's dim tip line, a place the screen already has,
   chosen by `users.login_count` and shown once each, never a modal: the
   paper already owns the login modal and a second box behind it is the
   one people learn to Esc through. The order: login two, the radio, the
   station count and the YouTube booth, in the `late` CLI or in a browser
   at `late.sh/listen`, shown to every session, CLI ones included; login
   three, the rung is up and a daily win starts the streak; login four,
   the people, the map, the profile; then the booth, the Artboard, the
   Nightcap. Never two lines at once. Measure: second-week newcomers on
   the CLI and the listen page's views against
   `late_ssh_did_you_know_total{line="radio"}`.
7. **The radio nudge where the hand already is.** Built; what it is
   lives in `late-ssh/src/app/audio/CONTEXT.md` (the plain-ssh nudge). A
   plain-ssh session that opens the Stations modal or the Music Booth, or
   presses the music keys, is told right there that nothing plays over
   plain ssh and where it can. This beats any schedule: the person is
   already reaching for it. Measured by the share of second-week
   newcomers on the CLI (16% today) and the listen page's views (147 in
   25 days), against `late_ssh_radio_nudges_total{spot}`. The read that
   gated it, on the week of session summaries to 7 Nov 2026 (accounts
   created 31 Oct to 5 Nov, arrival client from the first session,
   return is a session on a later day): 14 arrived on the CLI and 2 came
   back (14%); 72 arrived on plain ssh and 6 came back (8%). Small
   numbers, same direction as the bet, so the nudge stays in its place.

- **Not built:** XP, levels, leagues, rerolls or skips. A league needs
  cohorts of similar players; 85 weekly players is one league. Rerolls
  exist only when a prescribed board can be unwanted; with any board
  keeping the day and the rung a bonus on top, there is nothing to
  reroll.
- Measure, for the step as a whole: live streaks (35, six past 30 days)
  and weekly distinct daily players (85). The ladder has worked if both
  move, not if wins per player moves alone.

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
