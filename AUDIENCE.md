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
| Concurrent sessions, weekly average | 15, 21, 20, 23, 22 over the five weeks ending 2026-10-22 |
| Concurrent sessions, 90-day peak | 43 |
| Distinct people connecting per day | 85 to 135 |
| Distinct people connecting per week | 371 |
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

## 2. Where the time goes

Attention is counted once a second per session, split by screen and by
place inside it, and marked active (a key within the recent window) or
idle. 2026-09-27 to 2026-10-22, about 13,900 session-hours in all.
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
cyberspace 85 h, DMs 71 h, #deadchannel 57 h before its switch,
Notifications 16 h, Discover 14 h, Feeds 4 h. Active time keeps the same
order: #lounge 561 h, topic rooms 134 h, News 20 h, DMs 10 h.

**Inside the Arcade** (hours on a board): Solitaire 270, Sudoku 190,
Nonogram 173, 2048 123, Le Word 89, Minesweeper 85, Tetris 72, Sliding
Puzzle 66, Rubik's Cube 14, Snake 10, Traffic 4. Boards finished per day:
160 to 285, climbing through October; 94% of finishes are the shared daily
board, 6% a personal one. By finishes Minesweeper leads (1,175), then
Sliding Puzzle, Le Word, Nonogram, Sudoku, Solitaire.

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

Web, 25 days: the landing page 535,000 views (bot traffic; treat as
noise), gallery 4,400, profile pages 3,700 plus 1,400 index views, terms
764, `/listen` 147, a live stream page 136.

**The newcomer funnel**, from the referral clock (section 6): the 298
accounts created 2026-10-02 to 2026-10-15, about 21 a day, followed for
their first week. An active minute is a UTC minute with keyboard input.

| | Accounts | Share |
|---|---|---|
| Typed anything on day one | 276 | 93% |
| 5 or more active minutes on day one | 156 | 52% |
| 20 or more active minutes on day one | 47 | 16% |
| Typed anything on days 2 to 8 | 42 | 14% |
| Typed on two or more of days 2 to 8 | 14 | 5% |
| One day of 20 or more active minutes in days 2 to 8 | 17 | 6% |
| Three such days | 7 | 2% |

Median active minutes in the week after day one: 0. The 90th percentile:
2. The per-day numbers are flat across the two weeks (0 to 3 returners a
day out of 9 to 39 signups), so this is the shape, not a bad week.

**deadchannel**, switched on 2026-10-21. In 25 days the first-contact gate
was evaluated 1,154 times at connect: 800 passes, 174 bios too short, 159
too few hours, 8 too few settings (counts per connect, not per person).
13 bios screened, all passed. Beats delivered: 137 clock glitches, 3 name
flickers, 1 breakthrough, 1 runner created. Fights: 207 roads started,
2,324 rounds, 231 wins, 11 losses, 2 crossings (staff and testers before
the switch). Lounge spawns: 29 landed, 8 hit, 4 put down, 25 slipped
away. In the two days since the switch: 9 spawns landed, 2 put down, 82
fight commands. Too early to read; what to watch is in section 5.

## 4. What the numbers say

- **The first session is the leak.** Nine in ten newcomers type on day
  one and half of them stay five minutes, then 86% never press a key
  again and 94% never come back for a second day. The regulars were not
  made by the current first session either: 298 signups in two weeks
  produced 7 people with three active days. Arrivals are not the lever
  until that changes; a launch post today pours 100 people into a 94%
  leak, which is what July's Hacker News spike measured. The lever is
  the first hour: what a newcomer sees, is handed, and has a reason to
  return to tomorrow.
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

- **The first hour, before anything else.** The funnel above says the
  next ritual, surface or launch post lands on people who are gone by
  day two. What a newcomer is handed on day one (the tour, the paper, an
  invitation to come back tomorrow: a daily board started, a runner
  promised, a seat kept) is the unmeasured half of this doc; measure the
  first session by screen with the attention counter filtered to new
  accounts before designing it.
- **Daily boards are the proven ritual.** The Arcade's daily is the floor
  deadchannel's ration loop is built to copy. More of that shape (one
  shared object a day, finished in minutes, talked about in #lounge) beats
  more destinations.
- **Reward connected time.** A Dark Room already credits village time
  only while the session is attached. A deadchannel beat that only a
  connected runner can take (a spawn, a crossing landing while you are on
  the wire) raises hours left open without a new surface.
- **Put a front door on the web, then post.** A public paper page and
  its feed, `rel="me"` on profile links so a member's Mastodon profile
  shows late.sh verified, and preview cards that unfurl a profile into
  its poster and a gallery link into its snapshot, drawn server-side as
  PNG. Post, do not federate: a bot account on an existing instance is
  one bearer token and two HTTP calls; federating profiles (ActivityPub,
  WebFinger, signed fetches, inbound moderation) is weeks of work and
  sends the visibility people want outward, away from the terminal.
- **The herald: one picture a day on the Fediverse.** An ordinary account
  on an existing Mastodon instance (fosstodon.org: FOSS-leaning, a local
  timeline that gives a new account readers on day one, bots welcome
  when marked), flagged as automated, `https://late.sh` in its profile
  links, and an application token with `write:media` and
  `write:statuses` only. The Fediverse is every server speaking
  ActivityPub (Mastodon, Pixelfed, PeerTube, Lemmy, Threads in part);
  one Mastodon account is addressable from all of it. Bluesky is a
  different network and would be a second client, later.
  - **What it posts.** One picture a day, rotating so the slot never
    repeats: yesterday's Artboard snapshot (tagged #ASCIIart and
    #MastoArt, the communities that boost strangers' art), a runner's
    poster the day it is painted at PRINTS with the member's handle, the
    day's best share card rendered as a picture with the `ssh late.sh`
    footer, a frame of the tavern or the night city. Each with alt text,
    a link to that day's `/paper` or the gallery, and the same four tags
    (#tildeverse #pubnix #ssh #terminal). The rare headline events the
    #lounge feed already treats as stories (the pot drawn, the crown
    taken, the monthly awards) may take the slot. Every picture comes
    through the front door's cell renderer, so anything the terminal
    draws can be posted.
  - **What it never posts.** Chat. A frame of #lounge is people's
    messages on the public web; the Artboard, posters and cards are
    things people made to be seen, messages are not.
  - **The rhythm.** One post a day, at most two when both are pictures.
    A muted account never recovers; the variety above is so one slot
    never repeats, not so there are more slots.
  - **The shape.** A small late-ssh domain built like the paper's press:
    a sweeper on every replica, a `herald_posts` row per UTC day claimed
    by conditional insert so several replicas post exactly once, the
    picture fetched from the front door's image routes over `web_url`, a
    media upload then a status call with an idempotency key so a retry
    never double-posts, a `posted / skipped / failed` counter on the
    Grafana board. Config off in dev and required in prod, so the first
    prod deploy after it merges needs the token secret in place (wired
    like `LATE_AI_API_KEY` in `infra/service-ssh.tf`) or the service
    refuses to start.
  - **The human half.** The account follows a few people, answers
    replies, and boosts members who post their own profile or card. A
    bot that only broadcasts gets muted; an account that posts once a
    day and otherwise behaves like a person gets followed.
  - **When.** It is an arrivals lever, and the funnel in section 3 says
    arrivals leak at 94% on day two. Build it, flip it on once the first
    hour has moved, so the first thing new followers find is a place
    that holds them.
- **Launch posts wait for the deadchannel public flip** (`GAME.md`, "The
  three surfaces"), which is the game's public launch. The Hacker News
  front page in July 2026 peaked at about 100 sessions and settled back to
  the same 40; a post before something holds people repeats that.
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
and `#deadchannel` hours inside Home (does anyone sit on the wire).

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
# distinct people per day, and sessions per person (logs, 7 days)
curl -s http://127.0.0.1:19428/select/logsql/query \
  --data-urlencode 'query=_time:7d "user connected" | stats by (_time:1d) count_uniq(username) u'
curl -s http://127.0.0.1:19428/select/logsql/query \
  --data-urlencode 'query=_time:7d "user connected" | stats by (username) count() n' --data-urlencode 'limit=1000'
# session lengths (summaries are written for a subset of sessions)
curl -s http://127.0.0.1:19428/select/logsql/query \
  --data-urlencode 'query=_time:7d "session wire summary" | stats by (client) count() n, quantile(0.5, secs) p50, quantile(0.9, secs) p90'
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

Gaps in what the telemetry can answer, by design (no per-person metric
labels, so the series count cannot grow with the crowd):

- How many people, not how many actions, use the bonsai, the pet, the
  aquarium, the Arcade or the doors. Needs `scripts/connect_db.sh` and
  read-only queries: distinct `user_id` in `bonsai_trees` by
  `last_watered`, in `pet_companions` by `last_petted`, in
  `user_aquarium_care` by `last_fed`; `user_online_time` as the lifetime
  distribution (`total_milliseconds` per `user_id`); `chat_messages` by
  `room_id` and distinct `user_id` per day; `deadchannel_runners` rows
  with `left_at` null.
- What a newcomer does on day one: no counter tells a newcomer from a
  regular. Planned: a `tenure` label (`day_one`, `first_week`,
  `regular`) on the engagement counters, a count of each first-visit
  tour stop reached, and the screen that was up when a session ended,
  on a Grafana "Newcomers" row. Until then the funnel above is the
  database's answer.
- Voice room usage: no counter.
- Logs keep 7 days, so any per-person trend longer than a week comes from
  the database or from the weekly leaderboards.
