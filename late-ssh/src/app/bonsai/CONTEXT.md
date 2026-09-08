# Bonsai Context

## Metadata
- Scope: `late-ssh/src/app/bonsai`
- Last updated: 2026-09-08 (one Bonsai for everyone: the living branch graph is the only bonsai. The classic stage ladder, its daily care rows, the graveyard, the `dynamic_bonsai` shop unlock, and the `bonsai_variant` slot are gone (migration 177), and every tree was reset to a fresh seed rather than migrated. `bonsai_v2` took the plain `bonsai` name in code and in the database. Same day, earlier: the care modal is the pot (a fixed `CANVAS_WIDTH` 81 by `CANVAS_HEIGHT` 26 canvas drawn 1:1), growth is bounded by that canvas and shaped by length budgets per branch order, interior segments and leaf pads back-bud so a tended tree is never finished, the branch cap is 128, and the sidebar and profile draw one fixed 21x13 preview of the canvas that sways on the wall tick. Admins temporarily bypass the daily watering gate to test growth.)
- Purpose: local working context for the bonsai branch-graph system.
- Status: Live for every account, planted at first login.
- Parent context: `../../../../CONTEXT.md`

The user-facing name is "Bonsai". Nothing says "Dynamic" or "classic" any more; the words only appear here as history.

---

## 1. Scope

The bonsai is a persistent branch graph that every account owns:

```text
seed + persistent branch graph + vigor/stress/care actions -> rendered ASCII tree
```

The tree is not a finite ladder of predefined pictures. The visible structure is a persistent record of player decisions: watering, wiring, pruning, pinching, stress, recovery, and future growth.

History: this shipped 2026-07 as "Dynamic Bonsai", a 1000-chip shop unlock that replaced the classic stage-ladder tree for whoever equipped it, with a two-way watering bridge keeping both trees fed. On 2026-09-08 the owner decided to run one tree only and to reset everyone rather than migrate growth points into graphs: a seeded 700-point classic tree would have landed three badge rungs below Blossom anyway, and seeding it large enough to keep the badge would have filled the pot on day one.

---

## 2. File Map

```text
late-ssh/src/app/bonsai/
|-- mod.rs              # Module declarations only
|-- state.rs            # Persistent branch graph, growth simulation, care actions, badge scoring
|-- render.rs           # The graph renderer (modal canvas), the fitted preview (sidebar, profile), the sway
|-- modal_ui.rs         # Care workbench modal
|-- modal_input.rs      # Modal key handling
|-- svc.rs              # Persistence, the daily chip payout, activity events
`-- CONTEXT.md          # This file
```

Related files:

```text
late-core/migrations/056_create_bonsai_v2.sql     # the table, created under its old name
late-core/migrations/177_one_bonsai_for_everyone.sql
late-core/src/models/bonsai.rs                    # `Tree` on `bonsai_trees`
late-core/src/models/bonsai_decay_protection.rs   # the Bonsai Decay Shield window
late-core/src/models/user.rs                      # the chat badge join
late-ssh/src/app/common/sidebar.rs
late-ssh/src/app/profile_modal/
late-ssh/src/app/render.rs
late-ssh/src/app/input.rs
late-ssh/src/app/tick.rs
late-ssh/src/session_bootstrap.rs
late-ssh/src/ssh.rs
late-ssh/src/app/chat/svc.rs
```

---

## 3. Current Architecture

Persistence:
- Table: `bonsai_trees` (created as `bonsai_v2_trees` by migration 056, renamed by 177). One row per user.
- Stores `seed`, `last_watered`, `is_alive`, `vigor`, `water_stress`, `last_simulated_date`, `branch_graph` JSONB, `selected_branch_id`, `mode`, and precomputed `badge_glyph`. Also stores `planted_at` and `state_revision`; saves increment `state_revision` and the DB upsert ignores stale async writes with `WHERE bonsai_trees.state_revision < EXCLUDED.state_revision`.
- `BonsaiService::ensure_tree` runs at every session bootstrap: it plants the bare root (`seeded_graph`, badge `·`) for a user with no row and returns the existing row untouched otherwise, together with the live Bonsai Decay Shield window. There is no other creation path.

Session state:
- `App::bonsai_state` is the one tree. Global `w` opens the care modal (`open_bonsai_modal_globally`) when not composing; `Ctrl+B` does nothing.
- `BonsaiState::new` runs `apply_elapsed_days`, which applies dry-day decay and death on real calendar dates. There is no in-session tick: growth comes from watering and from the daily catch-up only, and a death is only detected at the next login (the service then fires `ActivityKind::BonsaiLost`, which stays private).
- Watering is once per UTC day. `BonsaiState::water` refuses a second press with "Already watered today"; the first press moves the meters, runs a growth wave, and persists through `persist_watering` rather than the plain `persist`. **Temporary (2026-09-08): admins bypass the in-memory gate** (`DailyWaterGate::AdminBypass`, chosen by `modal_input::daily_water_gate` from `App::is_admin`) so growth waves can be tested without waiting a day. Remove it once the growth rules have settled.
- The 200 chips: `BonsaiService::water_task` runs `Tree::water_day` (an `UPDATE ... WHERE last_watered IS DISTINCT FROM today RETURNING`, the one atomic witness of the first watering of the day, which never bumps `state_revision`), then `Tree::save`, then `UserChips::apply(ChipMove::BonsaiWatered)` and the `BonsaiWatered` activity event only when the gate said yes. Gate before save, in one task, so the save that stamps the same date can never starve the gate. The admin bypass therefore waters again but pays again never.
- The Bonsai Decay Shield (`bonsai_decay_shield_two_weeks`, a `bonsai_consumable`) is read into `decay_protection` at construction and refreshed from the shop snapshot on tick; `simulate_day` skips the stress and vigor cost on a covered day. It only matters from the next login's catch-up onward.

Rendering:
- One plotter, `plot_tree`, puts the graph's cells (branches, then leaf pads, a pad drawn whether or not a shoot has budded out of it) into a grid with the trunk base above the last row at the center column. `render_ascii` adds the pot and is the true render; `canvas_lines` is that at `CANVAS_WIDTH` x `CANVAS_HEIGHT`, which the care modal (with selection highlighting, `center_lines` leading it inside the wider frame) and the share snippet draw 1:1. The growth rules keep every tip (and its leaf pad reach) inside the canvas, so the modal never cuts anything off.
- The sidebar panel (`draw_bonsai_inline`) and the profile hero draw `render_preview_lines`: the true canvas fitted into one fixed block, `PREVIEW_WIDTH` 21 by `PREVIEW_HEIGHT` 13 (the sidebar's width; the profile centers the same block in its hero), by `render_preview_ascii`. It never invents anything. When the tree fits, it is the modal's own glyphs, trimmed around the trunk. When it does not, one integer scale factor is applied to both axes so the block keeps the modal's proportions (a wide tree comes out squat, never tall and thin); bare rows (structure only, no foliage, nothing mid-pinch) are dropped from the pot upward, the trunk base always kept, only as far as needed to stop the height forcing a larger factor than the width already does. A preview cell that gathered one sample keeps that sample's glyph; several samples resolve to the dominant kind, foliage as density glyphs (`@` / `*` / `#` by count), structure as its commonest glyph. The preview pot is `[=====]`. `BONSAI_MIN_HEIGHT` in the sidebar is the block plus the footer row.
- The modal, the sidebar, and the profile hero all apply `apply_sway` off the wall tick and ride the `ANIM_HALF_TICK` tier. The profile modal takes `wall_tick` in `draw`, and `tick.rs` marks it changed on the `anim_half` edge while it shows a tree (`ProfileModalState::bonsai()`).
- The care modal is sized to the canvas (`CANVAS_WIDTH + 12` by `CANVAS_HEIGHT + 7`): the tree, a blank row, two status rows, a two-row key footer. The status row appends "full: cut to make room" at the branch cap.
- Profile views use `BonsaiState::view_only`, which applies elapsed-day catch-up in memory for rendering but never persists to the viewed user's row.
- Child branches do not redraw their parent joint cell; only root segments draw their starting cell. This keeps one-cell graph segments from visually collapsing into uneven long ASCII runs.

Chat badge:
- `bonsai_trees.badge_glyph` is joined in `User::list_chat_author_metadata` as `bonsai_badge_glyph`; `chat/svc.rs` shows it when non-empty. No row yet (a user who has not logged in since migration 177) or an empty glyph (a dead tree) shows nothing.
- `BonsaiState::new` refreshes and persists `badge_glyph` when the current score ladder would compute a different glyph, so loaded trees migrate across badge-threshold changes without requiring a care action.

---

## 4. Branch Graph Model

`BonsaiGraph` stores:

```text
version
next_id
branches: Vec<Branch>
```

`Branch` stores:

```text
id
parent_id
start_x/start_y
end_x/end_y
thickness
age
vigor
status
bend_x/bend_y
last_pruned_day
ramification
last_pinched_age
```

Statuses:
- `Growing`: normal live branch/tip.
- `Wired`: live branch/tip with remembered directional bias.
- `Pinched`: compact branch that was just pinched and will not grow.
- `NeedsPinch`: compact branch ready for the next pinch step.
- `LeafPad`: terminal growth converted into compact foliage.
- `Cut`: legacy pruned segment; new cuts remove segments instead of leaving scars.
- `Deadwood`: dead retained structure.

Important concept: user actions should affect future geometry, not only the current frame. Wiring sets bend memory. Cutting removes the selected branch and descendants. Pinching marks the selected tip as compact growth; it must be pinched three times over separate growth moments to become a leaf pad, and pinched branches do not keep extending. Splitting marks the selected tip for the next growth wave; it forks only if both target cells are open.

Branches are stored as one-cell growth segments. Growth adds a new child segment instead of extending the selected branch endpoint, so selecting/cutting a branch id targets that exact segment and descendants downstream from it.

Branch cap: `MAX_BRANCHES = 128` (`state.rs`, raised from 96 when back-budding arrived). Every branch-adding path checks it independently, defense-in-depth rather than one gate: `BonsaiGraph::add_branch`, `grow_tips_once` and `grow_tip_once` before spending a growth-wave slot, the side-shoot spawn, `split_tip_once` (`+2` since a split needs two new branches), `bud_once`, and the graph-normalize/migration path. At the cap growth silently adds nothing, and the care modal's status row says "full: cut to make room" (`BonsaiV2State::is_full`) so a still tree reads as a prompt, not a bug; steering, pinching, cutting, and splitting still work, and any cut reopens growth. `grow_graph_once` has no early return at the cap on purpose: the wave still ages branches and turns a set pinch into `NeedsPinch`, so pinching keeps cycling on a full tree (it once froze there, because the cap check sat above that bookkeeping). Deadwood keeps its cells: it is not selectable and not cuttable today. The chat badge score is presence-derived from the graph (branch length plus leaf-pad weight, see section 7), so it is bounded by this same cap.

---

## 5. Simulation

Main state values:
- `vigor`: overall growth strength.
- `water_stress`: dry/neglect pressure.
- `last_simulated_date`: UTC date used to catch up elapsed daily growth.
- `last_watered`: UTC daily watering gate.

The pot (2026-09-08):
- Every growth target must satisfy `target_in_canvas`: `|x| <= TIP_MAX_ABS_X` (38) and `1 <= y <= TIP_MAX_Y` (23), checked in `growth_target_is_open`, the one choke point every branch-adding path goes through. A leaf pad reaches 2 cells sideways and 1 up, hence the margin inside the 81 x 26 canvas, which is wider than tall because a bonsai spreads and a cell is twice as tall as wide. The canvas is the care modal's tree area, so the restriction is the frame the player is looking at.
- Length budgets (`run_budget` by `order_and_run`): the trunk and first arms run 4 single-file cells, the next two orders 3, deeper 2. A tip at its budget forks (`split_tip_once`) when a fork is open; when it is not, it extends one cell as usual and is asked to fork again next wave, so a crowded crown creeps toward the canvas edge rather than dying in place (the canvas bound and the branch cap are the only hard stops). A wired tip (status `Wired`) extends where it was told, once: the wire is spent on that extension (the tip returns to `Growing`), and the new segment inherits the bend as a lean for one more cell (its own child gets no bend), so a steer is a two-cell bend that then forks on budget like any other tip. Before 2026-09-08 children inherited `Wired`, which turned a steered arm into an unforking pole. `split_candidates` draws each fork's two arms from a seeded roll, one arm per side: the trunk's fork is mostly both-rising with the odd level arm; deeper forks mix both rising, one level, both level, and occasionally one drooping arm, so a crown is not a ladder of `\ /`.
- `trunk_step`: the trunk goes straight, leans one cell to the seed's side, then straight, and forks at four cells.
- `natural_steps`: a free tip (not wired, not the trunk) meanders around its heading with a seeded roll per step: keep going, one notch toward level, one notch toward up, and at order 4+ sometimes a droop. Young orders mostly climb, deeper orders drift level and hang. The rolled step comes first and the others follow as fallbacks when a cell is taken, so a taken cell costs the wave nothing. The other diagonal is the last fallback. `candidate_steps` is the one list of steps a tip would take (wire, trunk, or meander), and `tip_can_grow` checks exactly that list, so the wave filter, the bud gate, and the growth itself can never disagree (they did once: the filter accepted a diagonal the meander never tried, the oldest such tip was picked every wave, failed every wave, and starved the rest until the player selected a real tip). `tip_parked`: a tip against the ceiling or a side wall grows nothing (a wire can still pull it back in), does not hold the bud gate, and the tree's energy goes to buds, which prefer room. Nothing slides or climbs along the pot.
- The bud gate counts only shoots that can still grow (`tip_can_grow`: the straight step or any level or rising step has an open cell), so tips parked against the pot or wedged in the crown never hold the gate shut. Bud sites are ordered by `open_run`, the empty level run the shoot would face on its side, so new arms go toward empty space rather than into the crown.
- Back-budding (`bud_once`, run at the end of every growth wave, including waves with no growing tip at all): two kinds of site at `end_y >= 2`. A live interior segment carrying exactly one live child throws a shoot leaning away from that child. A leaf pad with no shoot yet throws one out of its foliage (the renderer keeps drawing the pad; `plot_leaf_pad` no longer requires the pad to be a tip). Watering buds one site, a plain day one on a one-in-four roll, a dry day none; vigor under 40 or stress 60+ buds nothing, and neither does a tree with `MAX_OPEN_SHOOTS` (4) growing or wired tips still open off the trunk: tend the shoots you have and the tree offers more, which is what keeps an admin-watered tree from outrunning anyone's pinching. This is what puts foliage at every height and keeps a pinched-out tree in play: pinch the shoot three times and the pad has moved outward by a cell, denser.
- `repot_into_canvas` runs in `new` and `view_only`: branches whose end lies outside the canvas are cut with their descendants (trunk untouched); `new` persists and says "Repotted: cut back N glyphs to fit the new pot". Trees planted before the canvas pay this once.

Growth paths:
- Daily catch-up happens in `BonsaiState::new` via `apply_elapsed_days(today)`.
- Watering grants vigor, reduces stress, and triggers extra growth attempts. There is no passive in-session growth (removed 2026-07-23).
- Dry elapsed days increase stress, reduce vigor, and can create wild growth or deadwood.
- Each growth event is a small wave, not a single tip: split-marked tips resolve first, then the selected tip, then the other live tips in rotation (`growth_tip_order`): the tip that has waited longest since it appeared (highest `age`) goes first, tips with nowhere to grow (`tip_can_grow`) are skipped so they never waste a slot, and the tiebreak hash is salted with `graph.next_id` so repeated waterings on one day never pick the same favourites. Water/high vigor grows the broadest wave; stress can narrow it.

Per-day rates (`simulate_day`):
- Dry day: `water_stress += 11` (clamp 0..120), `vigor -= 7` (floor 0).
- Watered day: `water_stress -= 4` (floor 0), `vigor += 2` (cap 100).
- Watering action (`water`): `water_stress -= 35` (floor 0), `vigor += 18` (cap 100), plus a growth wave.

Current death model:
- If `water_stress >= 100` and `vigor == 0`, the tree is marked dead and weak tips become deadwood.
- This is intentionally less binary than the old classic tree, where death was a flat 7-dry-day cutoff.
- Survival without watering: `water_stress` crosses 100 by ~dry day 10, so `vigor == 0` is the gate. Vigor reaches 0 after `ceil(vigor / 7)` dry days, giving a death window of about 10 dry days from a fresh plant (vigor 70) up to 15 dry days from full health (vigor 100). Compare classic Bonsai, which dies at exactly 7 dry days.

---

## 6. Input Model

Modal keys:

```text
w          water or replant if dead
tab / n    select next live branch
shift-tab  select previous live branch
wheel      scroll-select previous/next live branch
←↓↑→ / hjkl steer selected tip's future growth
x          prune selected branch
p          pinch selected tip toward a leaf pad; needs 3 pinches over time
s          split selected tip on next growth if both target cells are open
c          copy share snippet
?          open Bonsai help
q / Esc    close
```

Current interaction limitations:
- Selection is branch-cycle based, not cursor/mouse picking; mouse wheel only cycles selection.
- Wiring records future growth bias; it does not instantly extend the branch.
- Pruning the trunk is intentionally blocked in the prototype.
- If the tree is dead, the first `w` replants and returns; a later `w` waters.
- Foliage is earned: pinch a tip, wait for it to become ready again, and repeat until the third pinch turns it into a leaf pad.
- Splits are explicit: `s` marks a tip, and the next growth wave forks it only when both split target cells are unoccupied. High stress can still create messier random side shoots.

---

## 7. Badge Scoring

Badge intent: keep the familiar bonsai badge meaning "this person is invested here", but derive it from actual rendered/tree presence instead of a points ladder.

Current implementation:
- Computes graph presence from live branch length plus leaf-pad weight.
- Applies a health/stress multiplier.
- Maps score to the familiar glyph ladder:

```text
0-16      ·
17-40     sprout
41-80     sapling
81-150    pine
151-240   tree
241-360   blossom
361+      flower
```

These thresholds are doubled from the original prototype ladder; the final flower badge now requires a 361+ health-adjusted graph-presence score.

Dead trees return an empty badge.

Important invariant: a huge neglected mess should not automatically be prestigious. Health/stress must keep mattering.

---

## 8. Critical Invariants

- `mod.rs` stays declaration-only.
- Rendering comes from graph state, never from static ASCII stage templates.
- Persist mutations after user-visible graph/state changes. A watering persists through `persist_watering` so the chips ride the DB gate; every other action uses `persist`.
- The daily chips are paid in exactly one place, `BonsaiService::water`, behind `Tree::water_day`. No other path credits `ChipMove::BonsaiWatered`.
- Badge metadata must stay cheap for chat; use the persisted `badge_glyph`, not per-message graph rendering.
- The renderer must tolerate narrow/sidebar areas without panics: out-of-grid cells are dropped, never indexed.
- The care modal shows the whole canvas 1:1, always; the growth rules guarantee it fits. Small surfaces show the preview, which reads the true canvas and never invents cells: when it must shrink, drop bare rows first, then scale each axis by its own integer factor. No crop, no camera.
- Unit tests for `state.rs` and `render.rs` stay pure logic/rendering tests; `svc_test.rs` is the DB-backed slice (planting, the chip gate).

---

## 9. Current Rough Edges

- Renderer is functional, not final art.
- Branch geometry is simple and can create awkward silhouettes.
- No mouse branch picking.
- No seasonal cycles, flowering schedule, scar aging, root work, or repot mechanics yet.
- The budgets, bud rates, and canvas size are first guesses, tuned by feel.
- Deadwood cannot be selected or cut, so its cells are lost for the life of the tree. A jin-removal action (cut deadwood) is the obvious next care verb.
- `branch_graph` JSON has `version`, but no migration/upgrade path exists yet.

---

## 10. Desired Direction

The interesting version is a small horticulture sim, not a cosmetic randomizer:

- Let branches compete for vigor.
- Let neglected growth become recoverable-but-ugly before death.
- Make pruning create deadwood and back-budding without noisy scar glyphs.
- Make wiring affect future growth more than instant shape.
- Make leaf pads emerge from terminal tips and pinching history.
- Add seasonal overlays as renderer texture, not separate templates.
- Cut back to the fork: make `x` walk toward the trunk while each segment has a single live child, so a long single-file run goes in one press.
