# late.sh Audience Notes

Who comes to late.sh, what they do, where their time goes, what the numbers
say about it, and the ideas that follow. The capacity side of the same
numbers is `SCALE.md`; the game built on this diagnosis is
`late-ssh/src/app/deadchannel/GAME.md`.

Every number here is a measurement with its window named. Metrics come from
VictoriaMetrics (`late_ssh_*`, about 30 days kept), logs from VictoriaLogs
(7 days kept); the queries are at the end. Per-person figures need the
database, and the open questions that need it are listed there too.

## 1. The shape of the crowd

Measured 2026-10-15 to 2026-10-22 (logs) and 2026-09-27 to 2026-10-22
(metrics) unless stated.

| | |
|---|---|
| Concurrent sessions, weekly average | 24, 23, 24, 25, 24 over the five weeks ending 2026-11-06 (15, 21, 20, 23, 22 over the five ending 2026-10-22; the line is flat) |
| Concurrent sessions, 90-day peak | 43; 42 in the 60 days to 2026-11-06 |
| Distinct people connecting per day | 85 to 135 |
| Distinct people connecting per week | 371 (logs, week ending 2026-10-22); 295 seen in the 7 days and 1,069 in the 30 days to 2026-11-06 (`users.last_seen`) |
| Accounts | 18,726 on 2026-11-06; 291 with 8 h or more of lifetime, 172 with 24 h or more |
| Sessions per day | about 290, of which about 18 (6%) are a first session |
| Sessions per person per week | 170 people once, 89 two or three times, 59 four to seven, 34 eight to twenty, 19 more than twenty |
| Share of sessions from the 20 most frequent people | 49% |
| Share of connected time from the 10 longest-connected people | 45% (sampled from session summaries) |
| Session length, median | 4.6 min over plain ssh, 24 min through the `late` CLI |
| Session length, spread | 52% under 10 min, 14% between 1 h and 3 h, 9% over 3 h |
| Client split | 43% of sessions through the CLI, but they run five times longer, so about 58% of connected time |
| CLI platforms (pairs in 25 days) | Linux 3,579, macOS 2,192, Windows 313, Android 14 |
| Chat messages per day | 900 to 1,170 (weekly averages) |

The hour profile is flat: 19 to 20 sessions at any hour of the night
(UTC), rising to 28 between 18:00 and 19:00 UTC. Weekdays average 21 to
27 sessions, Saturday and Sunday 17 to 18. The crowd keeps a terminal open
through the working day and often overnight; the weekend dip says late.sh
is a desk companion more than an evening venue.

A person with one session in a week is not necessarily a drive-by: a
regular who leaves one session open all week also counts once. Session
counts measure reconnects, not visits.

The CLI is the regulars' client. Per person, over the session summaries
from 2026-10-20 to 2026-10-25 joined to lifetime `user_online_time`
(connected time, not keyboard time):

| People in the window | CLI only | Plain ssh only | Both | Mostly CLI, by time | Hours CLI / ssh |
|---|---|---|---|---|---|
| All 327 | 95 | 216 | 16 | 107 | 1,266 / 1,006 |
| The 81 with 24 h or more lifetime | 43 | 27 | 11 | 52 | 1,177 / 917 |

156 accounts had 24 h or more lifetime on 2026-10-25 (172 on 2026-11-06);
the other 75 did not connect in the window. Which client a session used is
in the logs only, so the split cannot reach further back than log
retention.

Newcomers arrive on plain ssh: of the 108 first-week accounts with a
session summary in the 7 days to 2026-11-06, 91 (84%) used it. The first
session is a bare terminal with no radio to hear.

## 2. Where the time goes

Attention is counted once a second per session, split by screen and by
place inside it, marked active (a key within the recent window) or idle,
and tagged with the account's tenure (its first day, its first week,
older). 2026-09-27 to 2026-10-22, about 13,900 session-hours in all, every
tenure together.
Places inside Home and Zen are labeled from 2026-10-02; the earlier 1,800
Home hours and 360 Zen hours sit in an unlabeled bucket.

**81% of all attention is idle.** The active 19% is 2,640 hours.

| Screen | All hours | Share | Active hours | Active share | Visits | Per visit |
|---|---|---|---|---|---|---|
| Home (chat) | 8,260 | 60% | 1,028 | 39% | 24,000 | 20 min |
| Arcade | 2,130 | 15% | 853 | 32% | 15,400 | 8 min |
| Zen | 1,804 | 13% | 177 | 7% | 2,400 | 45 min |
| Clubhouse | 623 | 4.5% | 118 | 4.5% | 7,000 | 5 min |
| A Dark Room | 186 | 1.3% | 65 | 2.5% | 171 | 65 min |
| Artboard | 185 | 1.3% | 113 | 4.3% | 4,500 | 2.5 min |
| Games hub | 143 | 1% | 33 | 1.2% | 4,300 | 2 min |
| Leaderboards | 102 | 0.7% | 16 | 0.6% | 2,200 | 3 min |
| Daily match boards | 84 | 0.6% | 51 | 1.9% | 2,100 | 2.4 min |
| Lateania | 83 | 0.6% | 45 | 1.7% | 206 | 24 min |
| People | 73 | 0.5% | 18 | 0.7% | 2,500 | 2 min |
| PTY doors, all ten | 146 | 1% | 95 | 3.6% | 630 | 14 min |
| House tables | 9 | 0.1% | 9 | 0.3% | 117 | 5 min |
| Calendars | 13 | 0.1% | 5 | 0.2% | 922 | under 1 min |
| Night city, Nightcap, Realm, Scratchpad | 24 | 0.2% | 16 | 0.6% | 600 | 2.5 min |

Doors in total (Lateania, A Dark Room, the Realm and the ten PTY doors):
3% of all time, 8% of active time.

**Inside Home** (labeled hours, 2026-10-02 on): #lounge 4,461 h (69%),
public topic rooms 1,409 h (22%), News 183 h (3%), private rooms 136 h,
cyberspace 85 h, DMs 71 h, #deadchannel 57 h before its switch (110 h in
the 14 days to 2026-11-06 after it),
Notifications 16 h, Discover 14 h, Feeds 4 h. Active time keeps the same
order: #lounge 561 h, topic rooms 134 h, News 20 h, DMs 10 h.

**Inside the Arcade** (hours on a board): Solitaire 270, Sudoku 190,
Nonogram 173, 2048 123, Le Word 89, Minesweeper 85, Tetris 72, Sliding
Puzzle 66, Rubik's Cube 14, Snake 10, Traffic 4. Boards finished per day:
160 to 285, climbing through October; 94% of finishes are the shared daily
board, 6% a personal one. By finishes Minesweeper leads (1,175), then
Sliding Puzzle, Le Word, Nonogram, Sudoku, Solitaire. Since Netwalk, Chess
Puzzle, Word Wheel and Quartets opened on 2026-10-28: 330 to 450 boards a
day, Netwalk second by wins within a week, while the week's distinct daily
players stayed at 85 (93 the week before) and daily wins per player per
week went from 13 to 22. A new daily is more boards from the same people.
Share cards per daily win, 14 days to 2026-11-06: Le Word 20%, Word Wheel
9%, every grid game under 2.5%.

**Inside the Artboard:** 143 h on the canvas, 7 h in the gallery. The
Artboard's active hours jumped from 1 to 3 a day to 21 and 31 on
2026-10-21 and 2026-10-22, the two days after PRINTS opened for runners.

## 3. What people do

Per day, 2026-09-27 to 2026-10-22 unless stated.

| Action | Per day |
|---|---|
| Chat messages | about 1,000 (plus 9 edits) |
| Arcade boards finished | 225 |
| Lateania fights won | 220 |
| Bonsai actions stored (bend, prune, water, pinch, split) | 430, plus 94 refused by a cooldown |
| CLI pairs | 245 |
| Login paper shown | 98; opened on purpose (browse or `/paper`) 7 |
| Idle screensavers | 77 (Earthrise is the pick four times out of five) |
| Pot tickets | 42 |
| Chat translations made | 28 (1.4 million served from cache) |
| Songs queued | 14 |
| Share cards | 13 |
| Door games watched | 9 (NetHack and Brogue most) |
| Nightcap drinks | 7 |
| Calendar writes | 5 |
| News stories shared | 4.4; the reader opened 3 times; a story played in the terminal 10 times in total |
| Gallery applause | 4 |
| Chat summaries | 3 |
| Zork launches | 2 |
| Rounds bought | 0.5 (13 in total) |
| Gilds | 0.3 (8 in total) |
| Chat YouTube links played in the terminal | 3 in total since the feature shipped |

Web, 25 days: the landing page 535,000 views (about 432,000 of them the
kubelet's readiness probe, every 5 s, from before it moved to `/health`; treat as noise), gallery 4,400, profile pages 3,700 plus 1,400 index views, terms
764, `/listen` 147, a live stream page 136.

Per person, 2026-10-23 to 2026-11-06 (14 days, database unless stated):

| | |
|---|---|
| Chat messages per day | 1,030 to 1,450 (740 to 1,020 in the three weeks before); the 10 busiest authors write 80% of them, the 20 busiest 90% |
| Daily quests completed per week | 250 to 280 by 52 to 64 people, flat since the week of 2026-09-28; weekly quests by 5 to 19 people |
| Daily quest streaks alive (a completion today or yesterday) | 35: 15 at 1 or 2 days, 10 at 3 to 6, 3 at 7 to 13, 1 at 14 to 29, 6 at 30 or more |
| Streak bonus paid | 33 to 36 people and about 46,000 chips a week |
| People who completed a daily quest | 64 in 7 days, 106 in 28, 524 ever |
| Doors, distinct people (openings) | the Realm 24 (254), Lateania 22 (148), A Dark Room 14 (132), night city 11 (110), NetHack 10 (68), Brogue 7 (53), DCSS 6 (72) |
| Lateania fights won | 63,000, 24,000 of them on 2026-10-30, from two level-100 characters at 212 M and 104 M xp: scripted, and it inflates the Games row, the paper's YESTERDAY line and the herald's Sunday card |
| Shop | the 181 purchases in the week of 2026-11-02 are the free aquarium claim, not chips spent |
| Chips | median balance 1,000 (the stipend), 90th percentile 1,100, top 353,000 |
| Referrals | 10 in 30 days |
| Radio, the roll call (`radio_listens`, from 2026-10-26) | 50 to 90 people a day on a station, 185 of 240 on chillsynth; rows written before the roll call was limited to paired, unmuted clients count the station picked, chillsynth is the default, and 148 of those 185 never touched the picker. Read the paper's top-stations line from the day that change shipped |
| Radio, the people who can hear it (78 CLI users with 10 min or more in the week to 2026-11-06) | 64 picked a station, 42 pinned slots; chillsynth 34, the other nightride.fm channels 22, the rest of the catalogue 22 (classical 6, chill 5, rock 3, coderadio 2, plaza 2, five others 1 each) |

**The newcomer funnel**, from the referral clock (section 6): the 628
accounts created 2026-10-02 to 2026-10-29, about 22 a day, followed for
their first week. An active minute is a UTC minute with keyboard input.

| | Accounts | Share |
|---|---|---|
| Typed anything on day one | 598 | 95% |
| 5 or more active minutes on day one | 331 | 53% |
| 20 or more active minutes on day one | 94 | 15% |
| Typed anything on days 2 to 8 | 93 | 15% |
| Typed on two or more of days 2 to 8 | 32 | 5% |
| One day of 20 or more active minutes in days 2 to 8 | 38 | 6% |
| Three such days | 10 | 2% |

Median active minutes in the week after day one: 0. The 90th percentile:
5. Each weekly cohort since the clock started reads the same within a
point or two, so this is the shape, not a bad week.

How deep day one went says who comes back:

| Active minutes on day one | Accounts | Typed again | On two or more days |
|---|---|---|---|
| 0 | 30 | 13% | 0% |
| 1 to 4 | 267 (43%) | 7% | 0% |
| 5 to 19 | 237 (38%) | 14% | 3% |
| 20 to 59 | 65 (10%) | 34% | 15% |
| 60 or more | 29 (5%) | 59% | 48% |

What day one held, read against the same return. This is correlation,
not proof: the people who did these may be the people who would have
stayed anyway.

| Did this on day one | Accounts | Typed again | On two or more days |
|---|---|---|---|
| Completed a daily quest | 19 (3%) | 58% | 37% |
| Won a daily puzzle | 44 (7%) | 34% | 25% |
| Sent a chat message | 157 (25%) | 26% | 12% |
| 5 or more minutes, neither won nor chatted | 177 (28%) | 16% | 5% |
| Everyone | 628 | 15% | 5% |

In days 2 to 7 the Arcade is the screen first-week accounts spend the
most active minutes on, ahead of Home (metrics, `tenure="first_week"`,
7 days to 2026-11-06).

**The first session is the tour.** Every first visit lands in the
Clubhouse and runs the forced tour (`late-ssh/src/app/clubhouse/CONTEXT.md`
§5). The route these numbers measure is the `House` one, 18 stops over
every page: Enter the only key that works, a pool break and a scripted
dungeon fight that must be played, the daily board a stop that is looked
at and not played, and nothing in the route that names tomorrow. Stops
reached on it, 14 days to 2026-11-06:

| Stop | Reached |
|---|---|
| welcome | 579 |
| chat | 499 |
| music (the Stations modal) | 475 |
| arcade | 348 |
| table (the pool break) | 338 |
| games | 288 |
| done | 265 |

46% finish. The biggest single drop is the Stations modal into the
Arcade page, on sessions that are 84% plain ssh with nothing to hear.
40% of day-one sessions end with the Clubhouse on screen
(`late_ssh_session_ends_total{tenure="day_one"}`). The route in force
now is the `Board` one (`TOUR_ROUTE`, one line to go back): four stops,
welcome, chat, one shot of pool, today's board, the board played for
real and won before the last Enter; its funnel is `welcome`, `chat`,
`table`, `board`, `done`, read against the targets in section 5 on the
first cohort that meets it.

**deadchannel**, switched on 2026-10-21, read 16 days later (2026-11-06):
16 runners, 13 of them touched the game that day and 15 in the week,
levels 1 to 11, no mark yet. The haunting since the switch, per connect:
1,792 gate passes, 39 bio screens, 24 whispers, 12 breakthroughs, 11
runners created. Fights: 17 to 33 roads started a day in the last week,
80 cleared and 21 dropped in all; 17 crossings, 11 won. Lounge spawns
land 4 a day: flickers put down 30 of 66, howlers 4 of 23 (two runners
are rarely on #lounge together), 9 distinct hitters. The wire carries 30
to 93 messages a day from 5 to 9 runners, and #deadchannel took 110 h of
Home attention in 14 days against 57 h in the three weeks before the
switch. The ceiling is the pool, not the ladder. Of the 295 people seen
in the week:

| | People |
|---|---|
| With 8 h online | 110 |
| With 8 h and a bio of 100 characters or more | 38 |
| Of those, already invited (16 hold a runner row) | 17 |
| With 8 h and no such bio | 72 |

About 20 people are left to haunt; dropping the hours gate to 2 adds
four. The bio is the binding leg, and by the game's own design the thing
that gets a regular to write one is the mark beside a runner's name,
which the room sees after the public flip.

## 4. What the numbers say

- **The first session is the leak.** Nine in ten newcomers type on day
  one and half of them stay five minutes, then 85% never press a key
  again and 95% never come back for a second day. The regulars were not
  made by the current first session either: 628 signups in four weeks
  produced 10 people with three active days. Arrivals are not the lever
  until that changes; a launch post today pours 100 people into a 95%
  leak, which is what July's Hacker News spike measured. The lever is
  the first hour: what a newcomer sees, is handed, and has a reason to
  return to tomorrow.
- **The first hour is a museum walk, and the exhibit that predicts a
  return is the one it does not let them touch.** The tour is 18 stops
  over every page with a pool break and a dungeon fight played by
  obligation; 46% reach the end and 43% of all signups are gone inside
  five active minutes. The daily board is a glance on the way past, yet
  the 7% who won one on day one came back on two days at five times the
  rate of everyone else, and the 3% who completed a daily quest at seven
  times. Depth says the same: an hour on day one and half come back
  twice, under five minutes and none do. The first session should get a
  newcomer to one finished daily and name tomorrow, and nothing else.
- **The Arcade's audience is fixed; a new daily deepens it.** Four new
  dailies doubled the boards finished a day and left the week's distinct
  daily players at 85, with wins per player up from 13 to 22. More boards
  is real hours-left-open value and the best day-one hook the site has,
  and it is not growth.
- **The station catalogue landed with the people who can hear it.** Among
  CLI users this week, four in five picked a station and half pinned
  slots; chillsynth holds under half of them, and the non-synth
  stations hold as many as the other nightride.fm channels. The roll
  call said otherwise only because it counted the default for the
  plain-ssh majority, who hear nothing; it now counts paired, unmuted
  clients only. Which station plays cannot be
  measured server-side for third-party streams (the CLI plays them
  direct), so the pick is the best read there is.
- **The quest strip is a progression system nobody is shown.** Two daily
  quests, a weekly, a six-slot streak meter and a chip bonus already run
  at the top of the Arcade lobby: 52 to 64 people a week complete one,
  flat since September, and 35 streaks are alive, six of them past 30
  days. It pays chips into a deflated economy, it is visible on one
  screen and in no message line or profile, and the tour gives it half a
  sentence. The streak is the strongest "tomorrow" the site owns and the
  newcomer never meets it.
- **deadchannel works for everyone in it and is out of people.** Thirteen
  of sixteen runners touch it on a given day, the wire doubled Home's
  attention on the room, and about 20 eligible people remain. The gates
  are the design, not a bug: the game is the prestige track and stays
  one, so its pool grows only as the house grows. The first hour feeds
  it; nothing inside it does.
- **late.sh is a terminal people leave open.** 81% of attention is idle,
  the night is as busy as the morning, and the longest-connected tenth of
  the crowd holds nearly half the connected time. Concurrency is weekly
  regulars times hours left open. The lever on a steady 50 is hours left
  open and daily return, not more arrivals: arrivals already run at 18
  first sessions a day and show up as a flat concurrency line.
- **Chat is the center and the Arcade is the second room.** The Arcade
  takes 32% of active time, nearly as much as all of chat, with 225
  boards a day and 94% of them the shared daily. It is the strongest
  daily ritual the site has, and it is single-player, zero-coordination,
  interruptible, with a shared object (today's board) to talk about. Any
  new ritual should copy that shape.
- **Doors are window-shopped.** The Games hub gets 4,300 visits of two
  minutes each; the ten PTY doors together hold 1% of time. The exception
  is A Dark Room at 65 minutes a visit, the save-that-grows shape
  (`devdocs/DOOR-CANDIDATES.md`, the incremental shelf). A door you
  travel to competes with chat for attention and loses; the number moves
  when the game happens in the room people already sit in.
- **Zen is the leave-it-open surface.** 1,800 hours at 45 minutes a
  visit, 90% idle. People park a terminal on it; the screensaver fires 77
  times a day. Anything that rewards being connected can ride it.
- **Anything needing synchronous coordination starves.** House tables: 9
  hours in 25 days, poker 6 minutes. Rounds 13, gilds 8. About three
  people want a live game at any moment, spread across every game type:
  a liquidity problem, not a UX problem. What works is ambient and
  interruptible.
- **People want to be seen, not to own things.** The shop's one hit is
  username colors and gradients: permanent, visible in every message,
  pure status. Chat effects do not sell because they are a moment half the
  room misses. Share cards (13 a day), gallery applause and the PRINTS
  spike say the same thing: a thing on your profile or in your message
  line gets used, a thing in a menu does not.
- **Care loops are alive at the action level and unmeasured at the person
  level.** The bonsai takes 430 actions a day; the aquarium logs sprouts,
  hatchings and starvations every day. Whether that is forty people or
  four needs the database (section 6). The structural read stands either
  way: nobody sees your bonsai unless they go looking, and day 40 is day
  4. A feature that cannot be seen from chat, the profile or the feed
  follows that curve however charming.
- **Half the crowd never logs in, so the return moment is a keypress.**
  Sessions stay open for days; a person's return is a key after hours of
  silence, not a connect, and the app spends that edge on an AFK
  indicator and nothing else. `/summary` answers "what did I miss" well
  but is pull, and the people it would help most (the ones who were gone)
  are the ones who do not know to run it. The Late Edition owns the
  connect side of the moment (shown 98 times a day); the keypress side is
  unowned.
- **#lounge's system feed has no room-level limit.** Every approved event
  posts, guarded only by a per-person 30-minute repeat window; ten people
  doing ten things in an hour is ten lines, all compliant. It holds
  because events are rare, and rare is what makes a line a story: one of
  five things that happened today lands, one of eighty is log output the
  room learns to skim. Any feature that multiplies event volume needs the
  valve in `devdocs/FEED-BUDGET.md` first.
- **Reading surfaces are glanced at, not sought.** The paper is shown 98
  times a day and opened on purpose 7 times. The News reader opens 3
  times a day, Calendars hold a visitor for under a minute, Discover 14
  hours in three weeks. These are fine as ambient texture; none of them
  is a reason to come back.
- **The chip economy is deflated.** Pot tickets (42 a day) are the only
  live sink; gilds and rounds are dead. People amassed fortunes with
  nothing to want. A new mechanic must be the sink, not another faucet.
- **The public web is the gallery and the profiles.** 8,000 real views in
  25 days against 147 on `/listen`. Nobody listens from the browser; they
  come to look at what people made and who they are.

## 5. Ideas

Live, in order of evidence behind them:

- **The first hour, before anything else: a first session built around
  the daily board.** The funnel says the next ritual, surface or launch
  post lands on people who are gone by day two, and the day-one reads
  (section 3) say what holds them: a finished daily, a quest ticked, a
  reason named for tomorrow. Built as the tour's `Board` route, beside
  the old `House` one and one line to revert
  (`late-ssh/src/app/clubhouse/CONTEXT.md` §5): four stops instead of
  18 (what this is, the chat, the pool break, today's board); today's
  Netwalk daily as the play the route ends on, played for real and won
  before the tour lets go, in place of the scripted fight; no Stations
  stop on plain ssh, where there is nothing to hear; the solved board's
  header as the send-off, naming tomorrow (a new board at 00:00 UTC with
  the hours left), and the chat as where the tour lands. Measure it on the next cohort
  with the queries in section 6. Targets: newcomers who win a daily on
  day one from 7% to 25% or more, two-day return from 5% to 9%, tour
  completion from 46% to 80%. One cohort to read. The steps, the ladder
  that follows and the post after that: `devdocs/PLAN-FIRST-HOUR.md`.
  - **A ladder on the dailies (the Duolingo shape).** Half of it is
    built: daily and weekly quests, a streak with a chip bonus, a strip
    at the top of the Arcade (`late-ssh/src/app/hub/CONTEXT.md`). What
    Duolingo has that the strip does not: the streak is the first thing
    you see and the thing you are warned about losing, others can see
    it, the reward is standing rather than currency, and day one ends
    inside it. The gap is visibility and the day-one handshake, not more
    quest kinds: the streak on the profile and beside the name in the
    Arcade rosters, the first session's daily starting it, the
    keypress-return moment (section 4) saying what it is at. Chips cannot
    be the reward; the median balance is the stipend. Unmeasured:
    whether a visible streak moves the 85 weekly players or only
    decorates them.
- **Daily boards are the proven ritual.** The Arcade's daily is the floor
  deadchannel's ration loop is built to copy. More of that shape (one
  shared object a day, finished in minutes, talked about in #lounge) beats
  more destinations.
- **Reward connected time.** A Dark Room already credits village time
  only while the session is attached. A deadchannel beat that only a
  connected runner can take (a spawn, a crossing landing while you are on
  the wire) raises hours left open without a new surface.
- **The front door is on the web; what is left is to post.** The paper
  is public at `/paper`, with dated back issues and an RSS feed (the
  announcements, WHAT WE WERE READING and OUTSIDE; room pages stay in the
  clubhouse), profile contact and links carry `rel="me"` so a member's
  Mastodon profile shows late.sh verified, and every page unfurls into a
  preview card: a profile into its runner's poster, a gallery link into
  its snapshot, both drawn server-side as PNG (`late-web/CONTEXT.md`).
  The paper opens with YESTERDAY ON LATE.SH, the covered day in numbers
  (the radio's top stations by paired listener, the tracks queued, the
  games finished in the arcade, the Lobby's matches started and
  finished, the doors opened), drawn
  from the rows and no model
  (`late-ssh/src/app/paper/CONTEXT.md`): the one paper section that
  answers a newcomer's "is anyone here?", on the web and on the herald's
  paper card too. Unmeasured: whether the paper's on-purpose opens (7 a
  day) move; nothing else here is measured beyond page views. Post, do
  not federate: a
  bot account on an existing instance is
  one bearer token and two HTTP calls; federating profiles (ActivityPub,
  WebFinger, signed fetches, inbound moderation) is weeks of work and
  sends the visibility people want outward, away from the terminal.
- **The herald: two posts a day on the Fediverse and Bluesky.** Built
  (`late-ssh/src/app/herald/CONTEXT.md`) and live: both accounts post,
  and the `herald_settings` row is on in prod (`/herald off` stops it).
  Each post is delivered to every network the herald has an account
  on. The
  Fediverse: an ordinary account on an existing Mastodon instance
  (fosstodon.org: FOSS-leaning, a local timeline that gives a new account
  readers on day one; invite-only, and its rule for automated accounts
  is that an active member runs them and answers replies), flagged as
  automated, `https://late.sh` in its profile links, and an application
  token with `write:media` and `write:statuses` only. The Fediverse is
  every server speaking ActivityPub (Mastodon, Pixelfed, PeerTube, Lemmy,
  Threads in part); one Mastodon account is addressable from all of it.
  Bluesky: a different network, so its own account, flagged with the
  `bot` self-label, posting through an app password; it asks that a bot
  never likes, reposts or replies unprompted, so the human half there is
  done from a person's own account.
  - **What it posts.** Two slots a day, five hours apart, each with its
    own kinds. Words at 15:00 UTC: the operator's word first, a day whose
    edition carries `#announcements` posts going out as words alone, the
    note compressed by the model into one post that links the edition for
    the note in full, whatever the weekday. Else by the week: a member's
    project on Monday, moving along to Friday if the operator's word or a
    failed write-up bumped it; the Late Edition's front page as a card on
    Wednesday and Saturday (the most played arcade game with its ASCII
    picture, the day in numbers, then the heads of the prose columns;
    linking the edition), and on a Monday with no project; on Sunday the
    week in every game (a scoreboard card, one line per game anyone played
    with everyone's plays added up and no name: the arcade's daily puzzles
    solved and score runs finished, the Lobby's chess, pool and card games
    by matches finished, the doors by every hub card's openings), else the
    paper. Tuesday, Thursday and Friday post no
    words with nothing owed and nothing announced. Art at 20:00 UTC: on
    Monday yesterday's Artboard snapshot cropped to what is drawn, once it
    has enough ink to be a picture (linking the gallery); every other day
    the piece over the door, the splash wall's piece of the day (credited
    `~name`, linking the piece's page; the wall is one queue in hang order,
    so the post and the login screen show the same piece). One candidate a
    day and no fallback: a bare board, an empty wall or a picture the look
    did not clear is no art that day. The wall moves on every day whatever
    the herald posts, so Monday's piece hangs over the door unposted, and
    no piece is posted twice. Alt text and the kind's tags. The first
    design alternated the board with members' PRINTS posters, each once;
    dropped before launch, since a few new posters a month cannot carry a
    slot.
  - **A member's project.** One a week, drawn at random from the projects
    of members who logged in in the last 7 days, none posted in the last 60
    days. A grounded model call reads the project's page (Google fetches
    it, no server of ours ever fetches a URL a member typed) and writes one
    or two plain sentences, or turns it down as commercial (Fosstodon bans
    commercial promotion), unfit or unclear, and the next one drawn is
    tried, three at most. With none written, or the model down, no project
    goes out that day: a member's words never reach the official account
    unchecked. Sharing a project is a public act the way hanging a piece
    is, so it earns its `~name`, with its own link the one address the
    herald carries that is not late.sh's.
  - **The tags.** Four at most, two that bring readers and two that
    bring members, chosen per kind so a tag never sits on a post its
    followers did not come for (a mismatched tag is how an account gets
    muted). The art tags (#ASCIIart, the one people follow even though
    the board is colour and Unicode, not strict ASCII; #MastoArt) bring
    views; #tildeverse (pubnix people, the likeliest to run `ssh late.sh`)
    and #terminal (terminal culture; #ssh was dropped, it is sysadmin and
    security talk) bring members. Per kind: the board and gallery pieces
    `#ASCIIart #MastoArt #tildeverse #terminal`;
    the weekly scoreboard `#puzzles #sudoku #tildeverse #terminal`; the paper
    `#tildeverse #terminal #TUI #smolweb`, never #MastoArt; an announcement
    `#tildeverse #terminal #TUI`, no art tags on a post with no art; a
    project `#tildeverse #terminal`, since no subject tag fits everything a
    member makes.
    Candidates not taken: #ANSIart (the BBS and coloured-terminal
    tradition, a smaller crowd), #TextArt. Unmeasured: compare recent posters on each
    tag's page on the instance before changing a set.
  - **The kinds.** Built (`late-ssh/src/app/herald/CONTEXT.md`, "The
    post"). Each picture kind is one PNG route on the front door
    (late-web's piece page and PNG, the week's games card, the paper's
    front-page card, all through `late-web/src/pages/shared/board_png.rs`),
    one candidate read, one text, and one variant of the herald's
    picture, which breaks the build until every match knows it; the
    announcement and the project have no route, words alone. A piece is the
    best supply of art the server has: a real picture a member made and
    chose to hang, immutable, credited, with applause. The scoreboard is
    the one kind that shows the whole shelf of games, weekly so it never
    reads as a daily brag feed, and nameless so it is never a brag at
    all: hanging a piece is a public act and earns its `~name`, winning
    Sudoku is not, and nobody opted into being posted; the names stay on
    the terminal's boards. The paper's post text names the edition
    only and drops `#MastoArt` (art communities dislike model-written
    text); its columns stay inside the picture, never in the text, since
    a list of links is what instance rules call spam. The cards are drawn
    from late.sh's own rows and never shown to the model. Ruled out: room
    summaries (a model's account of what people said in a room they took
    for a clubhouse, pointed outward) and a card of new profile slugs
    (dull, a handful a month). Further out, each behind a PNG route:
    share cards with the `ssh late.sh` footer, a frame of the tavern or
    the night city, the rare headline events the #lounge feed treats as
    stories. How many pieces wait is the wall's queue depth
    (`late_ssh_artboard_gallery_splash_queue_depth`), which decides how
    many art slots stay quiet.
  - **What it never posts.** Chat. A frame of #lounge is people's
    messages on the public web; the Artboard, posters and cards are
    things people made to be seen, messages are not.
  - **The rhythm.** Two posts a day at most, five hours apart, never
    three. A muted account never recovers, so the second slot came with
    fewer kinds in it, not more: the paper stayed at two days a week, some
    days post art alone, and art goes out clean or not at all. Whether two
    is too many is unmeasured; watch the follower count and mutes.
  - **The shape.** A late-ssh domain built like the paper's press: a
    checker on every replica, a `herald_posts` row per slot of a UTC day
    claimed by conditional insert so several replicas post exactly once, the picture
    picked once and stored on the row, fetched from the front door's PNG
    routes over the cluster service, then one delivery per network
    recorded as it lands (a retry sends only to the networks still
    missing). On Mastodon the status call carries the row's id as its
    idempotency key; on Bluesky the record key is derived from it, so a
    retry never double-posts on either. Counters
    `late_ssh_herald_posts_total` (posted, skipped, lost, failed) and
    `late_ssh_herald_deliveries_total` by network, on the Grafana board's
    Herald row. Going live on a network is a profile change: pin the
    instance or the handle, require its secret, ship the `-infra` release
    that injects it before the ssh image that needs it. Dev posts only
    with a network's pair in `.env.local`, unlisted on Mastodon (Bluesky
    has no unlisted).
  - **The human half.** The account follows a few people, answers
    replies, and boosts members who post their own profile or card. A
    bot that only broadcasts gets muted; an account that posts twice a
    day and otherwise behaves like a person gets followed.
  - **When.** It is an arrivals lever, and the funnel in section 3 says
    arrivals leak at 95% on day two. The row is on; two posts a day to a
    few followers is the account being built, not a launch. The posts
    that bring a crowd wait for the first hour, so the first thing new
    followers find is a place that holds them. Not measured yet beyond
    the counter.
  - **What is left.** The herald is live and documented in
    `late-ssh/src/app/herald/CONTEXT.md` (the post, the switches, the
    account setup by hand, the telemetry); its two secrets went out
    under the deploy skew rule in `infra/RUNBOOK.md`. The slot
    constraint is under the same rule: an old pod still running against
    a changed `herald_posts` constraint fails every claim, logging
    `failed to claim the herald's day` every five minutes until it
    drains, and loses nothing once it has. Bluesky's
    automation label is on (Settings, "Show automation label"), and the
    landing page links both accounts with `rel="me"`, which is what makes
    the Mastodon profile's `https://late.sh` link show verified. Open:
    the human half above, daily.
  - **The look: alt text and the gate, before every picture a member
    drew.** Built (`late-ssh/src/app/herald/CONTEXT.md`, "The look"),
    from a Fediverse member's note on the first post: describe every
    image, and keep nudity off the timeline. One Gemini vision call at pick
    time writes the alt text and picks one warning from a closed set, for
    the board and pieces. Anything flagged stays off every network and the
    slot rests for the day, and so does a picture the call could not check.
    Removal of a post already out stays a hand job from the account.
    - **What the hand run measured.** The gallery's 16 archived boards
      (7 dailies, 6 monthlies, 3 curated, on 2026-10-25), each asked
      three to five times. The descriptions are good: positioned,
      specific, no member names; one poster was named wrongly (Austin
      Powers as Rick Astley). The first prompt warned on 15 of 16
      boards, set by small words ("the art board needs boobs") and
      possibly by a pride flag; the prompt now labels what is drawn,
      never words or identity. The whole board alone missed a drawn
      penis on the 18 and 19 October dailies in every run; with the
      four quarters beside it the model found it in every run, for
      about 6,000 input tokens a call. A 4,096-token cap left 7 of 48
      answers empty (thinking spent the budget); at 16,384, none of 32.
      Labels still drift between runs on the same board (nudity, then
      suggestive). Every daily from 18 to 24 October carries a warning:
      a nude cyborg figure stays on the live board until the monthly
      wipe.
    - **Instance rules.** Fosstodon's rules, read 2026-10-25 from its
      API and its code of conduct (`hub.fosstodon.org/coc`): "DO NOT
      post adult or age-restricted content. This includes nudity and
      other sexualized or violent content", with no exception for a
      content warning, and no slurs. Every warning the look gives runs
      into one of them, which is why a flagged picture stays off and is
      not posted behind a mark. The boards the hand run marked for 18 to
      24 October were posted there before any of this; check the account
      for them.
- **Launch posts follow the first hour, not the deadchannel flip.** The
  Hacker News front page in July 2026 peaked at about 100 sessions and
  settled back to the same 40; a post before something holds people
  repeats that, and the lifetime yield today is about one regular per
  hundred signups. The flip (`GAME.md`, "The three surfaces") is the
  game's own launch and waits on runners; runners wait on bios; bios, by
  design, wait on the flip. Gating the posts on it as well gates them on
  a loop. Post once the next cohort's two-day return has moved; the flip
  comes when the game is ready, at its own pace.
- **Progress Quest** is the cheap next door on the incremental shelf, the
  only door shape that holds people (`devdocs/DOOR-CANDIDATES.md`).

Parked, with the reason:

- **Events and tournaments as a pillar** (the former SOCIAL.md): a
  coordination machine for a population that cannot fill one. The seeded
  score window is the first piece to un-park once the character layer
  gives people a reason to show up daily.
- **A space trade and fight MMO**: the most liquidity-hungry genre there
  is. A fine idea for a late.sh with 500 concurrents.
- **TradeWars as an always-on universe**: parked until it can run as a
  season (`devdocs/DOOR-CANDIDATES.md`).
- **The feed budget** (`devdocs/FEED-BUDGET.md`): events persisted first,
  one lines-per-hour dial on #lounge's system feed. Parked until the
  deadchannel lounge theater goes public, the first thing that will
  multiply event volume.

What to watch for deadchannel, from the counters in
`late-ssh/src/app/deadchannel/CONTEXT.md`, Gotchas: runners created
against gate passes (the funnel), roads started per day against runners
(the ritual), lounge hits against spawns landed (does the room notice),
and `#deadchannel` hours inside Home (does anyone sit on the wire). The
16-day read is in section 3: the ritual and the wire hold, the room
notices flickers and misses howlers, and the funnel's ceiling is the
eligible pool. The gates stay as designed; the game is the prestige
track. The arena and bets stay unbuilt at two or three runners online at
once (the coordination rule above); the first marks, due within days, are
the next thing the game itself has to show.

## 6. Measuring

Port-forward the monitoring services, then query. The attention counters
reset on every deploy; `increase()` over a window handles it.

```bash
kubectl port-forward -n monitoring svc/victoriametrics 18428:8428 &
kubectl port-forward -n monitoring svc/victorialogs 19428:9428 &
# hours by screen, last 25 days
curl -s --data-urlencode 'query=sum by (screen) (increase(late_ssh_attention_seconds_total[25d]))/3600' \
  http://127.0.0.1:18428/api/v1/query
# active hours by place inside Home
curl -s --data-urlencode 'query=sum by (place) (increase(late_ssh_attention_seconds_total{screen="dashboard",presence="active"}[25d]))/3600' \
  http://127.0.0.1:18428/api/v1/query
# arrivals per screen
curl -s --data-urlencode 'query=sum by (screen,place) (increase(late_ssh_place_visits_total[25d]))' \
  http://127.0.0.1:18428/api/v1/query
# the first session: day-one active hours by screen
curl -s --data-urlencode 'query=sum by (screen) (increase(late_ssh_attention_seconds_total{tenure="day_one",presence="active"}[7d]))/3600' \
  http://127.0.0.1:18428/api/v1/query
# where the tour loses people: stops reached, the drop between two is the leak
curl -s --data-urlencode 'query=sum by (stop) (increase(late_ssh_tour_stops_total[7d]))' \
  http://127.0.0.1:18428/api/v1/query
# distinct people per day, and sessions per person (logs, 7 days)
curl -s http://127.0.0.1:19428/select/logsql/query \
  --data-urlencode 'query=_time:7d "user connected" | stats by (_time:1d) count_uniq(username) u'
curl -s http://127.0.0.1:19428/select/logsql/query \
  --data-urlencode 'query=_time:7d "user connected" | stats by (username) count() n' --data-urlencode 'limit=1000'
# session lengths (summaries are written for a subset of sessions)
curl -s http://127.0.0.1:19428/select/logsql/query \
  --data-urlencode 'query=_time:7d "session wire summary" | stats by (client) count() n, quantile(0.5, secs) p50, quantile(0.9, secs) p90'
# the per-person client split: join this on user_id to user_online_time
curl -s http://127.0.0.1:19428/select/logsql/query \
  --data-urlencode 'query=_time:7d "session wire summary" | stats by (user_id, client) count() n, sum(secs) secs' --data-urlencode 'limit=100000'
```

The newcomer funnel comes from the database, not the logs: the referral
clock (`newcomer_activity_days`, `late-ssh/src/app/referral/CONTEXT.md`)
records every account's first 60 days, invited or not, one row per UTC day
with keyboard input, from 2026-10-02 on. Of the accounts created since the
clock started and at least a week ago, how many came back on one, three
and five separate days with 20 or more active minutes inside their first
week (read-only, through `scripts/connect_db.sh`; widen the window to 14
days once the table is a month older):

```sql
WITH cohort AS (
  SELECT id, created::date AS joined FROM users
  WHERE created >= '2026-10-02'
    AND created <  now() - interval '7 days'
), days AS (
  SELECT c.id,
         count(*) FILTER (WHERE a.active_minutes >= 20) AS good_days
  FROM cohort c
  LEFT JOIN newcomer_activity_days a
    ON a.user_id = c.id AND a.day > c.joined AND a.day <= c.joined + 7
  GROUP BY c.id
)
SELECT count(*) AS accounts,
       count(*) FILTER (WHERE good_days >= 1) AS back_once,
       count(*) FILTER (WHERE good_days >= 3) AS back_3_days,
       count(*) FILTER (WHERE good_days >= 5) AS back_5_days
FROM days;
```

The day-one reads in section 3 come from the same table. Depth on day one
against first-week return; for the action table swap the `m1` buckets for
a day-one fact (a `chip_ledger` row with reason `daily_puzzle_win`, a
`chat_messages` row, a `user_quest_progress` row completed on the join
day):

```sql
WITH cohort AS (
  SELECT id, created::date AS joined FROM users
  WHERE created >= '2026-10-02' AND created < now() - interval '7 days'
), d1 AS (
  SELECT c.id, coalesce(a.active_minutes, 0) AS m1
  FROM cohort c
  LEFT JOIN newcomer_activity_days a ON a.user_id = c.id AND a.day = c.joined
), ret AS (
  SELECT c.id,
         count(*) FILTER (WHERE a.day > c.joined AND a.day <= c.joined + 7) AS back_days
  FROM cohort c
  LEFT JOIN newcomer_activity_days a ON a.user_id = c.id
  GROUP BY c.id
)
SELECT CASE WHEN m1 = 0 THEN '0' WHEN m1 < 5 THEN '1-4' WHEN m1 < 20 THEN '5-19'
            WHEN m1 < 60 THEN '20-59' ELSE '60+' END AS day_one_minutes,
       count(*) AS accounts,
       count(*) FILTER (WHERE back_days >= 1) AS typed_again,
       count(*) FILTER (WHERE back_days >= 2) AS two_days
FROM d1 JOIN ret USING (id)
GROUP BY 1 ORDER BY min(m1);
```

The quest strip: completions and people per week, and the streaks alive:

```sql
SELECT date_trunc('week', p.completed_at)::date AS week, a.cadence,
       count(*) AS done, count(DISTINCT p.user_id) AS people
FROM user_quest_progress p JOIN quest_assignments a ON a.id = p.assignment_id
WHERE p.completed_at >= now() - interval '42 days'
GROUP BY 1, 2 ORDER BY 1, 2;
SELECT consecutive_days, count(*) FROM user_daily_quest_streaks
WHERE last_completed_date >= current_date - 1 GROUP BY 1 ORDER BY 1;
```

deadchannel's eligible pool: the people seen this week with the hours
and the bio the gate asks for (`haunt/state.rs` holds the thresholds),
and how many already hold a runner row:

```sql
SELECT count(*) FILTER (WHERE hours >= 8) AS with_hours,
       count(*) FILTER (WHERE hours >= 8 AND bio >= 100) AS eligible,
       count(*) FILTER (WHERE hours >= 8 AND bio >= 100 AND r.user_id IS NOT NULL) AS invited
FROM (
  SELECT u.id, coalesce(t.total_milliseconds, 0) / 3600000.0 AS hours,
         length(coalesce(u.settings->>'bio', '')) AS bio
  FROM users u
  LEFT JOIN user_online_time t ON t.user_id = u.id
  WHERE u.last_seen >= now() - interval '7 days'
) p
LEFT JOIN deadchannel_runners r ON r.user_id = p.id;
```

Gaps in what the telemetry can answer, by design (no per-person metric
labels, so the series count cannot grow with the crowd):

- How many people, not how many actions, use the bonsai, the pet, the
  aquarium, the Arcade or the doors. Needs `scripts/connect_db.sh` and
  read-only queries: distinct `user_id` in `bonsai_trees` by
  `last_watered`, in `pet_companions` by `last_petted`, in
  `user_aquarium_care` by `last_fed`; `user_online_time` as the lifetime
  distribution (`total_milliseconds` per `user_id`); `chat_messages` by
  `room_id` and distinct `user_id` per day; `deadchannel_runners` rows
  with `left_at` null. The doors are the exception: `door_openings` holds
  one row per Enter on a hub card (`late-ssh/src/app/door/CONTEXT.md`),
  so distinct `user_id` per `card` per day is one query, and the counts
  alone are on the Grafana Games row ("Doors Opened by Card") and in the
  paper's YESTERDAY line, every card.
- What a newcomer does on day one, per person. The counters split it by
  tenure (`day_one`, `first_week`, `regular` on the attention, visit and
  session-start series), count each first-visit tour stop reached, the
  welcome box included (`late_ssh_tour_stops_total{stop}`; `/onboard`
  reruns are left out) and the screen that was up when a
  session ended (`late_ssh_session_ends_total{tenure,screen}`), read on
  the Grafana "Newcomers" row. What they cannot say is whether twenty
  active minutes were one person or twenty; that is the funnel above and
  stays a database question.
- Voice room usage: no counter.
- Logs keep 7 days, so any per-person trend longer than a week comes from
  the database or from the weekly leaderboards.
