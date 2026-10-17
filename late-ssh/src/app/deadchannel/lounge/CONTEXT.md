# The lounge spawns, as built (late-ssh/src/app/deadchannel/lounge)

Parent: `../CONTEXT.md` §3d (the pointer and the seams). Design and the
reasons: `../GAME.md`, "The lounge pass". The first shared world object:
a glyph in a three-row scene above #lounge's composer that runners put
down together. Runners only; civilians see a composer. Three flickers and one howler a day,
spread by rule, landing by dice.

## File roles

| File | Owns |
|---|---|
| `state.rs` | Pure: `Due` and `due` (what the spawner lands this tick: a window's flicker once its minute has come, its window is open, and a runner is looking at #lounge; the howler once its minute has come and two are; nothing while one is live), `roll_dice` (the day's moments), `hits_needed` / `lifetime_secs` / `glyph` per `SpawnKind` (one and three hits; 90s and 3m; the cell), `pay_for` (a flicker a cache's worth of bits, a howler a crystal each, as the sheet's own `fight::state::Leavings`), `SPAWN_GAP_SECS` (45m), `parse_lounge_command` (`/hit`), `shows` (live and in time, or ended within `AFTERMATH_MS`), `is_stale` (an older payload of the spawn already held: ended here and live there, or fewer hits), `names` (the scene's) and `mentions` (the wire's), `scene_text` (what the scene says to a viewer, the headline and the detail, and whether the `/hit` key goes with them: `SceneText`), `STRIKE_MS` (5s) and `animates` (whether the scene has frames to play: a strike, or the face dissolving through the aftermath), `left_secs` (the countdown). No I/O, no clock. |
| `data.rs` | The copy and the art: the wire's slain and slipped lines, the scene's answers to a hit (`hit_line`, already hit, nothing to hit, signal down, not answering, deciding), `face` per kind (nine cells by three rows, the lounge's own, not the fight's), `EMPTY_SEAT`, `GHOST_PORTRAIT`, `STRIKE_FRAMES`. |
| `svc.rs` | `LoungeService`: the spawner and sweeper on every replica (`start_spawner_task`, every 20s: end what slipped and tell the wire, pay what a dead replica left unpaid, land what `due` says: the day row ensured with this replica's dice, the claim and the insert one transaction, the announce after the commit), the notify worker (`deadchannel_spawn` carries the whole row; a stale payload is dropped by `is_stale`; a resync re-reads the live one) into a process-shared `watch<Option<Spawn>>`, `hit_task` / `hit` (the runner's standing and signal off `FightService::settled_sheet`, then one conditional update on the live spawn; the hit that makes the count calls `settle`, the pay claim and `FightService::pocket` per hitter and the wire line, and only then announces the row), `announce` (a `pg_notify` that fails is logged and counted, never the hit or the spawn), `tick(app, anim_half)` (the session's side: the `hits_on` flag chat parses `/hit` under, the hit, the copy, the word, the sheet re-read after a kill this session was part of, and the frame owed on the half edge while the scene animates), and `animates(app)` (the wake cadence's seam: a runner on the #lounge card with a scene playing). The metric and the log line per outcome. |
| `session.rs` | `LoungeSession`: the tick copy of the spawn, the session's own word and its expiry, one hit in flight, `showing` (the spawn the scene shows at `now_ms`), `animating`, and the seconds-left last painted so the countdown owes one frame a second. Decides nothing. |
| `ui.rs` | `LoungeScene` and `scene` (three rows, `SCENE_HEIGHT`: one seat per hit the spawn needs, a hitter's portrait off the runner directory in theirs (`runner::ui::portrait_spans`, the ghost shape when the directory lost them), the lane with the strike crossing it for `STRIKE_MS` after the last hit and the face flashing when it lands, the face in amber while live and dissolving cell by cell through the aftermath, then the words: the headline, the detail or the session's own word, `/hit` lit when the key is on and `m:ss left` while live). Pure; `chat/ui.rs::draw_dashboard_chat_card` gives it its rows between the ticker and the composer. |

Tests sit beside each file (`state_test.rs`, `svc_test.rs`, `ui_test.rs`).

## The rules

- **The budget is a row a day** (`deadchannel_lounge_days`, migration
  239): the first replica to touch a UTC day inserts it with its dice (a
  minute inside each eight-hour window for that window's flicker, a
  minute anywhere for the howler), every other reads what it rolled. The
  new row starts its `last_spawn_at` from the newest spawn on record, so
  the gap holds across midnight. Spending a flicker or the howler is a
  conditional update on its spent flag, with the 45-minute gap in the
  same `WHERE`, so any number of replicas spend each exactly once; the
  insert of the spawn is in the same transaction and the one-live
  partial index (`WHERE ended_at IS NULL`) rolls the claim back if
  another kind landed first.
- **When one lands.** The spawner on every replica asks every 20s
  (`state::due`): a window's flicker once its minute has come, while the
  window is open, with at least one runner looking at #lounge (the
  `lounge` flag on presence, counted against the runner directory); the
  howler once its minute has come, any time in the day, with at least
  two. A minute that passed with nobody looking lands the moment someone
  is; a window that closes unspent loses its flicker. A live spawn
  blocks the next. A draining replica stops asking.
- **A hit is one command.** `/hit` in the composer is the lounge's only
  while the session is a runner looking at #lounge (`hits_on`, written
  by the lounge tick): swallowed then, never posted, whatever is there
  to hit. Anywhere else, and for anyone else, it falls through to
  chat's unknown-command banner like any slash word, never a post.
  The service reads the runner's standing and signal off the settled
  row (a dropped signal cannot hit, as decided under "Death"), then one
  conditional update appends the hit and ends the spawn as slain in the
  same statement when the count is made. **One hit per runner per
  spawn** (the `NOT EXISTS` over `hits`): a howler needs three runners.
  Level does not matter: a hit is a hit.
- **Pay goes through the sheet's one writer, before the kill is told.**
  The slaying hit takes the pay claim (`paid_at`, one caller) and calls
  `FightService::pocket` per hitter: `Command::Pocket(Leavings)` under
  the row lock, a flicker a cache's worth of bits at the hitter's level
  (the machine's garnish first), a howler a crystal each, never exp.
  Only then is the slain row announced on the channel: every hitting
  session re-reads its sheet when a spawn it hit reaches it as slain,
  and what it reads has to hold the leavings. A hitter who left the row
  meanwhile is logged and left; a pay that errs is logged and counted,
  not retried (the claim is taken, and paying twice is the worse
  failure). A slain spawn a dead replica left unpaid for 30s is settled
  by any sweeper; those sessions saw the kill before the pay and catch
  up on their next press.
- **The wire, once per spawn:** "@tom, @dax and @mira put down a howler
  in the lounge. a crystal each." or "a howler slipped past the lounge.
  nobody was looking." (with the hitters mentioned when some hit it:
  `mentions`, the wire's twin of the scene's `names`).
  Nothing in #lounge, nothing in scrollback, nothing on IRC: the spawn
  is render-layer chrome above the composer of runners looking at the
  lounge card, and the `/hit` line never becomes a message.
- **The scene.** Three rows open between the ticker and the composer
  while a spawn shows, and close after; the composer itself never
  changes. Left to right: a seat per hit the spawn needs (the hitters'
  portraits as they land, empty seats for the hits still owed), the
  lane, the face, the words (the headline, who hit last and how many to
  go, `/hit` lit unless the viewer already hit it, the seconds left).
  A hit sends a strike across the lane for 5s and the face flashes
  when it lands; the wake cadence rides the half edge while it plays
  (`svc::animates`). The session's own answer (`your tire iron hits
  the howler. 2 to go.`, `you already hit it.`) stands in for the
  detail for 8s. A spawn that ended stays 8s more as the aftermath
  (who put it down, or that it slipped) while the face dissolves, then
  the rows close. A spawn past its time hides on every session's own
  clock before the sweeper ends it.
- **The copy of the spawn** rides `deadchannel_spawn` (the whole row as
  JSON, every change: landed, hit, slain, slipped) into a process-shared
  `watch`; sessions copy it on every tick. Two hits landing together can
  notify in the other order from their rows, so the worker drops a
  payload older than the copy it holds (`is_stale`). A replica that
  boots mid-spawn re-reads the live row on its resync. There is no
  "hide the static" setting yet (GAME.md names it; its name is open).

## Telemetry

`metrics::record_deadchannel_lounge(LoungeBeat)`: `spawned`, `hit`,
`slain`, `slipped`, `paid`, `pay_left` (no standing runner to pay),
`pay_failed`, `hit_refused`, `hit_failed`, `notify_failed` (a row that
moved but could not be announced). The fight counter sees each pay as
`pocketed` on its own side. Spans: `deadchannel.lounge.sweep`,
`deadchannel.lounge.hit_task`, `deadchannel.fight.pocket`.
