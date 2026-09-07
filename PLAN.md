# Plan: Spectate and Bet, and the Share Loop

Two growth bets, picked 2026-09-05. Both bring people from outside to a link,
and both keep the SSH filter exactly where it is: the web is a window, never a
door. Everything that writes (chat, bets, moves) still requires `ssh late.sh`.

Status: proposal. Nothing here is built. Decisions marked OPEN need Mat.

---

## Why these two

- **Spectate and bet** turns the roguelike doors into events. Watching other
  people's runs is the loop that kept nethack.alt.org and the DCSS WebTiles
  servers alive for twenty years. Chips on the outcome make every serious run
  something you ping a friend about, and it is a chip sink the economy needs.
- **The share loop** is the only mechanic on the list with a proven K-factor.
  Wordle grew on a spoiler-free grid and nothing else. Every daily we run can
  emit one, and every correspondence game can be an invite.

They share one piece: a public "how to get in" page that every outside link
lands on. That page is the onboarding session, and the answer to "should we
build a web terminal". We should not. See §C.

---

## A. Spectate and bet

### A.1 User-facing behavior

- Any live roguelike run (DCSS, NetHack, Brogue first; Usurper later) can be
  watched by anyone, in the TUI and on a public web page, as a live terminal at
  text bandwidth. Watchers never send input.
- Watching is on by default, like every public crawl server. `/spectate off`
  hides your runs. The player sees the watcher count in the door's top bar.
- Each watched run has chat beside it. The room is the player's existing
  permanent public stream room (`{username}-live`, `kind='game'`,
  `game_kind='stream'`, created by `get_or_create_stream_room`). One room per
  person for everything they broadcast, screen share or dungeon.
- A "Live dungeons" list: in the Games hub (page `3`) above the cards, in the
  Lobby modal, and as `/watch` with no argument. `/watch @user` already exists
  for screen shares and gains the dungeon case.
- Bets: while a run is live, anyone but the player puts chips on how it ends.
  Parimutuel, copied from the pot: the pool pays the winning bracket, 20% burns.
  Bets close when the run ends. The result comes from the door log pipe, never
  from the screen, so nothing on the watch surface can be spoofed into a payout.
- The run's end posts a #lounge line that names the pool size and the biggest
  winner, through the existing activity feed shape.

### A.2 Architecture

**The mirror lives in the door host, not in the session.** Every network door
host (`late-dcss`, `late-nethack`, `late-brogue`, `late-usurper`) already owns
the PTY: one russh server, one child per SSH session, bytes bridged to the
late-ssh client which feeds a `vt100::Parser`. The host is one process per
door, so a mirror there is replica-clean by construction (multi-replica rule,
root `CONTEXT.md` §0), where a tee inside the late-ssh session would be
another row in the §7 debt table.

- Host: per child, keep a ring buffer of the last N KB of output (enough for a
  full repaint; OPEN: size, 64 KB is a starting guess) plus a `broadcast` of
  new bytes. Also record the child's current PTY size, since a watcher's parser
  must be sized like the player's.
- Host: a reserved watch username in the `late_*` namespace (`late_watch`, the
  same trick as `late_stats`), env request `LATE_WATCH_PLAYNAME=<handle>`. The
  host replies with the PTY size, replays the ring buffer, then streams live
  bytes. Input from a watch channel is dropped on the floor. Auth is the same
  shared-secret key the player client uses; only late-ssh can open it.
- Host: a presence frame on the existing `late_stats` stream
  (`presence\t<handle>\tstart|stop`) so the ingest task in late-ssh learns who
  is live without polling. OPEN: whether "live" is a host-memory fact (dies
  with the host, fine) or a `door_live_runs` row (survives, needs a sweeper).
  Host memory first, since the host already is the truth for "has a child".
- late-ssh TUI watcher: a `WatchProcess` beside `DcssProcess`, same russh
  client, same `vt100::Parser` and `blit_screen` path, minus input forwarding.
  Screen `Screen::Watch { door, handle }`, reached from the live list; `Esc`
  leaves, backtick cycles past it like any live door.
- Web watcher: `GET /api/watch/{door}/{handle}` in `late-ssh/src/api.rs` is a
  WebSocket that opens the same `late_watch` client and relays bytes to an
  xterm.js page in late-web at `/watch/{handle}`. Read-only, no auth, public.
  Bandwidth is the PTY diff, kilobytes per minute, so fan-out is cheap where
  the screen-share stream is not (`SCALE.md` Pain Point 8 does not apply).
- The page is a share link: `late.sh/watch/mat`, with the room chat rendered
  read-only beside the terminal and the "how to get in" strip below (§C).

**Bets are rows, settled by the log pipe.**

- Tables: `door_bet_pools` (one per live run: door, handle, `run_key`, status
  `open`/`settled`/`void`, settled outcome, burn and payout totals written once
  at settlement) and `door_bets` (pool, user, bracket, chips; never updated).
  CHECK forbids the player betting on their own pool, the artboard applause
  shape. Model in `late-core/src/models/door_bet.rs`, all queries there.
- `run_key` is the problem to get right. Runs resume across sessions (saves),
  and the xlog line only exists at the end. OPEN: key on the run's start
  milestone (DCSS `milestones` has a `begin` line, NetHack `xlogfile` has
  `starttime`, Brogue has its own), which the pipe already parses; a pool opens
  on `begin` and settles on the matching `door_runs` row. Brackets per door
  come from fields the pipe already lands (`DoorRunResult`, depth, runes).
- Markets: one pool per run over a closed enum of brackets, per door. Draft:
  DCSS `dies before 3 runes` / `3+ runes, no Orb` / `Orb, no win` / `wins`;
  NetHack `dies above the Castle` / `Castle or deeper, no Amulet` / `Amulet, no
  ascension` / `ascends`; Brogue by depth bracket plus `escapes`. OPEN: exact
  cut lines. Parimutuel makes late information harmless: everyone watching sees
  the same D:12, so the pool converges, it does not leak. The rational bet is
  the uncertain future, which is the fun.
- Money: two `ChipMove` variants, `DoorBetPlaced` (floor-guarded debit, one
  ledger row per bet) and `DoorBetWon` (credit, `counts_as_earnings = false`
  like `PotWon`, for the same reason: a gamble must not climb Top Chips). The
  burn is the gap with no credit row, like the pot. Caps: OPEN, start with the
  pot's 10 x 100 per pool per UTC day.
- Settlement: the ingest task, on landing a `door_runs` row, runs
  `UPDATE door_bet_pools SET status = 'settled' ... WHERE run_key = $1 AND
  status = 'open' RETURNING *` inside the transaction that credits winners, so
  N replicas each running ingest settle once. A run whose end line never
  arrives (host wipe, orphaned save) is voided by a sweeper after 30 days and
  refunded. OPEN: the refund `ChipMove`.
- Distribution: the crown/pot shape. A `door_bet_changed` notify seeds a
  process-shared `watch` of open pools with sizes; sessions project it on the
  tick. `/bet <handle> <bracket> <n>` in the composer, and a bet strip on the
  watch screen.

### A.3 Order of work

1. Host mirror + `late_watch` on `late-dcss`, TUI watch screen, live list. No
   bets, no web. This alone ships the spectating culture and proves the
   transport.
2. Web `/watch/{handle}` page over the API relay, with the §C strip.
3. Bets on DCSS: pools, brackets, settlement, HUD, feed line.
4. NetHack and Brogue hosts (near-clones; the door crates are twins by design),
   then Usurper watch-only (its shared world has no run to bet on).

### A.4 Telemetry

`door_watch_sessions` (door, surface tui/web), `door_watch_bytes`, `door_bets_placed`,
`door_bet_pools_settled` (outcome, voided), `door_bet_burned_chips`. Typed
`record_*` functions in the door telemetry module, labels from the closed enums.

### A.5 Risks

- Fan-out on the host: a popular run with 300 watchers is 300 broadcast
  receivers on one process. Bytes are tiny; the cost is task count. Cap
  watchers per run (OPEN: 200) and shed to the web page, which shares one
  relay per run in late-ssh rather than one host connection per viewer.
- Rollouts: a watcher's connection dies with the host pod, like the player's.
  The watch screen reconnects with backoff; a pool survives because it is rows.
- Consent: default-on spectating must be stated on the door landing and in the
  guide, and `/spectate off` must take effect on the next connect, not the next
  run.

---

## B. The share loop

A card is a few lines of text a player pastes wherever they already talk. It
gives nothing away, it fits in a phone screenshot, and its last line is a
shell command. That is the whole mechanic. Wordle grew on it and nothing
else; we have twenty-odd games that can each emit one.

Two surfaces, one key each. `s` copies the card to the clipboard. `S` posts
the same card into the room you are in. The second one matters more than the
first for a while: thirty people are already in chat, and a card posted into
#lounge turns the room into the group chat that Wordle only ever borrowed.
Nobody has to leave the tavern for the loop to close.

### B.1 The card grammar

Every card, every game, same shape:

```
late.sh Le Word #214 · 4/6
⬛🟨⬛⬛🟩
🟩⬛🟨⬛🟩
🟩🟩⬛🟩🟩
🟩🟩🟩🟩🟩
ssh late.sh
```

- Header: `late.sh <Game> #<n> · <result>`. The number is days since that
  game's first daily, so two people's cards from the same day match. The
  epochs live in one constant table.
- Body: at most 8 rows, no spoilers. A card must fit in a phone screenshot
  next to the header and footer. Two exceptions, where the art is the card:
  Artboard pieces and the bonsai.
- Footer: always `ssh late.sh`, never a URL. The command is the brand and the
  filter at once. Someone who knows what to do with it is in the tavern in
  thirty seconds; someone who does not asks, and a friend explaining beats
  any landing page. Since the footer cannot be clicked, the web root at
  `late.sh` must carry the §C strip, so a person who types it into a browser
  instead of a terminal still finds the door.
- Format: emoji by default, since it renders on every social network. A
  plain ASCII variant (`#`, `+`, `.`) for people who post in monospace is a
  preference in the settings modal, not a second key. Two keys per card is
  one too many.
- Copy goes through the existing OSC 52 path (`App.pending_clipboard`, the
  same one bonsai uses today). Post goes through the room's normal message
  send; a posted card is a message like any other, so it shows up in
  backlog, the feed, and search.
- A card posted into a room is clickable: click it and you are on that
  puzzle. Cards from the same day form the results thread by themselves.
- After you finish a daily, the result panel shows the cards of everyone
  else who finished it today, best first. Wordle never had this; people did
  it by hand in group chats. We have the data.

Code shape: a pure card builder per game beside its `state.rs` (`share.rs`
+ `share_test.rs`), returning a `ShareCard { title, rows, footer }` and
whole-state tested against fixed inputs. One central renderer turns a
`ShareCard` into text in either format. The copy and post actions, the
banner, and the telemetry call live at the orchestration edge, once, never
in a game module.

### B.2 The catalogue, in delivery order

**1. Arcade dailies.** Seven cards plus one.

- Le Word: the guess grid. A loss shows all six rows and `X/6`.
- Nonogram: the finished picture in half-blocks, so a 10x10 is 5 rows. The
  picture is the solution, and that is accepted: copying a picture by hand
  into clue-checked cells is more work than solving it, and the picture is
  the brag.
- Sudoku: no digits. A 3x3 of coloured squares, one per box, coloured by
  which third of your solve finished it (green, yellow, red). Every solver
  gets a different fingerprint on the same puzzle. Plus time.
- Minesweeper: no board, that spoils the mines. Difficulty, time, and a strip
  of your last ten clicks as glyphs: safe, flag, boom. A boom at the end
  tells the story on its own. Click history is session-local, so a board
  resumed from a save shows the clicks since resume.
- Solitaire: the four foundation piles as bars out of 13, spade heart diamond
  club, plus moves. A loss card shows exactly how far you got.
- Rubik's Cube: the solve as a colour ribbon, one square per face turn
  coloured by face, wrapping at 12 per row, so a 40-move solve is four rows.
  Nobody can spoil a cube; this is pure signature. Plus move count.
- Sliding Puzzle: a heatmap of where the blank tile spent its time, 4 rows
  for a 4x4, plus moves against par.
- The day card, from the arcade lobby: one row of seven glyphs, one per
  daily, filled for each you won today, plus your streak.
  `late.sh Daily #214 · 7/7 · 🔥 41`. This is the card people will paste
  every morning, because it is one card for the whole habit, not seven.

**2. Lobby daily matches.** The correspondence roster only (chess, chess960,
battleship, connect four, reversi, checkers, backgammon, briscola), never the
house tables. The card is the final position when a match ends, with
`mat beat kai in 34 moves` as the result. Chess fits in 8 rows exactly with
piece glyphs; connect four, reversi, checkers, and battleship are 8 rows or
fewer natively. Backgammon and briscola get a score line and no board. Chess
and chess960 get a second key that copies the PGN, since the move history is
already persisted and chess people want exactly that.

**3. Roguelike tombstones.** The NetHack tombstone is the most shared ASCII
in the genre's history, and the ingest pipeline already lands result, depth,
turns, and score in `door_runs`. One tombstone shape for DCSS, NetHack, and
Brogue: name, class, cause of death, dungeon level, runes or the Amulet. A
win gets a different frame. Because it renders from the row, it also works
for a run that ended while you were disconnected, from the door landing's
recent runs. Usurper has a shared world and no run line, so it waits.

**4. Artboard.** A hung piece is already text, so its card is the piece with
a byline and the applause count, and anyone can copy it from the gallery,
not only its author. Archive snapshots too. This is the exception to the
8-row cap, capped at the piece's own size.

**5. Fix the bonsai.** It has `s` today and emits the tree plus an
`ADMIRE my tree (Day N)` label, with no header and no footer. Bring both
renderers (`bonsai/state.rs` and `bonsai_v2/state.rs` `share_snippet`) into
the grammar: `late.sh Bonsai · Day 41` on top, `ssh late.sh` below, the
label gone, the dynamic tree allowed its full height like an Artboard piece.

**Later, mentioned so nobody re-derives them:**

- High-score arcade run cards at game over, with the rank line pulled from
  the leaderboard: `Lateris · 48,210 · #3 all-time`. Motifs: 2048's final
  board as a 4x4 tile heat, Snake's final length drawn as a wrapped green
  snake, Lateris as a bar per piece kind, Traffic as a coloured square per
  track grade. The rank is the brag, the motif the fingerprint.
- Native doors (Dope Wars, Green Dragon, Lateania, Dark Room, and the rest
  of `DoorGameId`): one generic outcome card over the closed enum, score and
  detail from `DoorGameEvent::Outcome`.
- Pet: three lines, face, name, age, mood.
- Profile `/card`: username, crown if held, streak, chips, badges. The
  deadchannel thesis in one paste: people want to be seen, and this is the
  status line they can carry outside.
- Moment cards: any `ActivityKind` story that names you (pot won, crown
  taken, boss killed) shareable straight from the feed line.
- The live run card, the one card allowed a URL (`late.sh/watch/mat`),
  ships with spectating in §A, not before.

### B.3 Challenge links

`/challenge` today needs both players to be users. Add an invite form:

- `/challenge link chess` creates an open challenge row with a nullable unique
  `invite_code` on `daily_matches` (short, `capability_id()` style but 8
  characters is enough, it is not an access token, claiming still needs an
  account). The composer answers with `late.sh/c/<code>`.
- `late.sh/c/<code>` is a late-web page reading the row: "mat challenged you
  to a game of chess. It is played one move a day over ssh." Then the §C
  onboarding strip, then the claim instruction: once in, `/challenge accept
  <code>`, or the Lobby modal shows "claim by code".
- Claim is the existing guarded UPDATE with `invite_code = $1 AND status =
  'open'` added, so a code claims once. The challenger cannot claim their own
  code. Codes expire with the challenge (no expiry in v1, same as open
  challenges), OPEN: a 7 day invite expiry so a stale link is not a stale row.
- The match then behaves like any daily match: private chat room, voice,
  payout gates, the #lounge result line. A friend brought in by a link is
  playing within minutes of their first `ssh`.

A card is a broadcast to everyone; a challenge link is an invite to one
person, and the only share with somewhere to land.

### B.4 Public result surfaces (later)

`late.sh/daily`: today's champions per daily, no spoilers, updated from the
leaderboard snapshot. Cheap, but it is the third thing, not the first.

### B.5 Order of work

1. The grammar, the renderer, `s` and `S`, and the Le Word card. One game,
   both surfaces, the loop proven. Built 2026-09-07.
2. The other six dailies and the day card. Built 2026-09-07, session-local
   histories for Sudoku, Minesweeper, Rubik's, and Sliding Puzzle included;
   the ASCII format is rendered and tested but not yet wired to a setting.
   Still open from this step: clickable posted cards, and the finishers'
   cards on the result panel, which needs the card persisted per win.
3. Lobby daily match cards, PGN copy.
4. Roguelike tombstones from `door_runs`.
5. Artboard gallery copy for everyone.
6. Bonsai brought into the grammar.
7. Challenge links: column, command, claim guard, page.
8. `/daily` page.

### B.6 What we can and cannot measure

We cannot see pastes. We can count copies and posts per game and format
(`share_card_copied`, `share_card_posted`), clicks on posted cards
(`share_card_opened`), and first connects per day. Challenge links and the
start page are the only outside surfaces with real view counts
(`challenge_invite_created`, `challenge_invite_claimed` with days since
created, `challenge_invite_page_views`), so they are where conversion gets
measured. Cards prove intent, links prove arrival. If copies stay near zero
for a week after step 2, the outside half of the loop is dead for this crowd
and the in-room half still stands on its own.
---

## C. The "how to get in" page, instead of a web terminal

Decision: no browser terminal. The SSH requirement is the moderation system,
and a guest web terminal is a troll door with the filter removed. Every
outside link lands instead on a page that teaches a curious person to get in,
in under a minute, on their OS.

- Route: `late.sh/start`, also embedded as a strip on `/watch/{handle}` and
  `/c/<code>` so nobody has to find it.
- Content, per OS tab: macOS and Linux (one command, keys generated for you by
  the CLI installer, or `ssh-keygen` in one line), Windows (OpenSSH is built
  in, PowerShell one-liner), Android (Termux), iPhone (Blink or Termius).
  Then "your key is your account, there is no signup". Then what they will see
  first: the tavern, `?` for the guide, `Tab` to move around. Then the CLI
  installer for audio.
- The page states the deal plainly: it is a terminal, it is text, and it is
  the same for everyone. That sentence is the filter, said out loud.
- What the web does: watch, read, get invited. What the web never does: talk,
  bet, move. This is the whole security posture of the plan and should be
  written into `late-web/CONTEXT.md` as an invariant when the first page ships.

---

## Not doing

- Agents as citizens. Wrong crowd for now.
- A web terminal or guest mode. See §C.
- Betting on Lateania, Green Dragon, A Dark Room: native games with no
  external run log; a market there would settle on our own state, which is
  fine technically but is not the roguelike spectator culture this bets on.

## Open decisions (collected)

1. Ring buffer size and watcher cap per run.
2. `run_key`: start-milestone keyed pools, confirm each door's start line.
3. Bracket cut lines per door.
4. Bet caps per pool and per day; the refund `ChipMove` for voided pools.
5. Invite code expiry.
6. Sudoku and Minesweeper cards need per-solve histories (box completion
   order, click log) that the states do not keep today; session-local is
   enough, but confirm before the card is designed around a saved board.

---

## Backlog

Ideas and parked designs. A `CONTEXT.md` describes what is; this list holds
what might be, so nothing below is scheduled or promised. When an item is
picked up it gets a section of its own above, or its own design doc, and
leaves this list. When an item is rejected for good, it moves to "Not doing"
with the reason.

### Product loop and measurement

- Nail one addictive loop: join, listen, chat, vote, return tomorrow.
- Pick a clear ICP: solo devs at night, or remote teams during work hours.
- More reasons to come back beyond the daily puzzles, chips, and leaderboard
  that exist: daily room rituals (lo-fi standup, shipped rollup, weekend
  recap) and timed events (coffee breaks, AMAs, mini coding jams). The
  deadchannel game (`late-ssh/src/app/deadchannel/GAME.md`) is the current
  answer to the ritual half.
- Keep friction near zero: `ssh late.sh`, with late.sh/listen for anyone who
  only wants the audio.
- Measure retention early: D1/D7 return, session length, messages per user,
  votes per session.

### Chat

- Better backlog pagination, moderation polish.
- Matchmaking from chat: `/play <game>` or `/challenge @user <game>` (the
  challenge-link form is §B.2 above).
- Snippet paste into the `/pair` scratchpad.
- Ambient presence: quiet hours, listening since, typing indicator.
- Community texture: rotating shoutout board, wall of thanks.
- Personalization: accent color, favorite vibe, custom tagline.
- Cozy utilities beyond `/pomodoro`: focus playlists, now-playing shoutouts.

### Games

- Monthly chip leaderboard resets and hall-of-fame surfaces.
- Strategy multiplayer with a W/L record or a rating, beyond the daily
  correspondence roster.
- Nonograms v2: replace random generation with a pixel-art-to-nonogram
  pipeline, or bulk-curate from webpbn.com.
- Classic BBS / door-game references for future door work (study, not port
  targets): Legend of the Red Dragon and LORD II, Arrowbridge / Arrowbridge
  II, TradeWars 2002, Falcon's Eye, Barren Realms Elite, Solar Realms Elite,
  Land of Devastation, The Pit, Sinbaud, Bordello, Yankee Trader. Any
  LORD-like work is the deadchannel game: native late.sh design, never a
  fragile DOS binary in production.
- Persistent multiplayer 4X / trading world where every connected session is
  a participant: parked, not wrong. The most liquidity-hungry genre there is,
  coordination machinery for a population we do not have; see the graveyard
  note in `GAME.md`. Open questions if it is ever revived: tick-based or
  real-time with rate-limited actions, how much happens offline, map
  topology, win conditions or endless sandbox.
- `monteslu/retroemu` lobby (libretro cores in WASM rendered as ANSI through
  chafa-wasm, `https://github.com/monteslu/retroemu`): researched, do not
  pursue. It has a usable programmatic API (`LibretroHost`, `VideoOutput`,
  `AudioBridge`, `InputManager`, `SaveManager`) but does not solve the real
  blocker, legally clean redistributable games. Candidates found: Tobu Tobu
  Girl DX (GB/GBC, MIT + CC BY 4.0), uCity (GBC, GPLv3+ + CC BY-SA 4.0),
  Mr.Boom (MIT, 8-player Bomberman-style libretro core), Freedoom via a Doom
  core, 2048 ports. Only Mr.Boom would change the multiplayer story. A late.sh
  lobby would need one emulator process per room, controller-port
  assignment, remote multi-port input (upstream work), frame broadcast, audio
  reconciliation, ROM licensing, per-room saves, CPU/memory quotas, and abuse
  controls before a prototype.

### Bonsai

- Seasonal color shifts (real-world date), profile display for visitors,
  graveyard rendering on the profile.
- Fancier renderer: port or adapt `cbonsai`
  (https://github.com/mhzawadi/homebrew-cbonsai) for richer growth animation
  and branching.

### Widgets

- GitHub notifications widget: read-only PR reviews, mentions, and issue
  updates via a PAT. A productivity reason for solo devs to keep the terminal
  open.

### Audio

- Direct radio polish: surface artist/title attribution from the Nightride
  SSE metadata, and let voting choose between approved Nightride stations.
- Jazz playlist: the thinnest genre and a removal candidate if never filled.
  Source targets: HoliznaCC0, Kevin MacLeod, Ketsa.
- Verified CC0/CC-BY sources, not yet downloaded:
  - HoliznaCC0: 571 tracks across ~50 albums, all CC0,
    https://freemusicarchive.org/music/holiznacc0/discography
  - Ketsa: large catalog (lofi, jazz, soul, ambient, downtempo), CC-BY; the
    album "CC BY: FREE TO USE FOR ANYTHING" has 70 tracks,
    https://freemusicarchive.org/music/Ketsa/cc-by-free-to-use-for-anything
  - John Bartmann: "Public Domain Soundtrack Music: Album One" (CC0) on
    Bandcamp
  - Kevin MacLeod: 359 tracks (CC-BY),
    https://kevinmacleod.bandcamp.com/album/complete-collection-creative-commons
  - FMA public domain search (9,000+ tracks):
    https://freemusicarchive.org/search?adv=1&music-filter-public-domain=1
- Rejected sources, so nobody re-researches them: Pixabay (custom license,
  not for a standalone stream), Chad Crouch (CC BY-NC plus commercial split),
  Blue Dot Sessions (CC BY-NC only), Kai Engel (mixed CC-BY/CC-BY-NC,
  licensing unstable since July 2025), Classicals.de (terms unclear).

### Artboard

- The web `/gallery` listing hung pieces (today it only shows the archive
  snapshots).

### Infra

- Stop routing SSH through ingress-nginx: a dedicated TCP LoadBalancer,
  NodePort, or host proxy for port 22, so HTTP/TLS config reloads (cert
  renewals are a recurring trigger) cannot drop long-lived SSH sessions. The
  short-term mitigation, raising `worker-shutdown-timeout`, only delays the
  disconnect. The risk itself is recorded in `CONTEXT.md` §7.
