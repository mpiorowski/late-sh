# Dynamic Bonsai Context

## Metadata
- Scope: `late-ssh/src/app/bonsai_v2`
- Last updated: 2026-05-31
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

This is not final polish. It is an end-to-end dynamic prototype with real persistence, shop selection, sidebar preview, modal, input, rendering, growth, and badge plumbing.

---

## 2. File Map

```text
late-ssh/src/app/bonsai_v2/
|-- mod.rs              # Module declarations only
|-- state.rs            # Persistent branch graph, growth simulation, care actions, badge scoring
|-- render.rs           # Modal renderer plus compact sidebar preview renderer
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
- Stores `seed`, `last_watered`, `is_alive`, `vigor`, `water_stress`, `last_simulated_date`, `branch_graph` JSONB, `selected_branch_id`, `mode`, and precomputed `badge_glyph`.
- Rows are loaded/created for users who own Dynamic Bonsai during session bootstrap. `BonsaiV2Tree::save` upserts, so fallback state can persist after a user buys the item mid-session.

Session state:
- `App` always has `bonsai_v2_state`, but classic Bonsai remains visible unless Dynamic Bonsai is equipped.
- `App::use_bonsai_v2()` follows `ShopState::dynamic_bonsai_enabled()`.
- Global `w` opens Dynamic Bonsai when selected; otherwise it opens classic Bonsai.
- Global `Ctrl+B` no longer opens this modal.
- Classic Bonsai remains present for all users. Watering either unlocked Bonsai variant mirrors the care action to the other tree for existing daily chip/water compatibility.
- Decision: neither tree freezes. Both run their life/death clocks on real calendar dates regardless of which variant is equipped. A freeze model (rebase the inactive tree's clock on re-equip plus skip its death check while inactive) was considered and deferred.
  - Classic is always loaded, and `bonsai_state.tick()` runs unconditionally in `App::tick()`, so it keeps passive-growing in-session and its 7-dry-day death is checked live and at login.
  - Dynamic is loaded at every login whenever the user OWNS it (`has_dynamic_bonsai()` = owns, gated in `session_bootstrap.rs`, not equip). `BonsaiV2State::new` runs `apply_elapsed_days`, which applies dry-day decay and death on real dates even when classic is the active tree. Only the in-session passive-growth `bonsai_v2_state.tick(active)` call is gated by `use_bonsai_v2()` and recent input activity; the death clock still catches up at the next login.
- The watering bridge is bidirectional after Dynamic Bonsai is owned. Watering Dynamic also waters classic; watering classic waters Dynamic only after the `dynamic_bonsai` entitlement exists, so new users cannot create or care for a Dynamic tree before unlocking it.
- Admin sessions can temporarily repeat-water Dynamic Bonsai from the modal for preview/growth testing. Legacy chips and classic Bonsai growth remain daily-gated.

Rendering:
- The modal uses the detailed graph renderer and highlights the selected branch.
- The sidebar uses a separate compact preview renderer when Dynamic Bonsai is selected.
- The compact preview samples graph-space branch/leaf cells, anchors horizontally on the trunk/pot center, scales into the sidebar area, and uses density glyphs. Sparse leaves render as `@`, denser foliage as `*`/`#`.
- Child branches do not redraw their parent joint cell; only root segments draw their starting cell. This keeps one-cell graph segments from visually collapsing into uneven long ASCII runs.
- There is no static stage template in Dynamic Bonsai rendering.

Chat badge:
- `bonsai_v2_trees.badge_glyph` is joined in `User::list_chat_author_metadata`.
- Chat bonsai glyphs follow the equipped Shop bonsai variant.
- If Dynamic Bonsai is selected in the `bonsai_variant` slot, chat uses the persisted Dynamic Bonsai `badge_glyph`.
- If classic Bonsai is selected, chat uses classic Bonsai `stage_for(is_alive, growth_points).glyph()`.

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

---

## 5. Simulation

Main state values:
- `vigor`: overall growth strength.
- `water_stress`: dry/neglect pressure.
- `last_simulated_date`: UTC date used to catch up elapsed daily growth.
- `last_watered`: UTC daily watering gate.

Growth paths:
- Daily catch-up happens in `BonsaiV2State::new` via `apply_elapsed_days(today)`.
- Passive growth happens in `tick(active)` only while Dynamic Bonsai is selected, recent user input keeps the session active, enough active ticks have accumulated, and vigor is high enough.
- Watering grants vigor, reduces stress, and triggers extra growth attempts.
- Dry elapsed days increase stress, reduce vigor, and can create wild growth or deadwood.
- Each growth event is a small wave, not a single tip: split-marked tips resolve first, then the selected tip, then a deterministic random spread of other live tips. Water/high vigor grows the broadest wave; stress can narrow it.

Per-day rates (`simulate_day`):
- Dry day: `water_stress += 11` (clamp 0..120), `vigor -= 7` (floor 0).
- Watered day: `water_stress -= 4` (floor 0), `vigor += 2` (cap 100).
- Watering action (`water_inner`): `water_stress -= 35` (floor 0), `vigor += 18` (cap 100), plus a growth wave.

Passive growth rate:
- User input grants a short active window. Idle open sessions do not count.
- The passive interval is `15 * 60 * 60 * 6` active ticks, so a continuously active session gets about 2-4 passive growth waves per real day.

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
←↓↑→ / hjkl steer selected tip's future growth
x          prune selected branch
p          pinch selected tip toward a leaf pad; needs 3 pinches over time
s          split selected tip on next growth if both target cells are open
c          copy share snippet
?          open Bonsai help
q / Esc    close
```

Current interaction limitations:
- Selection is branch-cycle based, not cursor/mouse picking.
- Wiring records future growth bias; it does not instantly extend the branch.
- Pruning the trunk is intentionally blocked in the prototype.
- Watering either unlocked Bonsai variant also calls the other variant for chip and daily-care compatibility when the other tree is alive.
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
0-8       .
9-20      sprout
21-40     sapling
41-75     pine
76-120    tree
121-180   blossom
181+      flower
```

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
- Renderers must tolerate narrow/sidebar areas without panics.
- Sidebar preview is a compact silhouette, not an exact modal miniature.
- Unit tests in this module must stay pure logic/rendering tests only. DB/service integration belongs under crate `tests/`.

---

## 9. Current Rough Edges

- Renderer is functional, not final art.
- Branch geometry is simple and can create awkward silhouettes.
- No mouse branch picking.
- No seasonal cycles, flowering schedule, scar aging, root work, or repot mechanics yet.
- Sidebar preview uses a trunk-centered scale-to-fit camera; very large or highly asymmetric trees may still need better crop/simplification rules.
- `branch_graph` JSON has `version`, but no migration/upgrade path exists yet.
- Chat badge promotion is still partly staff-gated even though the shop item is user-facing.

---

## 10. Desired Direction

The interesting version is a small horticulture sim, not a cosmetic randomizer:

- Let branches compete for vigor.
- Let neglected growth become recoverable-but-ugly before death.
- Make pruning create deadwood and back-budding without noisy scar glyphs.
- Make wiring affect future growth more than instant shape.
- Make leaf pads emerge from terminal tips and pinching history.
- Add seasonal overlays as renderer texture, not separate templates.
- Improve the sidebar camera so it preserves the pot/trunk silhouette while compressing detail.
- Eventually promote Dynamic Bonsai by migrating classic Bonsai users into seeded graphs and replacing the classic modal/sidebar paths.
