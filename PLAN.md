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

### B.1 Share cards

Every daily emits a spoiler-free result card, copied to the clipboard through
the existing OSC 52 path (`App.pending_clipboard`, the same one bonsai `s`
uses). Key `s` on any daily's result panel.

```
late.sh Le Word #214 · 4/6
⬛🟨⬛⬛🟩
🟩⬛🟨⬛🟩
🟩🟩⬛🟩🟩
🟩🟩🟩🟩🟩
ssh late.sh
```

- Le Word: the guess grid. Nonogram: the solved picture in block glyphs.
  Sudoku, Minesweeper, Solitaire, Rubik's Cube, Sliding Puzzle: difficulty,
  time or lives, and a small motif each (OPEN per game; a card must fit in a
  phone screenshot, at most 8 rows).
- Puzzle number is days since each game's first daily, so two people's cards
  from the same day match.
- Two formats per card, one keypress each: `s` emoji (renders on every social
  network), `S` plain ASCII (`#`, `+`, `.`) for people who post in monospace.
  OPEN: whether one of these is enough.
- The card builder is a pure function per game beside its `state.rs`
  (`share.rs` + `share_test.rs`), whole-state tested against fixed inputs.
- The footer line is the whole marketing. It always reads `ssh late.sh`, never
  a URL, because the command is the brand.

### B.2 Challenge links

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

### B.3 Public result surfaces (later)

`late.sh/daily`: today's champions per daily, no spoilers, updated from the
leaderboard snapshot. Cheap, but it is the third thing, not the first.

### B.4 Order of work

1. Le Word card, both formats, `s` on the result panel. One game, the loop
   proven.
2. The remaining dailies, one card module each.
3. Challenge links: column, command, claim guard, page.
4. `/daily` page.

### B.5 Telemetry

`share_card_copied` (game, format), `challenge_invite_created` (game),
`challenge_invite_claimed` (game, days since created), `challenge_invite_page_views`
in late-web.

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
5. Share card motifs for the non-grid dailies; one format or two.
6. Invite code expiry.
