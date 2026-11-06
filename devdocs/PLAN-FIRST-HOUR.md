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
  (`welcome`, `chat`, `table`, `board`, `done`; `board` against `done`
  is the board's cost). Step 2 can start while that cohort runs. Step 3
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
   purpose: "tomorrow is your first star: easy Sudoku" is tomorrow
   named with a prize instead of a time. Measure: the rung's roster
   size against the other boards (`AUDIENCE.md` section 6), active
   minutes on rung days.
3. **The marks beside the name.** The chat author label already carries
   the bonsai glyph, the country and the allowlisted badges; the ladder
   adds at most two cells and keeps that budget. The streak is a moon
   that fills, slow, so a one-day streak is not a badge: nothing under
   three days, `☽` at 3, `◑` at 7, `●` at 30, `◉` at 100; break the run
   and it is gone until day 3 again. The rung is `✦` in amber (the
   tour's own glyph), beside the name today only, gone at 00:00 UTC:
   the live signal that makes someone in chat do the board. No number
   in the line; the count lives on the profile and the share card,
   where "day 12" is the Wordle line and travels outside the house
   for free. Single-cell glyphs, no emoji; the label's wide-glyph path
   covers ambiguous-width terminals. People want to be seen
   (`AUDIENCE.md` section 4): standing in the message line gets used,
   a meter on one page does not.
4. **The week, the rung's surface.** One widget, built in the Arcade,
   drawn twice. In the Arcade lobby, above the game cards: seven days,
   each a small ASCII badge for its rung (about 9 by 5 so seven fit in
   80 columns), today starred if won, tomorrow highlighted, the day's
   roster count under it, one line under the row: "12 boards, one a
   day, all new at 00:00 UTC". Time-ordered, where the cards below stay
   game-ordered. On the tour, as the fifth stop after the board: today's
   badge starred, tomorrow's badge large (about 20 by 8) with its name,
   the other five as names, look only, Enter to the chat; it gets its
   own funnel label so its cost is a number. Tomorrow large and the
   rest small, because seven big badges to a newcomer is a menu. Twelve
   small badges and twelve large ones are one-off art. The week cannot
   ship before the rotation: until then it has nothing honest to show.
5. **The keypress-return moment says what it is at.** When a session
   wakes after hours idle (the AFK edge in `app/state.rs`), one private
   line: the streak's day, today's rung and that it is open. Private,
   so the feed budget does not apply.
6. **"Did you know", one line per login.** Selling the house to someone
   who came back, not to a stranger. On the second login and the few
   after it, one dim sentence in a place the screen already has, chosen
   by login count and shown once each, never a modal: the paper already
   owns the login modal and a second box behind it is the one people
   learn to Esc through. The order follows what predicts return: login
   two, the rung is up and the streak is at one; login three, the
   radio plays in the `late` CLI, one line to install, or in a browser
   at `late.sh/listen`; login four, the people, the map, the profile;
   later the booth, the Artboard, the Nightcap. Never two lines at once.
7. **The radio nudge where the hand already is.** A plain-ssh session
   that opens the Stations modal or presses the music keys is told right
   there that nothing plays over plain ssh and where it can. This beats
   any schedule: the person is already reaching for it. Measured by the
   share of second-week newcomers on the CLI (16% today) and the listen
   page's views (147 in 25 days). Before the radio line is written,
   read whether newcomers who arrive on the CLI come back more than the
   plain-ssh ones (logs keep a week, so a rough read on the latest
   cohort); if they do not, the line moves later in the series.

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
