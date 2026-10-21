# deadchannel Context (late-ssh/src/app/deadchannel)

## Metadata
- Domain: the deadchannel game (`GAME.md`). Open to everyone: every session can be haunted, and the gate in `haunt::svc::bootstrap_gate` decides who climbs past the clock and gets the invitation that opens #deadchannel. Built for several replicas (root `CONTEXT.md`, multi-replica rule).
- Subdomains and their docs:
  - `haunt/`: the first-contact haunting ladder (`haunt/CONTEXT.md`).
  - `runner/`: the character layer, the runner and its look (`runner/CONTEXT.md`).
  - `city/`, `street/`, `tailor/`, `poster/`, `guide/`: the night city under the clubhouse on a second `0`, the shared street over `app/presence`, the mirror that edits the look, PRINTS (the poster on the profile), the undercity guide (`city/CONTEXT.md`).
  - `fight/`: the runner's sheet, the day's road, the round of cards, the wire's news, the shops, crystals, Dead Air (`fight/CONTEXT.md`, with `fight/MONEY.md`, `fight/BALANCE.md`, `fight/CRYSTALS.md`, `fight/CROSSING.md`).
  - `lounge/`: the spawns above #lounge's composer (`lounge/CONTEXT.md`).
- Design sources in this directory: `GAME.md` (the game: thesis, first contact, the runner) and `DIGEST.md` (the feed budget and the welcome-back paper).
- Status: Active. Parent context: `../../../../CONTEXT.md`.

## 1. Summary

The game is never announced; it arrives. What exists: **first contact**
(the escalation ladder that haunts a person into the game instead of a
tutorial, `haunt/`), the **runner** a person becomes on joining
#deadchannel (`runner/`), the **night city** under the clubhouse
(`city/`), the **road and the round** at the end of its street (`fight/`),
and the **lounge spawns** runners put down together above #lounge's composer
(`lounge/`). Who is haunted (everyone sees the clock; a gate of hours,
touched settings and a screened bio picks who climbs further) and how the ladder paces itself are in
`haunt/CONTEXT.md`.

**Replica rule.** Nothing in this domain is a process-local source of
truth: every cap, stamp and spend is a conditional claim on a row, so any
number of sessions on any number of replicas agree. The few process-local
pieces, like the stage-2 wire, are delivery paths, never sources of truth.

## 2. Module map

| File | Owns |
|---|---|
| `haunt/` | The first-contact haunting: see `haunt/CONTEXT.md`. |
| `runner/` | The runner: the look, the directory, the face in #deadchannel and on the profile: see `runner/CONTEXT.md`. |
| `city/`, `street/`, `tailor/`, `poster/`, `guide/` | The night city, the shared street over `app/presence`, the mirror that edits the look, PRINTS (the 32 by 12 poster a runner paints and hangs on the profile), the undercity guide: see `city/CONTEXT.md`. |
| `fight/` | The sheet, the road, the round, the shops, crystals, Dead Air: see `fight/CONTEXT.md`. |
| `lounge/` | The lounge spawns: see `lounge/CONTEXT.md`. |
| `glyphs.rs` | `GLYPH_ALPHABET`, the game's shared character vocabulary. Game-level: the haunting borrows it, stage-4-era spawns will render with it (the clock glitch is retroactive foreshadowing). Distinct from the static shades `░▒▓` (noise, not creatures). `MARK_ALPHABET` is what a runner can wear as a mark: the alphabet less the Signal's `╬`, which is the paragon count in the badge and nobody's for free. |

## 3. The four stages (behavior contract)

`haunt/CONTEXT.md`: the clock glitch, the whisper, the splash, the breakthrough and the name hit, with the root and chat seams that carry them.

## 3b. The night city

`city/CONTEXT.md`: the undercity under `0`, its map, shops, panels and art, and the `city/` files.

## 3c. The road and the round

`fight/CONTEXT.md`: the day's road, the service and the wire, the armorer and patch, the crossing, and the `fight/` files (the round, the cards and the screens in `fight/MACHINE.md`; money, crystals and balance in the docs beside it).

## 3d. The lounge (the snack: a glyph above the composer)

GAME.md, "The lounge pass": the first shared world object, a flicker or
a howler in a scene above #lounge's composer that runners put down together with
`/hit`, three flickers and a howler a day by dice, paid through the
sheet's one writer. The contract (the day row and its claims, when one
lands, the hit, the pay before the kill is told, the wire, the scene,
the copy on the channel, the beats) is `lounge/CONTEXT.md`; the file
roles and seams are there too.

## 4. Persistence

- `deadchannel_runners` (model `late-core/src/models/deadchannel_runner.rs`): one row per runner, the look, the sheet, and the poster (`poster`, migration 249, the serialized canvas PRINTS writes); columns, triggers, the nuke and the leave stamp in `runner/CONTEXT.md`, the sheet's writer in `fight/CONTEXT.md`.
- The first-contact chain keys on `users.settings` and the table-less stage-2 wire: `haunt/CONTEXT.md`, Persistence.
- `deadchannel_ambushes` (model `late-core/src/models/deadchannel_ambush.rs`): one row per fight between two runners, its unique partial indexes are the caps: `fight/CROSSING.md`.
- `deadchannel_lounge_days` and `deadchannel_spawns` (model `late-core/src/models/deadchannel_spawn.rs`): the lounge's day budget and its spawns: `lounge/CONTEXT.md`.
- Everything else is render-only and session-local: no chat rows, no IRC
  projection (the invitation DM is the deliberate exception: stage 4 is
  where the fiction goes real, and an invitation that vanishes cannot be
  followed three days later). A witnessed beat is render-only too: the
  message body, the row, and the IRC projection are all untouched, so the
  corruption exists only on the screens that were looking.

## 5. `/haunt` (admin composer command)

`haunt/CONTEXT.md`: admin-only, parsed in `chat/state.rs::submit_composer`; for anyone else the line posts as plain text.

## 6. Gotchas

The haunting's own gotchas are in `haunt/CONTEXT.md`, the runner's in `runner/CONTEXT.md`.

- Where to look when the ladder seems dead (per person, in the logs, all
  keyed by `user_id` and `username`, the two fields every deadchannel line
  carries): `first contact gate evaluated` at every connect
  with each leg's number, the bio standing, and the `GateVerdict`; `first
  contact armed` for every session, with `chosen` and `whisper_armed`; then one line per hit, whisper, breakthrough, invitation, bio
  screen, and runner. How many the gate turns away, and on which leg, is
  `late_ssh_first_contact_gate_total{verdict}` (one count per
  connect, not per person); bio screens by outcome are
  `late_ssh_first_contact_bio_screens_total`; delivered beats are
  `late_ssh_first_contact_beats_total`. The street's wire is
  presence's, `late_ssh_presence_total{beat}` (`late-ssh/CONTEXT.md`). The
  game's own counters sit under the same row: fights by beat (the per-
  exchange `round` beat left out; the crossing's `crossed`, `answered`,
  `ambushed`, `downed` ride the same counter), the tailor, PRINTS
  (`late_ssh_deadchannel_poster_total{beat}`: saved, burned, no_runner,
  failed for the writes; read_failed and undecodable for the read when
  the panel opens), and the door, Old Signal
  payouts, and the lounge (`late_ssh_deadchannel_lounge_total{beat}`:
  spawned, hit, slain, slipped, paid, pay_left, pay_failed, hit_refused,
  hit_failed). The name is a log field only, never
  a metric label: the three counters stay keyed on closed enums so the
  series count cannot grow with the player base. Grafana's "deadchannel"
  row (`monitoring/dashboards/observability.json`) reads both: the beat and
  gate counters as reset-safe `max_over_time` sums, and from the log
  lines a Runners table (one row per person: ladder hits, invited and
  joined times, last seen, latest gate verdict and legs), a Recent Beats
  table, and the Haunt Log, all three filtered by the `$runner` regex
  variable. The log tables use the `instant` query type and extract
  fields, because a `stats` query splits every group into its own series
  and cannot carry strings; logs keep 7 days, so older rungs show empty
  there and `users.settings` stays the truth. Counters live on the pod, so a
  deploy zeroes the live value; every panel there sums per-instance
  high-water marks instead. The row ships to prod with the dashboard
  ConfigMap, on a `-infra` release (`infra/monitoring.tf`), not on merge.
- A fight action holds a pooled connection while it waits for the row
  lock, so `FightSession::request` drops a press while one is out (the
  bonsai rule); the scene shows `the static is deciding.` meanwhile.
- The fight's dice are `rand::thread_rng` in the service; the state
  takes any `Rng`, so `state_test` drives a fight to the end with a
  seeded `StdRng` and asserts the whole sheet.
- The city map is generated: hand edits to `city/map.rs` are clobbered by
  the next `scripts/gen_city_map.py --write`. Move a prop in the script
  and its zone, reach, and animation cells move with it. The literal
  arrays carry `#[rustfmt::skip]`, so `cargo fmt` and `--write` agree.
- The city's copy (`city/data.rs`: shop lines, cart lines, draft move
  names, notices) is placeholder content at feed-template quality
  standards and faces design review with the rest of the phase 2 copy.
