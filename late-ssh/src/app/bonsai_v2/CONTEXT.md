# Bonsai V2 Context

## Metadata
- Scope: `late-ssh/src/app/bonsai_v2`
- Last updated: 2026-05-22
- Purpose: local working context for the experimental living bonsai branch-graph system.
- Status: Active prototype, moderator/admin gated.
- Parent context: `../../../../CONTEXT.md`

---

## 1. Scope

Bonsai V2 is the experimental replacement path for the old static-stage bonsai renderer. It is currently enabled for moderators/admins through `App::use_bonsai_v2()` and leaves regular users on Bonsai V1.

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
- V2 rows are created only for moderators/admins during session bootstrap. `BonsaiV2Tree::save` upserts so a fallback V2 state can still persist if a user gains permissions mid-session.

Session state:
- `App` always has `bonsai_v2_state`, but the visible V1 Bonsai surface remains the default for all users.
- `App::use_bonsai_v2()` currently returns `permissions.can_moderate()` and is used for V2 background lifecycle only; it must not make `w` or sidebar previews switch to V2.
- Reserved global `Ctrl+B` opens the Bonsai V2 care modal for admin/moderator sessions, except during active Artboard editing where raw control bytes stay local.
- V1 remains present for all users. For V2 testers, V1 watering still runs for existing daily chip/water compatibility.

Rendering:
- Sidebar previews always use the old Bonsai renderer while V2 is hidden.
- The V2 modal uses `bonsai_v2::modal_ui::draw` only when `show_bonsai_v2_modal` is opened by `Ctrl+B`.
- Renderer draws the graph into a fixed grid, rasterizes branches, adds leaf pads around healthy tips, and highlights the selected branch in the modal.
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
- `LeafPad`: terminal growth converted into compact foliage.
- `Cut`: visible pruning scar.
- `Deadwood`: dead retained structure.

Important concept: user actions should affect future geometry, not only the current frame. Wiring sets bend memory. Pruning changes the graph and can create back-buds. Pinching trims grown tips and builds ramification; healthy ramified tips leaf out during later growth.

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

Current death model:
- If `water_stress >= 100` and `vigor == 0`, V2 marks the tree dead and weak tips become deadwood.
- This is intentionally less binary than V1, where death is primarily a dry-day cutoff.

---

## 6. Input Model

V2 modal keys:

```text
Ctrl+B     open Bonsai V2 Care globally for admins/moderators
w          water or replant if dead
tab / n    select next live branch
shift-tab  select previous live branch
h / left   wire selected tip left
l / right  wire selected tip right
k / up     wire selected tip upward
j / down   wire selected tip downward
x          prune selected branch
p          pinch selected grown tip; repeated grown pinches build ramification
t / T      admin-only: advance 1 / 10 simulated days
s          copy V2 share snippet
?          open Bonsai help
q / Esc    close
```

Current interaction limitations:
- Selection is branch-cycle based, not cursor/mouse picking.
- Wiring records future growth bias; it does not instantly extend the branch.
- Pruning the trunk is intentionally blocked in the prototype.
- Watering V2 also calls V1 watering for chip compatibility when the old tree is alive.
- Admin V2 testers can repeat `w` on the same day; V1 chips and legacy growth remain daily-gated.
- If either V1 or V2 is dead, the first `w` replants and returns; a later `w` waters.
- Admin-only fast-forward simulates whole-tree elapsed days with the normal daily/dry rules.
- Foliage is earned: growth makes branch structure, repeated grown pinches add ramification, and healthy ramified tips leaf out during later growth.

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
- Staff gating is broad (`can_moderate`) rather than a dedicated feature flag or mod setting.

---

## 10. Desired Direction

The interesting version is a small horticulture sim, not a cosmetic randomizer:

- Let branches compete for vigor.
- Let neglected growth become recoverable-but-ugly before death.
- Make pruning create scars, deadwood, and back-budding.
- Make wiring affect future growth more than instant shape.
- Make leaf pads emerge from terminal tips and pinching history.
- Add seasonal overlays as renderer texture, not separate templates.
- Add a real sidebar camera that preserves the pot/trunk silhouette and compresses detail.
- Eventually promote V2 by migrating V1 users into seeded graphs and replacing V1 modal/sidebar paths.
