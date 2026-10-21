# The runner (late-ssh/src/app/deadchannel/runner)

Part of Dead Channel (`../CONTEXT.md`; design in `../GAME.md`, "The runner").
The character layer: one `deadchannel_runners` row per person who joined,
its look (hood, eyes, coat, mark, unlocked by level), the process-shared
directory every replica keeps of standing runners, and the face and badge
that runner wears in #deadchannel and on the profile. The sheet columns on the
same row are written by the fight (`../fight/CONTEXT.md`); the street, the
tailor's mirror and the guide that read the look are listed with the city
(`../city/CONTEXT.md`).

## Files

| File | Owns |
|---|---|
| `runner/state.rs` | The look: `PIECES` (the closed table, one five-cell row per piece, `Slot` hood/eyes/coat, each with the `level` that unlocks it: three per slot at 1, 4, 7, 10, 13; no earned or legendary pieces, every piece is reachable by level), `Tint` (the closed palette of seven, each with its unlock `level`, white alone at 15; gold deliberately absent), `UNLOCK_LEVELS`, `unlocked_pieces` / `unlocked_tints` / `next_unlock` (the level gate), `Look` + `Worn` (typed, table references), `Look::random(level, rng)` (the join's dice at 1, the tailor's shuffle at the runner's level), `Look::to_json` / `Look::parse` (the JSON contract on the runner row; unknown codes are a `LookError`, never a blank), `PORTRAIT_WIDTH` / `PORTRAIT_HEIGHT`. No I/O. `state_test` asserts every row is five single-width cells and pins the unlock ladder whole. |
| `runner/ui.rs` | `portrait_spans`: the look as three styled spans, one per worn piece in its tint; `badge_text` (the mark and the level, `▚7`, with the marks behind the Signal's glyph once there are any, `▚3╬2`) and `level_color` (the newest tint the level unlocked: static to 3, phosphor to 6, cyan to 9, magenta to 12, red to 14, white at 15) for the wire's author header and the profile; `tint_color` maps the palette onto the theme (cyan and magenta fixed, the theme has neither). Pure. |
| `runner/data.rs` | `welcome`: the voice's welcome for a runner whose row was just created, one message, one paragraph per line (the runner mentioned by name, the story so far, `0` twice as the way down, `/leave` and `/join #deadchannel`). Names no key on the street: those are the guide's. Placeholder copy at feed-template standards. Pure. |
| `runner/svc.rs` | `RunnerLookService`: the process-shared runner directory (`watch<Arc<HashMap<Uuid, RunnerEntry>>>`, an entry being the look, level, peak level, and marks of a standing runner), seeded and refreshed from `deadchannel_runners` (`list_standing`) on the `deadchannel_runner_changed` LISTEN, re-read whole on any change. A look that fails to parse is logged and skipped. `fixed_looks_rx` for test apps. |

## Seams

The runner's seams are as thin: `ChatService::join_deadchannel_room`
creates the row (`DeadchannelRunner::ensure_for_user`, a conditional
insert, so two devices joining at once share one face; a fresh row is
the `RunnerCreated` beat and posts the voice's welcome on the wire
(`runner/data.rs::welcome` through `post_wire_line_task`, a mention of
the runner, once per person because only the winning insert lands there;
a second device or a return finds it in the room's history), a return
the `RunnerDoor::Returned` one),
`ChatService::leave_room` stamps the leave for the `deadchannel` kind
(`mark_left`, the `RunnerDoor::Left` beat), `State.runner_looks` holds the directory
service (`main.rs` starts its listener), `App.runner_looks` is the
session's owned copy refreshed on the 1 Hz edge in `tick.rs` (bumping
`chat_ctx_epoch`, so the rows rebuild once per change; the same edge
reloads `App.fight`'s sheet for a runner and drops it for a leaver), and
chat's rows builder takes `runner_looks: Option<&HashMap<Uuid, RunnerEntry>>`,
`Some` only while the rendered room is #deadchannel: a runner's author
header opens its badge stack with the level badge (`runner/ui.rs::badge_text`,
`▚7`, painted in the level's band through `AuthorTint.runner`; the
first badge, so it sits right after the name, crown, and title),
every entry in the room wraps `PORTRAIT_GUTTER` (6) cells short, and a block-opening message by a
runner gets `attach_portrait` (the face right-aligned on the entry's
first rows, the hood level with the header, wearing what the entry has
rows for: a one-liner, header plus one body row, shows hood and eyes
and hands the coat to the first row of a continuation right under it
(none, or a divider between them, and it goes bare), anything taller
wears the coat itself, so no message grows a row for its face). The blank separator above a block stays blank, so two faces
stacked down the wire never touch. The profile modal grows a runner
column beside the late.fetch grid for a standing runner (`ProfileSnapshot.runner`,
loaded by `ProfileService::do_find_profile` from the row; no frame, its own
heading carrying the level badge, the face wounded by the missing signal beside
one key column: the signal and exp bars, the bits, the kit, the glyphs down), shown only when
the viewer is a runner too (`profile_modal::ui::draw`'s
`viewer_is_runner`, one argument to drop at the public flip;
`late-ssh/src/app/profile_modal/CONTEXT.md`). Continuations and system lines carry
no face; every other room renders without runner chrome.

## Persistence

- `deadchannel_runners` (model `late-core/src/models/deadchannel_runner.rs`):
  one row per user (`user_id` unique, cascade on delete). Columns:
  - `look` JSONB, `{"hood": {"piece", "tint"}, "eyes": ..., "coat": ...,
    "mark": {"glyph"}}`.
  - `left_at`, the leave stamp; `guide_seen_at`, the first descent's claim
    (`mark_guide_seen`, a conditional update on the standing row, fires no
    trigger).
  - The sheet: `level`, `exp`, `signal`, `weapon_tier`, `armor_tier`,
    `bits`, `rations_left`, `day` (the UTC date of the last roll), `fight`
    (JSONB, the fight in progress or null: the quarry
    `{"quarry": {"glyph": n}}`, the round, and the three piles of the
    deck), `crystals` (`CHECK >= 0`), `drink` (the day's glass, a code from
    the closed `Drink` menu, cleared by the day roll), `peak_level` (the
    highest level ever reached, the tailor's gate), `marks` (Old Signal
    kills), `unpaid_mark`, `stash` (the locker) and `debt` (the bits
    machine, whole bits, `CHECK >= 0`), `road` (JSONB, the day's `RoadRun`:
    `{"path": [{"lane", "mark"}], "static_cards", "patched"}`, null before
    the first step and after the roll; `patched` defaults when absent),
    `cards` (JSONB, the drafted cards in draft order, `["brace",
    "bulwark"]`, null with none taken), and `inks` (JSONB, the draft levels
    inked at INK in the order bought, `[3, 9]`, null with none; kept
    through a mark and the ledge).
  - The tally: `kills` (ever), `kills_today`, `runs_today` (zeroed by the
    day roll; the last-ration line's numbers).
  - `reset_generation`, bumped by every nuke and by nothing else; the Old
    Signal's payout key is `<runner row id>:<generation>:<mark>`, so a
    mark earned again after a nuke is paid, and the nuke leaves
    `unpaid_mark` standing (chips the house still owes).

  `deadchannel_nuke_runners()` puts every sheet back to a fresh one for a
  balance test (the row, the look, and the leave and guide stamps stay),
  and first closes every crossing still being fought as dropped, since the
  fight `settle_day` would close it from is gone (`fight/CROSSING.md`).
  To nuke, ship a migration holding only `SELECT
  deadchannel_nuke_runners();`. A migration that adds a sheet column adds
  it to that function, and a column whose stored values the new code
  cannot read (a card a draft no longer offers, a fight with no deck)
  ships with a nuke.

  Rows are created only by the invited join with the column defaults
  (level 1, signal 10, 50 bits, ten rations, today). The look is written
  again only by the tailor's mirror (`store_look`, standing runner only).
  An insert, or an update of `look`, `left_at`, or `level` (`peak_level`
  and `marks` only move with the level), fires
  `deadchannel_runner_changed` (payload: the user id, for logs only;
  listeners re-read every standing row); a sheet write that leaves the
  level alone fires nothing, so ten fights a day per runner wake no
  directory, and a level gained reaches every badge and HUD.
  - The sheet is written whole by `store_sheet` under `lock_standing`
    (`SELECT ... FOR UPDATE` on the standing row: `None` for a runner who
    left, so the door being shut also shuts the fight). The service is
    the only caller of both.
  - `/leave #deadchannel` stamps `left_at` (`mark_left`, conditional on
    the stamp being absent, so leaving twice writes and notifies once) and
    never deletes: the character keeps its row, its id, and its face, and
    an invited rejoin clears the stamp and gets that face back
    (`ensure_for_user`, whose `RunnerOrigin` is `Created`, `Returned`, or
    `Existing`, one statement per outcome). A stamp is also what keeps the
    directory honest, because the trigger fires on insert and update only:
    a delete would notify nobody and leave the undercity open on every
    replica that missed it.
  - `list_standing` serves only rows with `left_at IS NULL`, so one write
    closes the gate and drops the portrait everywhere. A runner who left
    disappears from the #deadchannel gutter retroactively, old messages
    included: going dark takes the face with it.

## Gotchas

- The look directory starts empty and fills on the listener's first
  load, so a portrait can be absent for the first seconds after a
  replica boots; a session copies the directory on its next 1 Hz tick.
  The `mark` is painted as the level badge in the #deadchannel author
  header and the heading of the profile's runner column; the clubhouse floor
  does not show it.
- Piece rows are five cells with no wide glyph; the state test guards
  that and nothing more. The rows are block, box-drawing, and shape
  glyphs (`◈ ◌ ●` and their kin), which are East Asian ambiguous width
  like the rest of the TUI's frames, so portraits assume the same
  ambiguous-narrow terminal the whole app does. Any new piece with a
  shape glyph should still be checked in the terminals people here use
  before it ships.
