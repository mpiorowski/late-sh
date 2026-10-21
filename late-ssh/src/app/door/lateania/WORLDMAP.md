# Lateania: the world map

The overhead field and the land map (`worldmap.rs`, `ui.rs`). Part of the Lateania door; the rest of the game is `CONTEXT.md`.

## The overhead world map (`worldmap.rs`)

`m` opens a full-view, biome-coloured overhead map centred on the player. It is derived, never stored: the world is deterministic, so `worldmap::derive_coords` gives every room an `(x, y, z)` once per process behind a `LazyLock`, and `LateaniaService::new` warms it (plus the POI index) on a blocking task so the first opener does not pay a world-gen on the render thread.

`world::biome_of` picks the live field's ground and the overhead map's room colours. Generated regions use their zone layout; authored Duskhollow Caverns and Drowned Crypts use Cavern, and Emberpeak Mines uses Ash, keyed by `Room.zone` from the map's shared world so their side wings inherit the same art. Other authored rooms keep their id-block fallback.

### The chrome sheds; the map does not (`ui::MapChrome`)

The map is the one view that answers "where am I", so a phone-sized terminal still gets the map: what does not fit is the seven rows of header, inspector, controls and three legend lines stacked around it. `MapChrome::for_area` sheds those in reverse order of what they are worth to someone who is lost, and `map_fits` is the floor the body itself needs (32x10):

- **terrain key** first (the biome colours are already on the map): needs 60 columns and 20 rows.
- **marker legend** (18 rows) and **symbol legend** (16 rows), both also gated on `MAP_LEGEND_W` (92 columns). Width sheds the legends first and hardest: they are single unwrapped ~90-column lines, so on a narrow terminal they would only be a sentence clipped mid-word at the edge.
- **the inspector's second row** (what is at the crosshair) below 12 rows. On the surviving single row a `heading` outranks the region name, and with no heading the region name outranks the list of what is in the room.
- **header, the inspector's first row and the controls line stay to the floor**: between them they say where you are, what you are pointing at, and how to move the view. The controls line itself picks one of four widths (`ui::map_controls_line`) rather than being chopped - the full line is 79 columns, and chopping it on a phone would lose the two keys a lost player most needs. Each arm fits the floor of its own band, pinned by `the_map_sheds_its_legends_before_its_body_on_a_phone`.

Two placement mechanisms:

- Procedurally-generated zones decode straight from the room id via `world::region_layout` (`id = base + zone*stride + cell`), so each zone is an exact `w x h` block at its own reserved origin and is collision-free by construction.
- Hand-authored rooms (capitals, roads, villages, housing, archipelago) have no grid and are walked out by BFS over exits, one connected component at a time.

### Geometry and safeguards

The picture is honest by construction, not by filtering. Generated zones are
reserved blocks that cannot overlap; the hand-authored core is authored so its
summed-step embedding stays truthful: each descent zone sits on its own
z-level behind Up/Down seam exits (Whisperwood 0, Duskhollow -1, the Crypts
-2..-4, the Mines -5, Frostspire climbing -4 to -3, the Citadel -4, the
Throne -5), the Wildbound Waste decodes as blocks (`world::wildbound_layout`),
and the town/road wings fan away from what they once folded over. There is no
hide/filter/exempt layer on top of any of this - fog of war and the block
margins are the only things that keep a room off screen, so what is drawn one
cell away really is a few moves away.

The safeguards, in `worldmap_test.rs` unless said otherwise:

- `no_zone_presses_against_another_it_has_no_gate_into`: the fold detector
  (`worldmap::zone_interleaves`) proves no two zones draw within one cell of
  each other while more than `FOLD_WALK_LIMIT` real moves apart. Content that
  re-folds the field fails it with a report naming both rooms, their coords,
  and the walking distance.
- `every_walkable_room_is_reachable_from_the_start_room`: the exit graph is
  connected (portal-only lands excepted), so a severed wing cannot ship;
  `world::link` backs it up by asserting on an occupied exit instead of
  skipping it silently.
- `generated_zones_are_collision_free_and_the_core_stays_tight`: no generated
  room may ever collide; the hand-authored stack tail is measured and capped.
- `one_screen_never_shows_two_reserved_blocks`: `COMPONENT_MARGIN` beats a
  full pan plus a full viewport.
- `a_foe_beyond_the_cell_window_stays_off_the_field` (`svc_test.rs`): the live
  `†`/`☺` lists are scoped by the field's cell window and nothing else.
- `ui::level_label` captions the viewed z; an open-sky zone the ladder pushed
  below z 0 (Frostspire Ascent, the Saltwind Wharves) names its own layer
  instead of reading "underground".

Invariants to keep:

- **`COMPONENT_MARGIN` must stay >= `MAX_VIEWPORT_COLS`.** Blocks are unrelated places; a seam between two of them must never share a screen. At the original margin of 4, an 80-column map showed five zones side by side with a forest slab against Embergate's town square. Locked by `one_screen_never_shows_two_reserved_blocks`.
- **The player always wins their own cell; failing that, the room matching where they stand wins.** Since the unfold the field's collisions are a handful of same-zone stacks (a named wing folded back over its own zone's side room: Frostspire 92/425, Emberpeak 77/395), 0.04% of rooms, measured by `generated_zones_are_collision_free_and_the_core_stays_tight`. `resolve_collision` (shared by `viewport_explored` and `map_canvas`) still resolves each contested cell as player-room first, then a room in the *same region the player currently stands in*, then lowest *visited* id as a last, purely-deterministic tie-break; the region preference is what keeps the answer following the player if a cross-region stack ever returns. `viewport` (fog-less, lowest id wins, no player/region context) is for dumps and tests only.
- **The map never lies about distance: no zone presses against another it has no gate into.** The geometry guarantees it, so no region filter or `exempt` set papers over overlaps. The full picture and the test suite that pins it live in "Geometry and safeguards" above.
- The collision tail is measured, not assumed: `generated_zones_are_collision_free_and_the_core_stays_tight` fails if any *generated* room collides at all, and caps the hand-authored tail at 3% of rooms.

**What the map can and cannot do, measured.** Over the built world's 22,839 exits, 22,827 (99.95%) land exactly where the direction walked says they should, so the embedding does *not* lie about direction and "I walked north and the map moved me south" is not a real failure mode. What is real: **198 rooms have an exit whose two ends sit further apart in the field than a whole screen** (median ~5,600 cells, max ~60,000), because `COMPONENT_MARGIN` reserves a block per zone and almost every zone chains to the next one by a *vertical* exit. Crossing one replaces everything on screen at once. That is the whole of "you enter one node and you're in a totally different graph", and no amount of rendering fidelity addresses it - the three things that do are below, and all three work by adding information the picture structurally cannot carry.

- **A stub is sided by its exit's own `Dir`, never by the coordinate delta to its destination.** Across reserved blocks that delta carries no meaning at all: it only records which block `derive_coords` laid down first. The Timber Longhouse door faces *east* onto Hearthward Close, which sits 5,622 cells *west* of it in the field, so siding by delta drew a corridor stub to the west; the field drew its own honest east stub from `PlayerView.exits` at the same time, giving `─@─` with one real path and one phantom, indistinguishable. Players walked into the phantom. Inventing a path is the worst thing this map can do, and "I thought there was a path there that wasn't actually there" was a real player report, not a hypothetical (`a_scattered_links_stub_follows_the_exit_not_the_coordinate_delta`).
- **`Tile::Stair`** (`⇓` a way down, `⇑` a way up, `⇕` both; bold double arrows so a stair reads at a glance against the terrain, kept clear of the compass's thin `↑↓` and black `⬆⬇`, single-width with no emoji form). A flat level has no direction in which to draw a vertical link, so without these markers the map would show everything about a zone *except* the way out of it. Stairs are drawn in each room's own up-right corner cell (odd column, odd row): rooms sit on even/even and corridors on the odd cell between two of them, so every room owns exactly one free corner and no two rooms can claim the same one (`stair_corners_never_collide_with_rooms_corridors_or_each_other` pins the whole layering rule). The marker says only "there is a way through here", never what is on the far side.
- **The chain breadcrumb.** `RegionPlacement` carries `zone_count`, so the map header reads `Broceliande · zone 7 of 20 · Oakheart Grove`. A ~20-zone continent is 20 unrelated blocks in the field; the picture can never say you are 7 zones into a run of 20, and that sentence is what turns "lost in a forest" back into a position. Regions that aren't procedurally chained simply omit it.
- **The compass: marked destinations and routing** (`worldmap::route`, `state::Heading`). `x` on the map marks the crosshair room (`⚑`, outranking every other marker on that cell); the room panel then carries a `compass` line directly under `exits`, because it answers the question the exits raise: they say what is available, it says which one to take. It is always present once a destination is marked (`Dir::compass_glyph` - ↑↓→←⬆⬇, one per `Dir`, deliberately distinct from the `▴`/`▾` stair glyphs - plus a room-count, e.g. `→ the Vigil House · 4 rooms · take east`). The walk is restricted to `visited` rooms, which is what makes it honest rather than a spoiler machine: it can only retrace ground already covered, it can never reveal an unexplored shortcut, and it needs no gate check because a room can only be in `visited` if the player legitimately walked into it. It is computed client-side (the world graph is process-global in `worldmap`) and memoised on the `(standing in, heading for)` pair, so the search runs once per room actually entered rather than once per keystroke. `Heading` is a closed enum - `Toward`/`Arrived`/`Unreachable` - so a mark that can no longer be reached says so instead of showing a confident direction. There is no automatic quest-objective targeting: `BoardQuest` objectives are not exposed on `PlayerView`, only the older Frontier-zone `quests: Vec<QuestView>` list is, so a player marks their own destination with `x` rather than the compass auto-tracking a bounty - a deliberate scope cut, not an oversight.

Camera: `worldmap::MapCamera` is a pure value (offset from the player + level offset) held on `state::State`, clamped to the field's `bounds()` so panning cannot walk off into blank. `wasd`/`hjkl`/arrows pan, `<`/`>` change level, `x` marks/unmarks the crosshair room as a destination, Enter re-centres, `m` closes; the header flags a panned camera because it names where you *stand* while the inspector names the crosshair. Fog of war comes from `PlayerView.visited`, shared as an `Arc<HashSet<RoomId>>` because `State::view()` clones a whole `PlayerView` on every keystroke and every frame.

Overlay markers come from `worldmap::pois()`: `★` a zone boss (with its guaranteed drops in the inspector), `♥` a tameable wild beast. The marked destination `⚑` outranks all of them, and an active quest's target room draws a `!` (below `⚑`, above `★`) when the quest overlay is on (`state.map_quests`, toggled with `q` on the map, default on; targets come from `QuestView.target`). Border arrows carry a deliberate two-color contract: **amber arrows are the world's** (off-screen bosses/tames to go find, always on, `PAN_LIMIT`-honest), **the green arrow is yours** - only the *tracked* destination gets one, drawn by the same straight-line `PAN_LIMIT` rule as the amber ones (`quest_arrows` over `&[dest]`). Deliberately **not** route-based: `worldmap::route` refuses any destination not in `visited` (its first line), so a route-driven arrow would vanish for exactly the case tracking exists to serve - a boss you have not found yet. A straight-line direction needs no `visited` at all and is honest within a land, which is where it is drawn. For the same reason `quest_arrows` draws the green arrow for **every** target the canvas is not drawing, not only off-screen ones: it skips a target that is on screen *and* walked, because there the canvas paints the `⚑` on the room's own cell, but an on-screen target still in fog gets the arrow, since `map_canvas` emits a `Tile::Room` for visited rooms only and would otherwise leave it blank. The Sunken Catacombs, Thornwood Hollows and Drowned Caverns are each a single flat grid (12x8, 12x8, 14x10) that fits on screen, so a tracked room inside them relies on this (`a_tracked_room_still_in_fog_gets_an_arrow_even_on_screen`). `ui::hug_poi_arrows` pulls an arrow in to the explored cluster's edge, but one whose target sits inside the cluster's box stays on the target's own cell, since anywhere else would put it past the room it points at (`an_arrow_inside_the_explored_cluster_stays_on_its_target`). A target under the crosshair gets no arrow: the crosshair already marks it. The amber arrows (`poi_arrows`) skip anything on screen, fog included, on purpose: an unexplored boss you never asked about stays hidden until you walk to it. **Any destination in the world can be tracked, however far**, because the arrow aims along the real walk rather than at the target's coordinates: `worldmap::track_aim` BFS-walks the shortest path over the *whole* world (not `visited`, same reasoning as above) and cuts it at the first step that leaves the player's floor or land, returning a closed `TrackAim` - `Target` (the walk stays on this floor and in this land, so aim at the destination itself), `Stair { room, climb }`, or `Crossing { room }` (a flat exit into another reserved block, e.g. Melvanala onto the Sunderlakes). The aim is therefore always inside `PAN_LIMIT` on the player's floor, the only place a direction means anything, which `track_aim_always_aims_inside_the_players_land` pins. `State::dest_track_aim` caches it on the `(standing in, heading for)` pair exactly like `route_cache` (a whole-world sweep for an unreachable target is ~10ms in a debug build, so it must never run per keystroke). Once the aimed-at room is in view its stair corner swaps its `⇓`/`⇑` for the walk's single `↓`/`↑` in the tracked colour (`ui::paint_track_aim`; a recolour alone was invisible, since every stair is already drawn in `SUCCESS` green), and a `Crossing` room's glyph just takes the colour. While the map views another floor (`<`/`>`) the arrow aims straight at the destination, as before. `track_aim` walks exits as authored and ignores title gates, so a crown behind a sealed gate is aimed at that gate; a waystone-only land (`portal_lands`) has no walk at all, so it gets no arrow. The journal's `ui::quest_place_note` names the target's region and how the walk sits relative to the player ("in Whisperwood", "- 2 floors down", "- beyond this land", "- reached by waystone"). Untracked quest targets get in-view `!` markers only - no arrows - and `quest_arrows` over all targets still supplies the count of cross-land ones, which the marker-legend line reports as "N quests beyond this land (track: j)" instead of drawing a dishonest arrow. `worldmap::poi_arrows` additionally projects every *off-screen* boss/tameable onto the map border as a direction arrow (`hug_poi_arrows` pulls them in to hug the explored cluster), but **only within `PAN_LIMIT` of the player** - since `COMPONENT_MARGIN = PAN_LIMIT + MAX_VIEWPORT_COLS`, distinct reserved blocks always sit further apart than that, so a delta within `PAN_LIMIT` is guaranteed to be a POI in the *same* block (a real spatial relationship), and anything farther is dropped rather than pointed at with a meaningless direction - the camera could never pan there anyway, since `MapCamera::pan` clamps to the same limit. **These arrows are global map only** - the live field draws none, so a glyph beside `@` can never masquerade as a movement affordance.

`Tile` has two distinct "there's more this way" stubs, not one - both a plain `─`/`│` half-stub of corridor, never an arrow (arrows read as controls; a line means "walkable path" and nothing else): `Hint` is an exit into genuine fog (unvisited), styled dim; `HintKnown` is a link to a room you've *already visited* that the flat grid can't draw adjacent to you (a scattered hand-authored branch, or a jump into a whole other reserved block - the Sunderlakes hanging off Melvanala is the canonical example), styled brighter/amber so a known non-Euclidean jump reads differently from the true edge of your exploration. This is the map's answer to "I can't tell paths from dead ends" and "the water area only shows up once I'm already there": once discovered, the connecting room now visibly says "goes somewhere you know" every time you look, rather than looking identical to unexplored fog. The footer legend (four lines: controls, symbols, markers, terrain) explains both stubs plus `@`/corridors/stairs/the reversed-cell cursor explicitly.

## The land map (`worldmap::land_links`, `ui::land_map_lines`) [VOLATILE]

The map's second page, and the only view that answers **"how do I get there"**. The overhead field (above) is deliberately local: `COMPONENT_MARGIN` is wider than any terminal, so two regions can never share a screen. The text atlas is a flat list with no links. Neither draws a road.

The eighteen atlas regions are drawn as an **atlas**: every country sits at a hand-set place on a character grid, the two hubs every road runs through are walled keeps, and each road between two lands is a line joining them.

```
THE LANDS OF LATEANIA   5 of 18 walked
Every line is a road you can walk. Numbers are zones you have entered.

    Aelunor  0/12 ── Silvael ──╮
    Wildbound Waste  0/3 ──╮   │               Wayfarer's Hollow ──╮
   Sunderlakes  0/14 ──╮   │   │       ╭── City Districts ──╮      │
                    ╭──┴───┴───┴───────┴──╮             ╭───┴──────┴──────╮
 Broceliande  0/20 ─┤      OVERWORLD      ├─────────────┤    EMBERGATE    │
                    ╰──┬───┬───┬───────┬──╯             ╰──┬───────┬──────╯
    Sunken Catacombs ──╯   │   │       │  Frontier  3/20 ──╯       │
       Thornwood Hollows ──╯   │       │        Hearthward Close ──╯
             Drowned Caverns ──╯       │
                            Sundered Reaches  0/20
                                       │
                                       │
                                 Kaelmyr  0/20

Only the Ways reach:  Portal Villages · Shattered Archipelago

walked  ·  not yet  ·  where you stand
```

Three earlier attempts are worth knowing about, because all three failed the same way and the next one will too. An **indented tree** was correct and unreadable: the Overworld's ten roads out rendered as a vertical list, exactly what the picture exists to avoid. A **fan** drew a bare `|` down the middle with unattached names either side, which reads as three unrelated columns. A **trunk with spurs** attached each name to the trunk with its own rule, which fixed the attachment but still stacked the Overworld's spurs into a column. The lesson: *a layout algorithm cannot draw this world*. One node holds ten of the sixteen roads, so any generic arrangement collapses into a list. The map is therefore drawn by hand and checked by machine.

Load-bearing decisions, all of them deliberate:

- **The placement is authored; which lands touch is not.** `KEEPS`/`PLACES`/`ROADS` in `ui.rs` are a hand-set picture: rows, columns, and the legs each road runs. But `ROADS` may only name a pair `worldmap::land_links` derives from the room graph, and must name every such pair. `the_atlas_draws_every_road_in_the_world_and_invents_none` compares the two sets and fails the build if they ever disagree, so the map can neither invent a road nor lose one. **Adding a country means finding it a place in `PLACES` and a road in `ROADS`** - which is the point: a map nobody placed it on is a map that quietly dropped it.
- **It reads the room graph and region membership, and nothing else.** `derive_land_links` records an edge wherever one room's exit lands in another region (`region_atlas_entry`). It never looks at a title, a boss, or a level band, so it **cannot drift out of step with `can_cross_progression_gate`**, and it cannot leak a gate rule. Kaelmyr simply sits at the bottom of the long road south under the Sundered Reaches; a player who walks to the Sundering Deep and finds Yssgar in the doorway draws their own conclusion. Naming the gate would answer the question before it was asked.
- **The two hubs are keeps, and that is what makes the shape drawable.** The Overworld carries ten roads and Embergate five. A bare name has one row above and one below; a box has a whole wall, so each road leaves at its own junction (`┬`/`┴`/`├`/`┤`, worked out in `Canvas::stroke` from whatever the road lands on) and no two spurs share a line. `City Districts` sits between the two keeps because it opens off both of them - the one cycle in an otherwise tree-shaped world.
- **North of the road is the country you walk to; south of it is the dark.** The living-dark dungeons, the Frontier, the Reaches and Kaelmyr all hang below; the Sunderlakes, Broceliande, Silvael/Aelunor, the Wildbound Waste, Wayfarer's Hollow and the Close sit above. Nothing enforces it but the layout, and `the_atlas_lays_the_realm_out_the_way_it_is_walked` pins it.
- **A chain reads outward from the hub.** Aelunor is reached through Silvael, so it is drawn the far side of it (`Aelunor ── Silvael ──╮`, the road arriving on Silvael's side); the Reaches and Kaelmyr stack downward for the same reason. Distance on the page is distance from the hub, not reading order.
- **Every land is named, walked or not.** The name of a country was never the secret; the road to it is. Undiscovered lands render faint, not as `???`, and a road runs faint until both lands it joins are walked. The footer legend says what the three colours mean, since colour is the only thing carrying it.
- **Depth is in zones, not rooms, and is a number rather than a bar.** `RegionProgress.chain` counts zones with at least one visited room, from `LAND_CHAINS` (names written out, every number read from the generator's own consts). A country can be three zones deep on 2% of its rooms, and depth is the number that says how far in you are. A land absent from `LAND_CHAINS` draws with no depth at all. A shaded `▓░░░` bar was tried and removed: a run of block glyphs renders as one solid rectangle in plenty of terminal fonts, so it read as noise rather than progress.
- **Labels are anchored to their road, not to a column.** `At::Ends`/`Starts`/`Centered` fix the end that meets the road, so a counter gaining a digit (`3/20` -> `20/20`) pushes the name *away* from the road rather than shoving the road it is attached to. Both `the_land_map_stays_inside_the_narrowest_terminal_it_draws_into` and `no_land_on_the_atlas_is_written_over_by_another` render the fully-walked atlas as well as the fresh one, so a two-digit counter cannot silently overflow or overwrite a neighbour.
- **Names are shortened to fit** (`land_chip_name`: `land_label` plus a leading `The` stripped). The picture is a fixed 76x13 grid ending at column 74, so `lands_fit` (76x12) refuses to draw it into anything narrower and the text atlas serves instead, the same way the overhead field falls back below 50x14.
- **The scroll hint is only shown when there is something below the fold**, since a hint for a key that does nothing is worse than no hint. That is the only thing `land_map_lines` uses its `height` argument for.
- `LAND_LINKS` is a `LazyLock` forced by `worldmap::warm()` alongside the coordinate field and the POI index; a player never pays for it on the render thread.
- `land_map_lines` is split from `draw_land_map` so the layout is read in tests rather than only on a screen. Pinned by `the_land_graph_is_read_off_the_room_graph_and_covers_every_region` (`worldmap_test.rs`) plus six in `ui_test.rs`.
---
