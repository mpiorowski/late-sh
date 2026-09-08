# Dynamic Bonsai Context

## Metadata
- Scope: `late-ssh/src/app/bonsai_v2`
- Last updated: 2026-09-08 (the tree now lives in one fixed canvas, `CANVAS_WIDTH` 21 by `CANVAS_HEIGHT` 13, that the modal, the sidebar panel, the profile hero, and the share snippet all draw 1:1; the compact preview renderer is gone. Growth is bounded by the canvas and shaped by length budgets per branch order (fork over extend), the trunk takes one lean, live interior segments back-bud, and trees from before the canvas are repotted once at load. Admins temporarily bypass the daily watering gate to test it.)
- Purpose: local working context for the Dynamic Bonsai branch-graph system.
- Status: Active prototype, unlocked and selected through the `dynamic_bonsai` shop item.
- Parent context: `../../../../CONTEXT.md`

Internal code and database names still use `bonsai_v2`/`BonsaiV2*`. User-facing surfaces should say "Dynamic Bonsai".

---

## 1. Scope

Dynamic Bonsai is the experimental replacement path for the old static-stage bonsai renderer. It is selected through the Shop item `dynamic_bonsai`; classic Bonsai remains the default unless that item is equipped in the `bonsai_variant` slot.

The core idea is:

```text
seed + persistent branch graph + vigor/stress/care actions -> rendered ASCII tree
```

The tree should not be a finite ladder of predefined pictures. The visible structure should be a persistent record of player decisions: watering, wiring, pruning, pinching, stress, recovery, and future growth.

This is not final polish. It is an end-to-end dynamic prototype with real persistence, shop selection, sidebar panel, modal, input, rendering, growth, and badge plumbing.

---

## 2. File Map

```text
late-ssh/src/app/bonsai_v2/
|-- mod.rs              # Module declarations only
|-- state.rs            # Persistent branch graph, growth simulation, care actions, badge scoring
|-- render.rs           # The one graph renderer (modal, sidebar, profile) plus the sway
|-- modal_ui.rs         # Dynamic Bonsai care workbench modal
|-- modal_input.rs      # Modal key handling and classic Bonsai water/chip compatibility bridge
`-- CONTEXT.md          # This file
```

Related files:

```text
late-core/migrations/056_create_bonsai_v2.sql
late-core/migrations/067_seed_dynamic_bonsai.sql
late-core/src/models/bonsai.rs
late-core/src/models/marketplace.rs
late-core/src/models/user.rs
late-ssh/src/app/bonsai/svc.rs
late-ssh/src/app/common/sidebar.rs
late-ssh/src/app/hub/shop/
late-ssh/src/app/render.rs
late-ssh/src/app/input.rs
late-ssh/src/app/tick.rs
late-ssh/src/session_bootstrap.rs
late-ssh/src/ssh.rs
late-ssh/src/app/chat/svc.rs
```

---

## 3. Current Architecture

Shop selection:
- Catalog seed: `067_seed_dynamic_bonsai.sql`.
- SKU: `dynamic_bonsai`.
- Price: 1000 chips.
- Slot: `bonsai_variant`.
- Buying auto-equips the item through existing marketplace slot behavior.
- Pressing Enter on the owned/equipped item clears the slot and returns the user to classic Bonsai.

Persistence:
- Table: `bonsai_v2_trees`.
- One row per user.
- Stores `seed`, `last_watered`, `is_alive`, `vigor`, `water_stress`, `last_simulated_date`, `branch_graph` JSONB, `selected_branch_id`, `mode`, and precomputed `badge_glyph`. Also stores `planted_at` and `state_revision`; saves increment `state_revision` and the DB upsert ignores stale async writes with `WHERE bonsai_v2_trees.state_revision < EXCLUDED.state_revision`.
- Rows are loaded/created for users who own Dynamic Bonsai during session bootstrap. `BonsaiV2Tree::save` upserts, so fallback state can persist after a user buys the item mid-session.

Session state:
- `App` always has `bonsai_v2_state`, but classic Bonsai remains visible unless Dynamic Bonsai is equipped.
- `App::use_bonsai_v2()` follows `ShopState::dynamic_bonsai_enabled()`.
- Global `w` opens Dynamic Bonsai when selected; otherwise it opens classic Bonsai.
- Global `Ctrl+B` no longer opens this modal.
- Classic Bonsai remains present for all users. When both relevant trees are alive, watering either unlocked Bonsai variant mirrors the care action to the other tree for existing daily chip/water compatibility. If either tree is dead, the first `w` respawns the dead tree and returns; watering happens on a later `w`.
- Decision: neither tree freezes. Both run their life/death clocks on real calendar dates regardless of which variant is equipped. A freeze model (rebase the inactive tree's clock on re-equip plus skip its death check while inactive) was considered and deferred.
  - Classic is always loaded, and `bonsai_state.tick()` runs unconditionally in `App::tick()`, so its 7-dry-day death is checked live and at login. (Passive in-session growth was removed 2026-07-23: growth comes from watering only.)
  - Dynamic is loaded at every login whenever the user OWNS it (`has_dynamic_bonsai()` = owns, gated in `session_bootstrap.rs`, not equip). `BonsaiV2State::new` runs `apply_elapsed_days`, which applies dry-day decay and death on real dates even when classic is the active tree; the death clock still catches up at the next login. Dynamic has no in-session tick at all since passive growth was removed.
- The watering bridge is bidirectional after Dynamic Bonsai is owned. Watering Dynamic also waters classic; watering classic waters Dynamic only after the `dynamic_bonsai` entitlement exists, so new users cannot create or care for a Dynamic tree before unlocking it.
- Watering is once per UTC day; a second press the same day says "Already watered today" and moves nothing. The +200 chips are paid by the classic watering path (`DailyCare::mark_watered`), once per day. **Temporary (2026-09-08): admins bypass the Dynamic gate** (`DailyWaterGate::AdminBypass`, chosen by `modal_input::daily_water_gate` from `App::is_admin`) so growth waves can be tested without waiting a day. Classic's own gate and the chips are untouched. Remove the bypass once the renderer has been evaluated.

Rendering:
- There is exactly one renderer, `render_tree_lines` over `render_ascii`, and one size: `canvas_lines` renders the whole `CANVAS_WIDTH` x `CANVAS_HEIGHT` canvas, pot on the last row, trunk rooted at the center column. The modal (with selection highlighting), the sidebar panel (`draw_bonsai_inline`), the profile hero, and the share snippet all draw that block; `center_lines` leads it with blanks inside a wider area. Nothing is scaled or cropped anywhere, because the growth rules keep every tip (and its leaf pad reach) inside the canvas.
- The sidebar and the modal apply `apply_sway` off the wall tick; the profile passes no sway (a static hero). `BONSAI_MIN_HEIGHT` in the sidebar is the canvas plus the footer row.
- The care modal is sized to the canvas (48 by 21): the tree, two status rows, a two-row key footer.
- Profile modals also follow the selected bonsai variant. Dynamic profiles use `BonsaiV2State::view_only`, which applies elapsed-day catch-up in memory for rendering but never persists to the viewed user's row.
- Child branches do not redraw their parent joint cell; only root segments draw their starting cell. This keeps one-cell graph segments from visually collapsing into uneven long ASCII runs.
- There is no static stage template in Dynamic Bonsai rendering.

Chat badge:
- `bonsai_v2_trees.badge_glyph` is joined in `User::list_chat_author_metadata`.
- Chat bonsai glyphs follow the equipped Shop bonsai variant.
- If Dynamic Bonsai is selected in the `bonsai_variant` slot, chat uses the persisted Dynamic Bonsai `badge_glyph`.
- If classic Bonsai is selected, chat uses classic Bonsai `stage_for(is_alive, growth_points).glyph()`.
- When Dynamic Bonsai is selected but the v2 row/glyph is missing or empty, chat shows no bonsai glyph rather than falling back to classic; bootstrap normally prevents this once the owned tree has been loaded/created.
- `BonsaiV2State::new` refreshes and persists `badge_glyph` when the current score ladder would compute a different glyph, so loaded trees migrate across badge-threshold changes without requiring a care action.

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

Branch cap: `MAX_BRANCHES` (`state.rs`) is derived from the canvas, one segment per tip cell plus the root (171 at 21 x 13), so it is never the reason a tree stops: the openness check refuses an occupied cell first, and a full pot is one with no open cell, which any cut reopens. Every branch-adding path still checks the cap independently as the last line of defense: `BonsaiGraph::add_branch`, `grow_tips_once` and `grow_tip_once` before spending a growth-wave slot, the side-shoot spawn, `split_tip_once` (`+2` since a split needs two new branches), `bud_once`, and the graph-normalize/migration path. Deadwood keeps its cells: it is not selectable and not cuttable today, so a tree that took heavy stress damage carries that lost space until it is replanted. The chat badge score is presence-derived from the graph (branch length plus leaf-pad weight, see section 7), so it is bounded by the canvas rather than having a ceiling of its own.

---

## 5. Simulation

Main state values:
- `vigor`: overall growth strength.
- `water_stress`: dry/neglect pressure.
- `last_simulated_date`: UTC date used to catch up elapsed daily growth.
- `last_watered`: UTC daily watering gate.

The pot (2026-09-08):
- Every growth target must satisfy `target_in_canvas`: `|x| <= TIP_MAX_ABS_X` (8) and `1 <= y <= TIP_MAX_Y` (10), checked in `growth_target_is_open`, the one choke point every branch-adding path goes through. A leaf pad reaches 2 cells sideways and 1 up, hence the margin inside the 21 x 13 canvas.
- Length budgets (`run_budget` by `order_and_run`): the trunk and first arms run 3 single-file cells, the next two orders 2, deeper 1. A tip at its budget forks (`split_tip_once`) instead of extending; if no fork is open it is done. A wired tip (status `Wired` or any bend set) always extends where it was told. `split_candidates` opens the trunk's fork upward on both sides and mixes a level arm into deeper forks so the crown spreads sideways.
- `trunk_step`: the trunk goes straight, leans one cell to the seed's side, then straight, and forks at three cells.
- Back-budding (`bud_once`, run at the end of every growth wave, including waves with no growing tip at all): two kinds of site at `end_y >= 2`. A live interior segment carrying exactly one live child throws a shoot leaning away from that child. A leaf pad with no shoot yet throws one out of its foliage (the renderer keeps drawing the pad; `plot_leaf_pad` no longer requires the pad to be a tip). Watering buds up to two sites, a plain day one on a coin flip, a dry day none; vigor under 40 or stress 60+ buds nothing. This is what puts foliage at every height and keeps a pinched-out tree in play: pinch the shoot three times and the pad has moved outward by a cell, denser.
- `repot_into_canvas` runs in `new` and `view_only`: branches whose end lies outside the canvas are cut with their descendants (trunk untouched); `new` persists and says "Repotted: cut back N glyphs to fit the new pot". Trees planted before the canvas pay this once.

Growth paths:
- Daily catch-up happens in `BonsaiV2State::new` via `apply_elapsed_days(today)`.
- Watering grants vigor, reduces stress, and triggers extra growth attempts. There is no passive in-session growth (removed 2026-07-23).
- Dry elapsed days increase stress, reduce vigor, and can create wild growth or deadwood.
- Each growth event is a small wave, not a single tip: split-marked tips resolve first, then the selected tip, then a deterministic random spread of other live tips. Water/high vigor grows the broadest wave; stress can narrow it.

Per-day rates (`simulate_day`):
- Dry day: `water_stress += 11` (clamp 0..120), `vigor -= 7` (floor 0).
- Watered day: `water_stress -= 4` (floor 0), `vigor += 2` (cap 100).
- Watering action (`water`): `water_stress -= 35` (floor 0), `vigor += 18` (cap 100), plus a growth wave.

Current death model:
- If `water_stress >= 100` and `vigor == 0`, Dynamic Bonsai marks the tree dead and weak tips become deadwood.
- This is intentionally less binary than classic Bonsai, where death is primarily a dry-day cutoff.
- Survival without watering: `water_stress` crosses 100 by ~dry day 10, so `vigor == 0` is the gate. Vigor reaches 0 after `ceil(vigor / 7)` dry days, giving a death window of about 10 dry days from a fresh plant (vigor 70) up to 15 dry days from full health (vigor 100). Compare classic Bonsai, which dies at exactly 7 dry days.

---

## 6. Input Model

Dynamic Bonsai modal keys:

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
- When both trees are alive, watering either unlocked Bonsai variant also calls the other variant for chip and daily-care compatibility.
- If the currently opened tree is dead, the first `w` replants and returns; a later `w` waters. A dead mirrored Dynamic tree is replanted from classic watering and can be watered on a later `w`.
- Foliage is earned: pinch a tip, wait for it to become ready again, and repeat until the third pinch turns it into a leaf pad.
- Splits are explicit: `s` marks a tip, and the next growth wave forks it only when both split target cells are unoccupied. High stress can still create messier random side shoots.

---

## 7. Badge Scoring

Dynamic badge intent: keep the familiar bonsai badge meaning "this person is invested here", but derive it from actual rendered/tree presence instead of old growth points.

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

Dead Dynamic Bonsai trees return an empty badge.

Important invariant: a huge neglected mess should not automatically be prestigious. Health/stress must keep mattering.

---

## 8. Critical Invariants

- Keep Dynamic Bonsai separate from classic Bonsai until explicitly promoted.
- `mod.rs` stays declaration-only.
- Do not make Dynamic Bonsai depend on static ASCII stage templates. Classic Bonsai data may initialize Dynamic Bonsai, but dynamic rendering must come from graph state.
- Persist mutations after user-visible graph/state changes.
- Keep classic Bonsai water/chip compatibility while both systems coexist, or daily rewards will diverge.
- Badge metadata must stay cheap for chat; use the persisted `badge_glyph`, not per-message graph rendering.
- The renderer must tolerate narrow/sidebar areas without panics: out-of-grid cells are dropped, never indexed.
- Every surface shows the whole canvas 1:1. Do not reintroduce a preview renderer, a crop, or a camera; if the tree does not fit, the growth rules are wrong, not the window.
- Unit tests in this module must stay pure logic/rendering tests only. DB/service integration belongs under crate `tests/`.

---

## 9. Current Rough Edges

- Renderer is functional, not final art.
- Branch geometry is simple and can create awkward silhouettes.
- No mouse branch picking.
- No seasonal cycles, flowering schedule, scar aging, root work, or repot mechanics yet.
- The budgets, bud rates, and canvas size are first guesses, tuned by feel.
- Deadwood cannot be selected or cut, so its cells are lost for the life of the tree. A jin-removal action (cut deadwood) is the obvious next care verb.
- `branch_graph` JSON has `version`, but no migration/upgrade path exists yet.
- Dynamic Bonsai chat badge selection is user-facing through the `dynamic_bonsai` shop item; staff-only access is limited to Hub Admin catalog editing.

---

## 10. Desired Direction

The interesting version is a small horticulture sim, not a cosmetic randomizer:

- Let branches compete for vigor.
- Let neglected growth become recoverable-but-ugly before death.
- Make pruning create deadwood and back-budding without noisy scar glyphs.
- Make wiring affect future growth more than instant shape.
- Make leaf pads emerge from terminal tips and pinching history.
- Add seasonal overlays as renderer texture, not separate templates.
- Eventually promote Dynamic Bonsai by migrating classic Bonsai users into seeded graphs and replacing the classic modal/sidebar paths.
