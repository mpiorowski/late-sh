# Bonsai V2 Context

## Metadata
- Scope: `late-ssh/src/app/bonsai_v2`
- Last updated: 2026-05-22
- Purpose: local working context for the experimental living bonsai branch-graph system.
- Status: Active prototype, unlocked through the Dynamic Bonsai shop item.
- Parent context: `../../../../CONTEXT.md`

---

## 1. Scope

Bonsai V2 is the experimental replacement path for the old static-stage bonsai renderer. It is currently selected through the `dynamic_bonsai` shop item and leaves users on Bonsai V1 unless that item is equipped.

The core idea is:

```text
seed + persistent branch graph + vigor/stress/care actions -> rendered ASCII tree
```

The tree should not be a finite ladder of predefined pictures. The visible structure should be a persistent record of player decisions: watering, wiring, pruning, pinching, stress, recovery, and future growth.

V2 is not final polish. It is an end-to-end dynamic prototype with real persistence, sidebar preview, modal, input, rendering, growth, and badge plumbing.

---

## 2. File Map

```text
late-ssh/src/app/bonsai_v2/
|-- mod.rs              # Module declarations only
|-- state.rs            # Persistent branch graph, growth simulation, care actions, badge scoring
|-- render.rs           # ASCII rasterizer and sidebar/modal line rendering
|-- modal_ui.rs         # Bonsai V2 care workbench modal
|-- modal_input.rs      # V2 modal key handling and V1 water/chip compatibility bridge
`-- CONTEXT.md          # This file
```

Related files:

```text
late-core/migrations/056_create_bonsai_v2.sql
late-core/src/models/bonsai.rs
late-core/src/models/user.rs
late-ssh/src/app/bonsai/svc.rs
late-ssh/src/app/common/sidebar.rs
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
- Table: `bonsai_v2_trees`.
- One row per user.
- Stores `seed`, `last_watered`, `is_alive`, `vigor`, `water_stress`, `last_simulated_date`, `branch_graph` JSONB, `selected_branch_id`, `mode`, and precomputed `badge_glyph`.
- V2 rows are loaded/created for users who own the Dynamic Bonsai shop item during session bootstrap. `BonsaiV2Tree::save` upserts so a fallback V2 state can still persist after a user buys the item mid-session.

Session state:
- `App` always has `bonsai_v2_state`, but the visible V1 Bonsai surface remains the default unless Dynamic Bonsai is equipped.
- `App::use_bonsai_v2()` follows the equipped `bonsai_variant` shop slot. It switches `w` and V2 background lifecycle, but sidebar previews still stay on V1 until the preview path is redesigned.
- Global `Ctrl+B` no longer opens V2. Dynamic Bonsai is entered through the regular `w` bonsai launcher after shop selection.
- V1 remains present for all users. V2 watering still runs V1 watering for existing daily chip/water compatibility.

Rendering:
- Sidebar previews always use the old Bonsai renderer while V2 is hidden.
- The V2 modal uses `bonsai_v2::modal_ui::draw` only when `show_bonsai_v2_modal` is opened by the regular bonsai launcher while Dynamic Bonsai is selected.
- Renderer draws the graph into a fixed grid, rasterizes branches, adds leaf pads around healthy tips, and highlights the selected branch in the modal.
- Child branches do not redraw their parent joint cell; only root segments draw their starting cell. This keeps one-cell graph segments from visually collapsing into uneven long ASCII runs.
- There is no static stage template in V2 rendering.

Chat badge:
- `bonsai_v2_trees.badge_glyph` is joined in `User::list_chat_author_metadata`.
- Staff users with a non-empty V2 badge advertise that badge in chat.
- Non-staff users continue using the V1 `stage_for(is_alive, growth_points).glyph()` path.

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

Important concept: user actions should affect future geometry, not only the current frame. Wiring sets bend memory. Cutting only removes the selected branch and descendants. Pinching marks the selected tip as compact growth; it must be pinched three times over separate growth moments to become a leaf pad, and pinched branches do not keep extending. Splitting marks the selected tip for the next growth wave; it forks only if both target cells are open.

Branches are stored as one-cell growth segments. Growth adds a new child segment instead of extending the selected branch endpoint, so selecting/cutting a branch id targets that exact segment and descendants downstream from it.

---

## 5. Simulation

Main state values:
- `vigor`: overall growth strength.
- `water_stress`: dry/neglect pressure.
- `last_simulated_date`: UTC date used to catch up elapsed daily growth.
- `last_watered`: UTC daily watering gate.

Growth paths:
- Daily catch-up happens in `BonsaiV2State::new` via `apply_elapsed_days(today)`.
- Passive growth happens in `tick()` on a long interval when vigor is high enough.
- Watering grants vigor, reduces stress, and triggers extra growth attempts.
- Dry elapsed days increase stress, reduce vigor, and can create wild growth or deadwood.
- Each growth event is a small wave, not a single tip: split-marked tips resolve first, then the selected tip, then a deterministic random spread of other live tips. Water/high vigor grows the broadest wave; stress can narrow it.

Current death model:
- If `water_stress >= 100` and `vigor == 0`, V2 marks the tree dead and weak tips become deadwood.
- This is intentionally less binary than V1, where death is primarily a dry-day cutoff.

---

## 6. Input Model

V2 modal keys:

```text
w          water or replant if dead
tab / n    select next live branch
shift-tab  select previous live branch
h / left   wire selected tip left
l / right  wire selected tip right
k / up     wire selected tip upward
j / down   wire selected tip downward
x          prune selected branch
p          pinch selected tip toward a leaf pad; needs 3 pinches over time
s          split selected tip on next growth if both target cells are open
c          copy V2 share snippet
?          open Bonsai help
q / Esc    close
```

Current interaction limitations:
- Selection is branch-cycle based, not cursor/mouse picking.
- Wiring records future growth bias; it does not instantly extend the branch.
- Pruning the trunk is intentionally blocked in the prototype.
- Watering V2 also calls V1 watering for chip compatibility when the old tree is alive.
- If either V1 or V2 is dead, the first `w` replants and returns; a later `w` waters.
- Foliage is earned: pinch a tip, wait for it to become ready again, and repeat until the third pinch turns it into a leaf pad.
- Splits are explicit: `s` marks a tip, and the next growth wave forks it only when both split target cells are unoccupied. High stress can still create messier random side shoots.

---

## 7. Badge Scoring

V2 badge intent: keep the familiar bonsai badge meaning "this person is invested here", but derive it from actual rendered/tree presence instead of old growth points.

Current implementation:
- Computes graph presence from live branch length plus leaf-pad weight.
- Applies a health/stress multiplier.
- Maps score to the familiar glyph ladder:

```text
0-8       ·
9-20      ⚘
21-40     🌱
41-75     🌲
76-120    🌳
121-180   🌸
181+      🌼
```

Dead V2 trees return an empty badge.

Important invariant: a huge neglected mess should not automatically be prestigious. Health/stress must keep mattering.

---

## 8. Critical Invariants

- Keep V2 separate from V1 until explicitly promoted. Regular users must keep the stable V1 path.
- `mod.rs` stays declaration-only.
- Do not make V2 depend on static ASCII stage templates. Seeded V1 data may initialize V2, but V2 rendering must come from graph state.
- Persist mutations after user-visible graph/state changes.
- Keep V1 water/chip compatibility while V2 is staff-only, or daily rewards will diverge for testers.
- Badge metadata must stay cheap for chat; use the persisted `badge_glyph`, not per-message graph rendering.
- Renderer must tolerate narrow/sidebar areas without panics.
- Unit tests in this module must stay pure logic/rendering tests only. DB/service integration belongs under crate `tests/`.

---

## 9. Current Rough Edges

- Renderer is functional, not yet beautiful.
- Branch geometry is simple and can create awkward silhouettes.
- No mouse branch picking.
- No seasonal cycles, flowering schedule, scar aging, root work, or repot mechanics yet.
- Sidebar preview is a direct compact render, not a true camera/crop/simplification pipeline.
- Huge old trees need better viewport/camera behavior.
- `branch_graph` JSON has `version`, but no migration/upgrade path exists yet.
- Sidebar preview still renders V1 even when Dynamic Bonsai is selected.

---

## 10. Desired Direction

The interesting version is a small horticulture sim, not a cosmetic randomizer:

- Let branches compete for vigor.
- Let neglected growth become recoverable-but-ugly before death.
- Make pruning create deadwood and back-budding without noisy scar glyphs.
- Make wiring affect future growth more than instant shape.
- Make leaf pads emerge from terminal tips and pinching history.
- Add seasonal overlays as renderer texture, not separate templates.
- Add a real sidebar camera that preserves the pot/trunk silhouette and compresses detail.
- Eventually promote V2 by migrating V1 users into seeded graphs and replacing V1 modal/sidebar paths.
