# Netrunner Plan

## Goal

Add a late.sh Rooms-backed Netrunner-style two-player game, if the licensing and scope checks pass. Treat this as a major room-game project, not an Arcade game and not a quick card-table skin.

Working assumption: "Netrunner" means the current Netrunner ecosystem maintained by Null Signal Games, not an unrelated cyberpunk minigame.

## Research Snapshot

- Null Signal Games is the current nonprofit publisher/support organization for Netrunner and provides ongoing rules, formats, expansions, print-and-play products, and organized play resources: `https://nullsignal.games/`.
- As of 2026-06-21, Null Signal's home page says System Gateway - Remastered Edition is the recommended starting product, and their products can be bought or downloaded as print-and-play PDFs for free.
- Null Signal Supported Formats currently define "Netrunner Core Sets" as System Gateway plus Elevation, with no associated ban list. This is the best candidate MVP card pool because it is explicitly positioned for new-player replayability.
- Current Standard/Startup legality changes over time. Any implementation that imports live format legality must sync from current Null Signal/NetrunnerDB data instead of hardcoding old assumptions.
- NetrunnerDB provides a public API intended for deckbuilders, card databases, tournament managers, and other complementary tools, with cache headers and CORS. It also states that card texts and graphical information are copyrighted by Fantasy Flight Games and/or Null Signal Games, so late.sh must not treat API/card text/image availability as a blanket redistribution license.
- Jinteki.net (`https://github.com/mtgred/netrunner`) is a mature open-source browser implementation, MIT licensed, written mostly in Clojure/ClojureScript, backed by MongoDB. It is valuable as a behavior/reference corpus but too large and web-shaped to drop directly into late.sh's Rust TUI room-game runtime.

## Product Position

This should live under **Rooms**:

- Two seats: Corp and Runner.
- Embedded game-room chat stays available below the table.
- Room directory should show role occupancy, card pool, and mode.
- Backtick should cycle into active games where the current user is seated.
- Activity feed should publish match wins, not every agenda steal/run.

Do not add this to Arcade. Do not use the existing 52-card helpers in `app/games/cards.rs`; Netrunner needs its own card model because identities, card types, factions, subtypes, counters, zones, costs, advancement, and hidden information do not map to poker cards.

## Recommendation

Start with a **learning/Core Sets MVP**, not full competitive Netrunner.

MVP constraints:

- Card pool: System Gateway + Elevation only, pending legal review.
- Decks: fixed starter deck pairs first; imported decks later.
- Rules: enforce turn structure, clicks, credits, actions, runs, accesses, scoring, trashing, tags, damage, and basic install/rez/advance flows.
- Card effects: implement only the fixed starter-deck card scripts needed for a playable game.
- Images: avoid card art in the TUI MVP. Render compact text cards and use card title/type/cost only where permitted. Add card text only after explicit license/permission review.
- Automation: server-authoritative, no manual "pretend table" for hidden info. The service owns shuffled decks, hands, HQ/R&D/Archives, grip/stack/heap, facedown installed cards, and all random access choices.

Reason: full Netrunner is a card-programming project with hundreds of unique effects, interrupts, replacement effects, paid ability windows, prevention windows, nested runs, expose/reveal rules, and ongoing format churn. A small card pool lets late.sh prove the UI and room model before committing to a huge rules engine.

## Architecture

Source shape:

```text
late-ssh/src/app/rooms/netrunner/
  mod.rs             # declarations only
  manager.rs         # RoomGameManager + ActiveRoomBackend wiring
  svc.rs             # authoritative room service and SharedState
  state.rs           # per-session public/private snapshot cache
  input.rs           # key routing
  ui.rs              # terminal table rendering
  settings.rs        # card pool / mode / timer settings
  create_modal.rs    # room creation form
  cards.rs           # generated/imported card catalog model
  decks.rs           # starter deck definitions and validation
  rules.rs           # phase/action legality helpers
  effects.rs         # card script dispatch for MVP cards
```

Use the existing Rooms pattern:

- Add `GameKind::Netrunner` in `late-core::models::game_room`.
- Register `NetrunnerRoomManager` in `RoomGameRegistry`.
- Store mode/card-pool settings in `game_rooms.settings`.
- Use `game_rooms.runtime_state` only if/when durable resume is supported. For v1, allow process-local matches and rely on startup reconciliation like non-durable room games.
- One public `watch::Sender<NetrunnerPublicSnapshot>`.
- One private `watch::Sender<NetrunnerPrivateSnapshot>` per user, same pattern as Poker.
- Never expose hidden cards through public snapshots. Public snapshots may show card backs/counts for hidden zones, installed facedown server cards, and public board state only.

## Data Model

Runtime zones:

- Corp: HQ, R&D, Archives, scored area, servers, installed cards, identity.
- Runner: grip, stack, heap, scored/stolen area, rig, identity.
- Shared: turn, phase, priority window, trace/current run state, prompts, logs, winner.

Card catalog fields:

- Stable card code/id from NetrunnerDB or local generated data.
- Title, side, faction, type, subtypes.
- Cost/rez/install/play values.
- Influence, deck limit, agenda points/advancement requirement.
- Text/rules text only if license-approved for local storage/rendering.
- Effect handler key for implemented cards.

Deck validation:

- v1: fixed starter decks only.
- v2: paste/import NetrunnerDB decklist IDs after adding an approved data path.
- v3: local deck builder or saved profile decks.

## UI Plan

Terminal layout should optimize for playability, not card art.

Wide layout:

- Top status bar: room, seats, turn, clicks, credits, score, tags/bad pub, current prompt.
- Left half: Corp board with servers, HQ/R&D/Archives counts, remotes, ICE rows, scored agendas.
- Right half: Runner board with rig rows, grip/stack/heap counts, scored agendas.
- Bottom game command line above embedded chat: active prompt and key hints.
- Embedded chat remains the bottom pane via existing Rooms active-room layout.

Narrow layout:

- Compact board summaries with a selected-zone detail panel.
- A local cursor selects zone/card rows.
- `Tab`/`[`/`]` cycles focus groups; arrows move inside a group.

Input principles:

- Avoid one-letter collisions with embedded chat where possible.
- Keep high-frequency actions easy: `Space`/`Enter` confirm, `Esc` backs out/leaves prompt, arrows move cursor.
- Use a command palette for complex actions (`a` action menu, `r` run menu, `p` paid abilities, `i` install, `e` end turn).
- Show only legal actions for the current user and current priority window.

## Implementation Phases

### Phase 0 - Legal and Data Gate

- Confirm what late.sh may host/render: card names, rules text, images, print-and-play data, and generated JSON.
- Decide whether to request explicit written permission from Null Signal Games for a terminal implementation using Core Sets card data.
- If using NetrunnerDB, comply with public API caching and attribution expectations; do not scrape or hotlink card images.
- Decide product naming and disclaimers. Avoid implying Fantasy Flight Games, Wizards of the Coast, or Null Signal endorsement.

Exit criteria: documented yes/no list for data fields allowed in repo, DB, runtime cache, and UI.

### Phase 1 - Skeleton Room Game

- Add `GameKind::Netrunner`, manager registration, create modal, settings, and directory metadata.
- Implement two seats, sit/leave/start/resign, public/private snapshots, and a placeholder board.
- No real cards yet; prove lifecycle, private snapshot delivery, active-room routing, and UI scale.
- Add integration tests for room creation, seating, private snapshot isolation, and leave/cleanup behavior.

### Phase 2 - Core Engine MVP

- Build local card/deck model.
- Implement deterministic shuffle and server-owned hidden zones.
- Implement fixed starter deck loading.
- Implement turn structure and basic actions:
  - Corp clicks, draw, gain credit, install, rez, advance, score, purge if included.
  - Runner clicks, draw, gain credit, install, play event, make run.
  - Run phases, approach/encounter/pass ICE, access, steal/trash.
- Implement win conditions: 7 agenda points, flatline, deck-out where applicable, concession.

### Phase 3 - MVP Card Scripts

- Implement only the cards in the chosen fixed starter decks.
- Use explicit Rust effect handlers, not ad hoc string parsing.
- Add a fixture test per implemented card where the effect is nontrivial.
- Add golden-ish rules tests for run/access/scoring flows, but avoid brittle UI snapshots.

### Phase 4 - Playable TUI

- Render board zones, hand/grip/HQ counts, selected card detail, run state, prompts, and visible logs.
- Implement cursor/action menu flows.
- Add onboarding hints for the first few games, but keep gameplay as the first screen.
- Add turn notifications through `awaiting_my_action`, same as Chess/Poker.

### Phase 5 - Persistence and Quality

- Decide whether active matches persist through SSH process restarts.
- If durable: write compact game-owned runtime JSON into `game_rooms.runtime_state` after every accepted action, including shuffled deck order and hidden zones. Treat this as sensitive private game state.
- Add rematch/new-game flow.
- Add activity wins and optional chip payouts only after the game is stable.
- Add spectator mode only after private/public snapshot isolation is proven.

### Phase 6 - Deck Import / Broader Card Pool

- Import NetrunnerDB decklists only after API permission and OAuth/public endpoint requirements are clear.
- Add deck validation for chosen format.
- Expand card script coverage incrementally by set, not randomly by user deck.
- Consider a "manual unresolved card" blocklist rather than letting unsupported cards enter games.

## Risks

- **Licensing/content:** The biggest nontechnical risk. NetrunnerDB explicitly marks card text/graphics as copyrighted. Free print-and-play availability is not the same as permission for arbitrary redistribution in late.sh.
- **Rules scope:** Full automation is large. Jinteki.net has thousands of tests and years of card implementation work; late.sh should not underestimate this.
- **Terminal UX:** Netrunner is information-dense. A readable SSH UI may require a selected-card detail panel and command menu instead of trying to show full cards everywhere.
- **Hidden information:** Public/private snapshot leakage would break the game. Follow Poker's split-channel pattern and test it aggressively.
- **Format churn:** Standard/Startup legality changes; pin MVP to a local beginner card pool until automated sync is intentional.
- **Durable resume:** Persisting hidden zones is possible but sensitive. Process-local v1 is simpler; durable v2 needs careful schema/versioning.

## Suggested First Milestone

Build a non-shipping spike called `netrunner` behind a compile/runtime feature flag:

1. Room appears in create modal and directory.
2. Two users can sit as Corp/Runner.
3. Start creates fixed dummy decks with hidden hands.
4. Each player sees only their own hand in private snapshot.
5. A fake "gain credit / end turn" loop works.
6. `awaiting_my_action` notifies the active player.

This milestone proves late.sh fit without committing to card data or full rules.

## Sources Checked

- Null Signal Games home page and product positioning: `https://nullsignal.games/`
- Null Signal Supported Formats, Core Sets, Standard, Startup: `https://nullsignal.games/players/supported-formats/`
- NetrunnerDB API notes and copyright warning: `https://netrunnerdb.com/api/2.0/doc`
- Jinteki.net source and MIT license note: `https://github.com/mtgred/netrunner`
- Local late.sh room-game architecture: `CONTEXT.md`, `GAMES.md`, `late-ssh/src/app/rooms/CONTEXT.md`, `late-ssh/src/app/games/CONTEXT.md`
