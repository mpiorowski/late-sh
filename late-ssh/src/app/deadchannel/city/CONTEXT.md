# The night city (the undercity under `0`, the wallet; art first)

Part of Dead Channel (`../CONTEXT.md`; design in `../GAME.md`).

## Files

| File | Owns |
|---|---|
| `city/map.rs` | **Generated** by `scripts/gen_city_map.py --write` (never hand-edited): the 440x44 `MAP` literal, the `SOLID` collision bitmap, `SPAWN`, every zone (`SIGNS`, `BANNERS`, `CART_SIGNS`, `AWNINGS`, `WINDOWS`, `VENTS`, `PUDDLES`, `LAMPS`, `DROP_LIGHTS`, `SCREEN_FACE`, `WIRE`, ...), the closed `Neon` palette, `Landmark` + `nearest_landmark` (reach zones), `walkable`, `grid`/`char_at`. |
| `city/state.rs` | Per-session view state: the runner's cell, the animation clock, the open panel, the cursor on the armorer's wall (`picked_tier`, `pick_up` / `pick_down`), the cursor over INK's drafts (`picked_draft`, `pick_draft_up` / `pick_draft_down`), the ledge and the lean out over it (`arm_reset` / `disarm_reset` / `reset_armed`, only while looking over, cleared by every way back), the pinned street line. `walk`, `run`, `nearby`, `Landmark::on_enter` (`Enter::Panel` for shops and the blade shop, `Enter::Line` for the other carts and the closed shops, `Enter::Fight` at the screen, `Enter::Ledge` at the railing, `Enter::Leave` for the wire). Pure. |
| `city/landing.rs` | Night City's card in the Games hub (`door/hub`, runners only): an animated skyline in the city's palette (far and near towers, windows that flip, signs that short out and reflect in the wet street, a blinking antenna, rain, the monorail), NIGHT CITY in a five-row block font (one line when the pane is wide enough, stacked when not, each letter shorting out on its own clock), a billboard ticker, then the sheet (level, signal, pockets, today's road, glyphs, the glass, a waiting fight), the other runners on the street, and the keys down. Pure render over `LandingView` (sheet mirror, street view, `marquee_tick`); the hub repaints it on the ambience edge while it is selected (`hub::state::animates`). Also the sidebar row's neon styles. |
| `city/data.rs` | The city's copy and catalogs: the gear ladder (`COST_LADDER`, `WEAPONS`, `ARMOR`: LoGD numbers, `../GAME.md` names), `BANDS` with draft move names, `NOTICES`, `TAILOR_PRICES`, the per-landmark `lines` pools, `title` and `pitch`. |
| `city/input.rs` | `descend`: the way down (closes whatever was open, reloads the sheet, claims the first-descent guide, puts the runner on the street, `Screen::City`), called by `0` on the clubhouse and by Enter on the hub card; callers hold the runner gate. While the guide is open every key goes to `guide/input.rs`, and `?` anywhere on the page opens it. Arrows/hjkl walk; Enter at a landmark; `f` walks up to the static (the road, `FightSession::step_up`) and `p` opens patch from anywhere (the armorer, the tailor, the lockers, and the bits machine stay a walk away on purpose); Enter closes a panel or the ledge view, Esc too through the root's `dispatch_escape`, which on the bare street goes up to Home with #lounge selected (while one is open every typed key but the digits and `q` is swallowed: the walk keys, and the letters a global would spend). While the road or the scene is open every key goes to `fight/input.rs`. In the armorer's panel up/down walk the wall and `w` / `a` send `Command::Outfit` to `App.fight`; in patch's panel `p` sends `Command::Patch`; at the lockers `d` / `w` send `Deposit` / `Withdraw`; at the bits machine `b` / `r` send `Borrow` / `Repay`; at Dead Air `s` / `d` / `t` / `f` / `u` / `o` send `Command::Drink`; at the blade shop `w` / `a` send `Command::Cart`; at INK up/down walk the drafts and `i` sends `Command::Ink` for the picked one; over the ledge `r` leans out and a second `r` sends `Reset`, any other key leaning back in first. Returns `false` for globals. |
| `city/ui.rs` | Renderer: base styling by zone, the ambience pass (rain, puddles reflecting the nearest sign, neon shorts and dropped letters, window flicker, the screen's static and test pattern with rare glyph frames, steam, lamps, the drop's lights, the blimp, the mast, the bits machine, the wire's pulse), the runner as its mark, the popover, the street line, the shop panels (`armorer_lines` is the live till: the wall with the cursor, what you carry lit, the tiers under it dimmed, a row per key priced net of the trade-in; `bar_lines`: the crystals, the glass in you, each glass with what it does at this level and each pour with the card it puts in the deck, the refusal ahead of the keys; `ink_lines`: the crystals, the marks, the room the needle has left, a row per draft with its two cards inked or priced and the cursor on the picked one, the `i` key for it or the needle's refusal in that row; `cart_lines`: the next tier up from what each slot carries, priced in crystals, with what the wall asks in bits beside it; `patch_lines`: the signal, the price of the gap, `Sheet::patch_refused` spelled out in the key's row; `locker_lines`: on hand and locked up, the deposit priced net of the cut; `machine_lines`: owed against the cap, the loan and the repayment priced, the terms). Every panel is one size whatever happens at its counter (`draw_panel`): `PANEL_WIDTH` wide, its lines wrapped to that, each panel the same line count in every state (a refusal takes the row of the key it stands in for), and the counter's last word (`till`, or the tailor's) wrapped into `PANEL_WORD_ROWS` kept at the foot through `draw_framed`, which the ledge box uses too; on a screen too short for all of it the blank row over the word goes first, then `panel_lines` is handed the rows it is short by and the armorer's wall narrows to the tiers around the cursor, so the keys and the word stay on the screen; hands the sheet strip, the road, and the fight scene to `fight/ui.rs` (the strip drawn last and the two overlays given the area under its row, so the budget stays on screen through a fight), and the ledge (with the lean out and the till line, the box sized for the taller of its two states at the width it has) to `city/ledge.rs`. Its palette helpers (`ink`, `lit`, `glow`, `dim`, `action`, `scale`, `neon_rgb`, `tint_rgb`, the `INK_*` greys) are `pub(crate)` for that. |
| `street/state.rs` | Pure: `street_view`, the street derived from presence records (one `StreetRunner` per user: the latest mover's cell, present if any session is looking), and `StreetPresence`, the `App` slot: `descend` (on the street until the session ends), `sync` (the stand this session publishes; the move stamp moves only on a move), `leave`, and the derived `view` the renderer reads (`set_records` says whether it moved). Stands off this map are dropped here. The wire is `app/presence`. |
| `tailor/state.rs` | `Draft`, the mirror's editor over one `Look` at the runner's `peak_level`: four `Row`s (hood, eyes, coat, mark), `up` / `down`, `next` / `prev` around the row's unlocked rack (wrapping), `tint` around the unlocked tints (nothing on the mark row: a colored mark is earned), `shuffle` (the join's dice at the peak); `new` snaps a piece or tint the peak has not unlocked onto the rack's first entry, so the walks never meet one they cannot place. Pure. |
| `tailor/svc.rs` | `TailorService`, the look's writer after the join: `wear_task` runs `DeadchannelRunner::store_look` (one statement, standing runner only, last write wins) and answers `TailorOutcome::{Worn, NoRunner, Failed}` on the session's `mpsc`; the metric, the log line per outcome. The change trigger carries the look to every replica's directory. |
| `tailor/session.rs` | `TailorSession`: the `draft` while the panel is open, `worn` (what the row wears as far as this session knows), the tailor's `word`, one write in flight (`saving`); `open(runner)` / `close` / `changed` / `wear` / `tick`. Decides nothing. |
| `tailor/input.rs` | Keys while the tailor's panel is open: up/down (`k`/`j`) row, left/right (`h`/`l`) pick, `t` tint, `r` shuffle, `s` wear, Enter leaves; digits, Tab, `q` stay global (`?` is the guide's, taken in `city/input.rs` first), everything else is swallowed. |
| `guide/data.rs` | The undercity guide's copy: `SECTIONS`, a heading, its neon, and its `Block`s each (`Loop`, `Prize`, `Keys`, `Figures`, `Rule`), the short version first and built for the runner who reads nothing else (the loop as a chain, the prize boxed: 40,000 chips once every 30 days and the mark, the keys, the day's numbers, six bullets), then every key and rule of the street (with the lit shops, one line each), the sheet, the road (the map, the step down, the bright glyph, the rest, the cache), the crossing, the hand (the cards, the glyph's moves, static, the run, the `a` key), new cards, the Old Signal, the armorer, patch, the lockers, the bits machine, dead air, the blade shop, ink, the ledge, the tailor, the rest of the row, the lounge, and the wire. Prose marks `` `key` ``, `*strong*`, `‹card›` (drawn in the card's neon, marks kept; a name that is no card panics, and `ui_test` reads every line), and `{action}` (the action green, or a glyph's move in its color; braces dropped); nothing else uses a backtick, an asterisk, an angle mark, or a brace. Kept out of `app/help_modal` on purpose (that one is fed to the bot). **Always current**: see `city/CONTEXT.md`. Pure. |
| `guide/state.rs` | `State`: open, scroll, and the page the renderer last measured (`record_page`, a `Cell`), so `scroll_by` holds at the end. Pure. |
| `guide/svc.rs` | `GuideService`: `claim_first_descent_task` runs `DeadchannelRunner::mark_guide_seen` (one conditional update) and answers `GuideOutcome::{FirstDescent, SeenBefore}` on the session's `mpsc`; a failed claim answers nothing and logs. |
| `guide/session.rs` | `GuideSession`: the `state`, `descend` (the claim), `tick` (opens the guide on `FirstDescent`). Decides nothing. |
| `guide/input.rs` | Keys while the guide is open: `j`/`k` and the arrows scroll a line, PageUp/PageDown a screen, Enter, `q` and `?` close it; digits and Tab stay global, everything else is swallowed. `opens` names the key (`?`). |
| `guide/ui.rs` | `draw`: the box over the street in the city's palette, the copy's marks read through the fight's vocabulary (`city/CONTEXT.md`, "The vocabulary"), each section under a neon `▚ heading ───` rule, each block drawn its own way (the loop joined by arrows, the prize in a rounded amber box, keys as amber chips in as many columns as fit and one wrapped column when narrow, figures bright beside their meaning, rules as bullets with a hanging indent, the marks read into chips and bright spans), wrapped here through `common/markdown::wrap_spans` so the row count is the page the scroll holds against; the keys under; `body_lines`. Pure. |
| `tailor/ui.rs` | `mirror_lines` for the city's panel: the draft as a portrait with the mark under it, four rack rows beside it (the cursor, the label, the tint's name, a window of up to five pieces around the worn one, bracketed, the whole rack once while it is shorter; the mark alphabet on the mark row), the keys (`[s] wear it` lit only when the draft differs from what is worn), what the next unlock level opens. Pure; `city/ui.rs::draw_panel` puts the tailor's word at the panel's foot. |

../GAME.md, "The three surfaces": the city is a full-screen destination
where nothing happens that you could miss; transactions only. What exists
is the street and its doors, the screen (the fight, `../fight/CONTEXT.md`), and the open
counters (below, "The counters"); bands and the board are catalogs with
the till shut.

- **Where it is reached.** Under the clubhouse: `0` lands on the
  clubhouse, `0` again on the clubhouse goes down to the undercity, `0`
  on the undercity comes back up. The Games hub (page `3`) has a front
  door too: a Night City card first under "the house", above Lateania,
  whose Enter takes the same descent (`city/input.rs::descend`, the one
  function both doors call; the card is on the roster only for runners,
  `HubGame::roster`, so a non-runner can neither see nor select it).
  Runners only (`App::is_runner`: an
  entry in `App.runner_looks` for this user, so a `deadchannel_runners` row
  without a leave stamp; the app-wide gate for everything under the
  clubhouse, per user); anyone else stays on the clubhouse. The gate guards the descent,
  so the standing there is guarded on the 1 Hz edge in `tick.rs`: when the
  directory changes and this user is no longer in it, a session on
  `Screen::City` is walked back up to the clubhouse. That is the only
  place in the process that can notice a leave taken on another session or
  another replica. Not in the Tab cycle
  (`Screen::City.next()`/`prev()` return the clubhouse), no tab of its
  own, the clubhouse tab stays lit under it, title "Undercity" with
  `· f road · p patch · ? guide` beside it in the chrome. Enter at the wire goes
  back up to the clubhouse; Esc on the bare street (after it has closed
  whatever was open: the guide, a panel, the ledge, the scene) goes up
  to Home with #lounge selected instead, the chat rather than the
  tavern. The wiring is thin on purpose (`Screen::City`,
  `App.city`, `App.guide`, one dispatch line each in `input.rs`,
  `render.rs`, `tick.rs`).
- **The guide (`guide/`).** The street explains itself: a box over the
  city that opens on the short version (the whole game in a screen, for
  the runner who reads nothing else) and goes on to every key and every
  rule (the street, the sheet, the static, the armorer, patch, the
  tailor, the rest of the row, the wire). `?`
  opens it anywhere on the page, the fight scene and the tailor's panel
  included (the site guide's key, taken over down here; the site guide
  is a page away). Esc, Enter, `q`, `?` close it; `j`/`k`, the arrows,
  PageUp/PageDown scroll. It opens by itself on a runner's first descent,
  once per runner on any device or replica: the descent fires
  `GuideSession::descend`, the service's conditional update on
  `deadchannel_runners.guide_seen_at` (migration 203,
  `mark_guide_seen`) is the claim, and `FirstDescent` opens it a tick
  later over the street. Not in `app/help_modal`: that guide is fed to
  the bot, and this one is the runners' own. **Invariant: the guide is
  always current.** Every change to a key, a price, a rule, a counter,
  or a wire beat in this domain updates `guide/data.rs` in the same
  change, and `guide/ui_test.rs` renders every line, so a stale guide is
  a failed review, not a follow-up.
- **The vocabulary.** Every named thing is drawn one way on every screen
  (the panels, the road, the scene, the counter's word, the guide), and
  says what it does wherever there is a row for it. A card is `‹name›` in
  its neon (`fight/ui.rs::card_mark`; the machine's lines write
  `Card::marked`, and the renderers read the marks back with
  `card_marked`). An action is its verb in the `city/ui.rs::action` green
  right after its amber `[key]`; the step off the ledge alone is red,
  since it cannot be undone. A glyph's move is in `intent_style` (hits
  and heavy red, gathers amber, noise magenta) on the road, in the scene,
  and in the guide. A pour at Dead Air names its card and prints its
  rule; INK prints the picked draft's two cards, their rules, and what
  the pick replaces in rows that are always there, so the cursor changes
  what they say and never the panel's size. The hand's cards are their
  own frame and keep their plain names, and static as a count ("2 static
  cards") stays a word.

The counters, what each does, and where its contract lives (every one
a command on the sheet under the row lock; the guide says the same in
the runner's words):

| Counter (`Landmark`) | Keys | Paid in | What it does | Contract |
|---|---|---|---|---|
| the armorer (`Armorer`) | ↑↓, `w`, `a` | bits | the picked weapon or armor off the fifteen-tier wall, the carried piece taken in part-exchange | `../fight/CONTEXT.md`, "The armorer" |
| patch (`Repairs`) | `p`, from anywhere too | bits | the signal back to full, once a day | `../fight/CONTEXT.md`, "Patch" |
| the lockers (`Lockers`) | `d`, `w` | a tenth of each deposit | everything on hand in, all of it out free; a dropped signal never reaches it | `../fight/MONEY.md` |
| the bits machine (`Bits`) | `b`, `r` | a tenth on the loan | a loan up to a level piece's wall price, half of every kill and cache garnished until square | `../fight/MONEY.md` |
| Dead Air, the bar (`Bar`) | `s` `d` `t` glasses, `f` `u` `o` pours | a crystal, one a day | a glass: attack, defense, or max signal until the roll; a pour: a ‹surge›, ‹burn›, or ‹mute› in every deck dealt today | `../fight/CRYSTALS.md` |
| the blade shop (`Blades`) | `w`, `a` | crystals | the next tier up, nothing traded in | `../fight/CRYSTALS.md` |
| INK (`Ink`) | ↑↓, `i` | ten crystals | a draft's other card beside the pick, for good: kept through a mark and the ledge | `../fight/CRYSTALS.md` |
| the tailor (`Tailor`) | `tailor/input.rs` | nothing | the look, from the rack the peak level opened | below, "The tailor" |
| the ledge (`Ledge`) | `r`, `r` | everything but the marks, peak, inks, and debt | the reset to level 1, by choice | `../fight/MONEY.md` |

- **The register: tiles.** Top-down, one tile per
  thing, the Dwarf Fortress register. `#` walls, `+` doors, `╬` windows
  that flicker, `=` counters, `)` blades, `[` plate, `"` marks, `!`
  bottles, `%` bowls, `∩` lockers, `▬` beds, `▪` crates, `▓` shutters and
  cabinets, `≈` water, `@` people, `c` cats, `r` rats, `>` stairs, `*`
  lamps, `°` lanterns, `≡` grates that steam. Not front-on with
  multi-cell facades: that reads as a plaza. A 440-column street in three legs with a dogleg between
  each (the camera scrolls), four tiles wide, alleys one to three wide
  running off it (dead ends, a hidden court with a shrine and a plant, a
  back lane behind the second leg reached from its two end alleys and the
  arcade's back door), rooms you walk into, stalls of five tiles against
  the walls, a canal under the first two legs (a walkway, two bridges,
  warehouses on the far bank), the ledge with the railing and the wire
  stairs (`>` in the gap, the spawn) along the third leg, a small yard
  and the screen as three tiles of static closing the street. **Shops
  are where the game is played; the street is flavor.** Everything that
  spends or earns happens inside a building with a lit sign or at the
  screen; stalls, carts, and the reader only talk. The open shops are
  laid out by how often a runner needs them: the armorer and the blade
  shop side by side in the middle of the street with patch and Dead Air
  just west of them, the lockers beside the wire stairs where a runner
  arrives, loans (the bits machine in its corner) further east toward
  the screen, the tailor at the west end and INK a short walk east of
  it. An open shop is `shop='Name'` in the generator: its reach is its
  whole floor, every cell inside its walls (behind the counter, at the
  back, in a back room) and none outside them, so the popover holds
  anywhere in the shop and never shows from the street; `map_test`
  walks every floor cell to keep it so. A shop that does nothing yet
  is `closed='Name'` in the generator: its sign is in `map::DARK_SIGNS`
  (its own color at `DARK_SIGN`, throwing no light), its walls and door
  take no neon. Today that is MARKET, SHRINE, PAWN, BATHS, BANDS
  (its panel still opens as a preview), SLEEP, COIN, VIDS. Every one
  is already a `Landmark` (the flag is its name): its reach is its
  front doors, its popover says `data::CLOSED_PITCH`, and Enter answers
  `data::CLOSED_LINES`, the same for all. Opening one is three moves:
  drop the flag and name it a `shop` in the generator, move the variant to
  `Enter::Panel` in `city/state.rs`, and give it lines in
  `city/ui.rs`; the exhaustive matches name every other place to
  touch. The rest is there to be there: tenements
  (`tenement()` lays out corridor, rooms, beds and a sleeper from a
  seed), a lockup, a garage, a dock, a chop shop. **Walkers** (`map::WALKERS`,
  `ui::walkers`): people, cats and rats pacing a stretch of floor as a
  pure function of the tick, no state; the generator and `map_test`
  prove every path is open floor. Rule: every shop with a sign has a
  door somewhere in its walls (the generator refuses a signed shop
  without one); what happens inside can be shuffled or removed later.
- **Light.** Blade Runner, not cyberpunk: the street is
  dark and every color has a source. `map::LIGHTS` is every light on the
  street, found by the generator scanning the finished grid (a `*` is a
  lamp, a `°` a lantern, `$` `♪` `?` machines, `_` candles, `>` stairs)
  plus the signs, the shops' doorways, the windows and the screen, each
  with a kind, a `Neon` and a radius. Each frame `ui::Scene` spreads
  every light over the floor it reaches (Dial's buckets: a column costs
  one, a row two, light crosses open ground and doorways, lands on walls
  and stops) at the level `light_level` gives it this tick (lamps
  flicker, signs short out and their light with them, windows go dark
  for a while, the screen pulses with its static), and adds the
  spinner's searchlight passing over. A **visibility map** fades
  everything with distance from the runner (`SEE_FULL` columns in full,
  black-ish by `SEE_END`) and keeps a room at `INSIDE_DARK` until the
  runner is at its door. Every cell is a `Surface`: lit (its color times
  ambient plus the light on it, wet things more, halved in the shadow
  under a wall) or emissive (neon, lamps, windows: they burn on their
  own and only fade with distance). Height is faked: the top wall of a
  building against the dark draws as `▀`, the floor south of any wall is
  in shadow. Rain takes the color of the light it falls through, in that
  light's color. The runner carries a light (`CARRY_RADIUS`): where you
  stand is always the best lit place on the street. Light crosses flat
  water, so the canal and the puddles take the bank's lanterns. Every
  sign smears its color a few rows into the wet ground in front of it
  (`reflections`, shimmering). The fixed lights' footprints are computed
  once (`footprints`), as is every cell's base surface (`base_map`) and
  which room it is in (`inside_map`): a debug frame is ~30ms, not 130.
- **Traffic, billboards, splashes.** All pure in the tick. A car runs the
  length of the street (`car_route`: the three legs through both
  doglegs, `ui_test` proves every cell is open) as two cells, tail red,
  head white, with the pool of its headlights running ahead of it in the
  light map; it draws only on open floor, so it passes behind whatever
  is in the road. The monorail crosses the sky row (`TRACK_Y`, a dashed
  track across the top of the map), eastbound one run and westbound the
  next, windows amber and cyan, flickering. `map::BILLBOARDS` are nine
  one-row strips the generator leaves blank (five over the rooftops in
  the sky row, four on the towers below the ledge); the renderer edges
  each in dark steel and scrolls a line of street copy across it in the
  board's neon (`BILLBOARD_LINES`, one line per `BILLBOARD_CYCLE`, dark
  for a moment between, a letter flickering); each is a
  `LightKind::Billboard` in the light map. Not the glyph script: two rows
  of it read as floating blocks of noise. Raindrops
  landing on a puddle throw an `o`. Rain falls in two columns of three
  on every bare cell under the sky, the street, the drop and the void
  between the blocks alike, dim where the light is dim, and never inside
  a room (`inside_map`) or on a prop.
- **The ledge (`Landmark::Ledge`, `city/ledge.rs`).** Standing at the
  railing anywhere along it (one row north of `RAIL_Y`, not on the wire
  stairs) the popover says "look over"; Enter swaps the street for the
  lower city, all the way down: a perspective picture at half-block
  resolution (two colors per cell, `▀` with fg and bg), towers in four
  depths from the far hazed ones on the horizon to the near black ones
  standing below the frame, windows lit at random and flickering, the
  city's script in neon bands across the near towers, antenna lights
  blinking, the spinner's beam crossing, rain in the sky. Pure in the
  area's size and the tick; the same size always draws the same city.
  Enter or Esc steps back. `State::at_ledge` gates the walk keys like a
  panel does. The ledge is also the reset (`../fight/MONEY.md`): `r` leans
  out, the box bottom-right turns red and says what the fall takes and
  keeps, a second `r` steps off, any other key leans back in.
- **Own palette, not the theme.** The city does not follow the person's
  theme at all: one look, tuned once (`ui::NIGHT` painted under every
  cell and overlay, `ui::neon_rgb`, the surface constants, the `INK_*`
  greys of the overlay text, all fixed RGB). A hundred palettes cannot
  all be lit well, and a light canvas showing through the street breaks
  the night. Nothing in `city/ui.rs` reads the theme module; the mirror
  in the tailor's panel recolors the tints through `ui::tint_rgb`.
- **Running** (`State::run`, Shift+arrow or `HJKL`) goes up to
  `RUN_STEPS`, stops at anything solid and at the first landmark that
  comes within reach that is not the one you set off from: the railing
  runs the length of the ledge, so a run along it must not stop every
  step.
- **The runner** is its mark (../GAME.md, "The look"; `@` for a session
  without a runner row), name label above. Single-width glyphs
  only; the generator refuses wide and combining characters and
  `map_test` asserts it again.
- **Landmarks.** Enter at a shop (armorer, tailor, lockers, bands, Dead
  Air, patch, board, bits machine, INK) or at the blade shop opens a
  centered panel. The armorer's till, patch, the lockers, the bits machine
  (`../fight/CONTEXT.md`, `../fight/MONEY.md`), Dead Air, the blade shop, and INK
  (`../fight/CRYSTALS.md`) are open and the tailor's mirror edits (below);
  the others say their till is not: bands shows the three bands with
  draft move names. Enter at another cart, the reader, or the stairs pins
  a line from that landmark's pool top-left for ~8s. Enter at the wire
  leaves, back up to the Clubhouse; Esc with nothing open goes up to
  Home with #lounge selected.
- **The tailor.** Pick, not draw (../GAME.md, "The look"): Enter at the
  tailor opens the panel with the runner's look and level from the
  directory (`App.runner_looks`; with no entry yet the mirror says nothing
  looks back) as a `Draft` in `App.tailor`. The city's panel is the frame, every
  key goes to `tailor/input.rs`: the cursor walks the four rows, left
  and right walk the row's rack (a window of up to five around the worn
  piece, wrapping, the whole rack while it is shorter; the ten marks all
  at once), `t` cycles the piece's tint, `r`
  rolls the join's dice again, `s` wears it. Wearing is one write
  (`TailorService::wear_task`, `store_look`: the standing runner only, no
  lock, last write wins, since a look is one value), and the row's change
  trigger carries the new face to every replica's look directory, so the
  gutter portraits, the mark on the street, and the fight scene all
  change on the next directory refresh; the panel's own mirror shows the
  draft at once. Enter or Esc drops a draft not worn. The rack is gated
  by the peak level and free once unlocked: the draft only walks pieces
  and tints at or under the runner's `peak_level` (from the directory
  entry), and the panel names what the next unlock level opens. The
  draft is the gate, with no second check at `wear`: the peak only climbs
  (an Old Signal reset takes the level, never the peak), and a look that
  misses the rack (one written by an older replica mid-deploy) is snapped
  onto it by `Draft::new` at open, so the draft's walks never meet a piece
  they cannot place. The ownership check for bought and earned pieces lands
  with them.
- **Animation** rides the clubhouse's `anim_half` edge (~7.5fps,
  `tick.rs`), the wake tier is `ANIM_HALF_TICK` on this screen, and every
  effect is a pure function of `marquee_tick` and the cell, so nothing
  accumulates. The ambience never paints over a prop (`put_if_floor`, and
  `ui_test` checks the signs and the board survive a frame).
- **The camera looks north** (`LOOK_NORTH`, 8 rows): it centers above the
  runner so a 24-row terminal at the spawn shows shopfronts and street,
  not the drop.
- **The street is shared (`street/`).** Every runner who has gone down
  this session stands on it, on every replica, until the session ends:
  the first descent puts the runner on the street (`StreetPresence::descend`)
  and it stays there, lit while the session is on the page and dim while
  it is on another (`present`), standing where it was left. A logout
  (presence drops the session's record) or losing the runner (the directory edge in `tick.rs`) takes
  it off. One runner per user: two sessions show the latest mover's cell.
  Nothing is persisted: a fresh login starts at the stairs. Another
  runner who is looking carries a light like yours (`CARRIERS_MAX`
  nearest are spread per frame); one who is away has none and shows only
  under the street's own light, its name barely there. Runners do not
  collide, and a panel, the ledge or the fight are the session's own:
  others see you standing there. A stand off this map (a replica on
  another map, mid-deploy) is dropped in `street_view`, the boundary, so
  the renderer never indexes a cell it does not have.
  - **The wire** is presence (`app/presence`, `late-ssh/CONTEXT.md`): the
    street stand rides this session's presence record next to its tavern
    seat and its Nightcap stool, batched per replica per flush, heartbeat
    and timeout included. `App::sync_presence` syncs the stand and
    publishes it on every tick (a send only on a change) and derives the
    view from new records; a city frame is bought only when the street
    view itself changed, never for a step in the tavern. Another replica
    sees a step within about a flush and a frame; your own runner is
    drawn from `city::State` and never waits on the wire.
