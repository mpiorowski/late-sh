# Render-cost plan (working doc)

Living tracker for the render-cost work. Update as work lands; keep it
current-state only (no history chains). This file is the context handoff
between LLM sessions: read it fully before touching the render gate.

## Design rules (do not violate)

- PROVE-CLEAN, NOT PROVE-DIRTY. Anything uncertain reports changed. A
  spurious frame costs nothing; a wrong "clean" freezes UI.
- Both loops gate: `render_once` in ssh.rs AND web_tunnel.rs do
  `changed = signal.dirty.swap(false) | input drained | app.tick()`;
  `!changed` skips `terminal.draw()` entirely (ratatui diff does not advance
  on skip, no forced repaint needed). web_tunnel.rs mirrors ssh.rs
  deliberately (duplication over abstraction): change both.
- Peek receivers BEFORE draining (`has_changed()` on watches, `!is_empty()`
  on mpsc/broadcast). Exception: fixed-cadence publishers (chat snapshot,
  audio queue) report real change from the drain itself, never the peek.
- Watch receivers that are only `borrow()`ed at render must be marked seen
  (`borrow_and_update`) by whoever peeks them, or the peek latches dirty
  forever (this is why artboard's archive view skips the snapshot peek).
- Product decision (revised 2026-07-23): nothing paints at full rate.
  The old FFT bar visualizer is replaced by a synthetic ambient wave
  (`viz::render_wave`) that scrolls whenever the sidebar is visible; it,
  cats, bonsai sway, and clubhouse ambience all paint on the shared
  half-rate edge (`anim_half`, ~7.5fps); fish step on the quarter edge
  (`anim_quarter`, ~3.8fps); everything else is slow/static. Corollary: a
  sidebar-visible session never settles fully clean and never idles below
  ANIM_HALF_TICK; that steady ~37 draws/5s is the accepted price of the
  always-on wave (knob if it reads expensive: move the wave to the
  quarter edge, do not reintroduce audio-state gating).
  Marquee: 3 columns/sec in 1s steps (`MARQUEE_STEP_TICKS = 15`,
  `MARQUEE_STEP_COLUMNS = 3`, hold 45), so speed costs no extra frames;
  every marquee transition lands on a multiple of the step (tested).
- Metrics: `late_ssh_renders_total{reason=input|tick}` vs
  `late_ssh_renders_skipped_clean_total` (metrics.rs, `RenderReason` closed
  enum) observe the skip ratio in prod; they do not gate the work.
  Grafana: "Rendering" row in monitoring/dashboards/observability.json
  (render rate, clean-skip ratio, draws per session, stall guard).

## Test gotchas

- Any test driving `tick()` without `render()` leaves
  `pending_terminal_commands` queued and the gate correctly stays dirty;
  mirror the loop with a drain_frame (render + take commands). See
  `app/tick_test.rs`.
- The settle tests (`idle_ticks_settle_clean_and_chat_send_marks_changed`,
  `open_settings_modal_settles_clean`) loop to 30 consecutive clean ticks;
  their failure panic dumps a state snapshot; extend that dump when
  debugging new dirt sources.
- Never raw cargo test; `make test-llm ARGS="-p late-ssh -E 'test(...)'"`.
  `cargo check` under `systemd-run --user --scope -q -p MemoryMax=12G` is fine.

## Shipped

### Phase 0 (2026-07-22)
Chat row caches counter-validated (`ChatRowsVersions`; see chat/CONTEXT.md
"Cache"). Presence cached at 1Hz. Per-session `targeted_event_rx` for
single-recipient chat events. 64KB BufWriter frame path. OutputBudget guard
(32MB unacked pause, 30s disconnect).

### Phase 1 PR1 — dirty gate (2026-07-22)
`App::tick() -> bool` (late-ssh/src/app/tick.rs); both loops skip clean
frames. Per-subsystem changed signals: five tab states + chat + profile +
daily + quest + shop + voice + sudoku + terminal images + bonsai/bonsai_v2 +
aquarium (entitlement-gated, stepped on the half-rate edge) + viz settle
(SETTLE_EPSILON; superseded 2026-07-23 by the stateless ambient wave).
`Outbox::has_pending()` forces a frame for outbox/terminal
commands/clipboard. Pinstar rx-transition-to-None pattern. End-of-tick
composite: chat epochs, screen compare, banner-expiry one-shot,
now_playing/radio_meta peeks. afk-set + username-directory ptr-compares in
tick's 1Hz block; `last_sidebar_clock` minute rollover fires one global
frame per minute (free ride for any minute-granularity label anywhere).
CONTEXT.md §2.6 documents the gate.

### Phase 1 PR2 — tightening (2026-07-22)
- Chat snapshot cadence: `drain_snapshot -> bool` returns real change
  (unread_counts, ordered room list `(id, updated)` + full `ChatRoom`
  compare, per-room message signatures, lounge_room_id, active_polls,
  voice_channels, reactions, ticker inserts, selection sync);
  `ChatState::tick` uses it instead of the watch peek. Enabler: `model!`
  macro + chat-poll structs in late-core derive `PartialEq`.
  Test: `identical_snapshot_reapply_reports_clean` (state_internal_test.rs).
- Visualizer replaced by the ambient wave (2026-07-23): the FFT bars, the
  decay/procedural state machine, and the short-lived quantized paint
  gate are all gone. `viz::render_wave(frame, area, wall_tick)` is a
  stateless hand-drawn box-glyph wave tile rotated by `marquee_tick`
  (offset = `wall_tick / 2 % WAVE_PERIOD_COLS`, one column per anim_half
  edge; braille plotting was tried first and looked like scattered dots,
  box-drawing matches the panel borders), so
  tick() has nothing to advance: `changed |= anim_half` while the sidebar
  or a bonsai modal is visible IS the whole gate. Bonsai sway (v1 canopy
  shift, v2 `apply_sway` line shift) runs off the same wall clock, small
  constant amplitude, no longer beat-kicked. `SessionMessage::Viz` frames
  are dropped on arrival (still drain-excluded); the WS/CLI/late-core
  pipeline removal is scoped in VIZ_WAVE_BRIEF.md for a follow-up agent.
- Pet strip: `PetState::tick() -> bool` (feedback expiry, roam end,
  day-rollover mood/needs flips via `last_visual` compare). Animation pays
  exactly on frame boundaries: `pet::ui::strip_frame_changed(mood, tick,
  travel)` vs the `last_pet_strip_travel` Cell slot recorded at draw (same
  pattern as the click-target rect slots; slot reset each render, None =
  strip not drawn = no frames). Roaming overlay rides the anim_half edge
  (lowered from full rate 2026-07-23 after the wall-clock audit:
  `PetState::tick(wall_tick)` syncs animation_ticks to marquee_tick and
  scales the feedback countdown by elapsed ticks). Sad parked
  pet = fully static. Tests: pet/ui_test.rs.
- Modals now event-driven via their tick paths: settings (`SettingsTick`),
  profile modal (`tick() -> bool`), hub admin (`AdminTick.changed`), audio
  (`AudioTick.changed` = queue-snapshot peek, marked seen in tick; covers
  booth modal + sidebar stage), bonsai care (`tick() -> bool` while watering
  animation plays). Dropped from the coarse list entirely: hub, poll, icon
  picker, room search (results ride chat events), booth, bonsai_v2 (growth
  via bonsai_v2_state.tick, sway wall-clock on the anim_half edge).
- Scoped cadences kept: lobby modal 1Hz (live occupancy read at draw),
  ultimate modal 1Hz only while `has_cooldown_running()`, profile modal
  half-rate only while `aquarium_animating()` (reef steps via step_reef
  in App::tick; draw only paints),
  DailyMatch screen 1Hz (deadline clock; board is otherwise event-driven).
- Artboard: `DartboardState::tick() -> bool` (snapshot watch + event_rx +
  archive loader peeks; archive view deliberately excludes the latched
  snapshot peek).
- World Cup screen removed entirely (separate concurrent work), so its
  tightening item is moot.

### PR2 — domain sweep (2026-07-23, complete)

The dirty contract ("rule of three") is codified in CONTEXT.md §2.6;
every domain state now exposes `tick() -> bool` under it.

- Lateania + Green Dragon: shared blanket split; both `State::tick() ->
  bool`. Lateania reports its `MudSnapshot` peek plus join/reset
  transitions; Green Dragon reports while any one-shot load rx is in
  flight (`load_pending` OR over the 16 Option receivers + initial
  character load). Both drain off-screen but only dirty on-screen.
- House tables: all five game states return their watch-peek result
  (`HouseTableClient::tick() -> bool`); blackjack also reports event
  drains, asterion its FLASH_TTL expiry. Poker's action clock is now
  server-republished each second (`schedule_action_timeout` wakes at 1s
  steps, same shape as blackjack), so the watch peek covers every
  countdown and no client-side clock cadence exists. Server loops going
  quiet between rounds means an idle table settles clean.
- Clubhouse: ambience heartbeat on the anim_half edge (~7.5fps) while on
  screen; walker positions are input-driven, dog step wall-clock, so
  only cosmetic loops (jukebox EQ t/2 fastest) slowed. The clubhouse
  `anim_tick` is synced to `marquee_tick` (wall clock), not incremented
  per call: a per-call counter tied animation speed to the adaptive
  cadence (walking held the hot window and sped the room up 4x).
- Traffic: `tick() -> bool`, false while paused/non-playing (was the one
  arcade holdout forcing frames while paused).
- Heartbeat: a `SessionMessage::Heartbeat`-only drain no longer counts
  as changed.
- Ultimate modal: per-second countdown REMOVED (product decision):
  `format_cooldown` is minute-granularity ("<1m" floor) riding the
  per-minute global frame; the running -> ready flip pays one one-shot
  frame via `ultimate_cooldown_was_running` edge detect. No 1Hz.
- Chat image modal: Sixel fetch moved from render.rs into tick
  (`request_image_modal_terminal_image` keys off the capacity recorded
  by the input-forced draw that opened/resized the modal); the blanket
  `changed |= image_modal.is_some()` is gone, completions report via
  `poll_terminal_images`. NOTE for Phase 2: after the open frame records
  capacity, the fetch fires on the NEXT tick; the event loop must
  schedule one wake after a draw that records fresh modal capacity.
- Audit verdicts kept: proxy doors (rebels/nethack/dcss/usurper/
  dopewars) dirty solely via pending_terminal_commands (correct). The
  now_playing/radio_meta peek (tick.rs) is safe only because render
  unconditionally marks both seen (render.rs:374-384); keep pairing.
- Tests: settle tests unchanged and green; new
  `open_ultimate_modal_settles_clean_then_fires_once_on_ready`
  (tick_test.rs), traffic pause gate (traffic/state_test.rs),
  minute-granularity label pin (ultimates_test.rs).

## Phase 2 — adaptive world tick (2026-07-23, shipped)

The fixed 66ms interval is gone from both loops. Each render pass returns
`App::wake_hint() -> Duration` (computed under the app lock, after the
draw, so it sees draw-recorded slots) and the loop sleeps exactly that
long unless input or a RenderSignal wake lands first. Three tiers:

- HOT_TICK 66ms: splash, post-input window (2s after any input, so menu
  loads and chat send echo keep typing latency), active ultimate effect,
  HouseTable screen, Arcade with a game open, bonsai modals (the care
  watering animation still counts per tick call).
- ANIM_HALF_TICK 132ms: Clubhouse screen, a visible right sidebar (the
  ambient wave), pet roaming, or the pet strip drawn, all painting on the
  /2 `anim_half` edge (~7.5fps). The pet's clocks are wall-synced
  (`PetState::tick(wall_tick)`, 2026-07-23), so its wake matches its
  paint edge.
- ANIM_QUARTER_TICK 264ms: aquarium tray + profile reef, stepping on the
  /4 `anim_quarter` edge (~3.8fps).
- IDLE_TICK 500ms floor: everything else. Ticks only drain channels;
  worst-case latency for an unprompted event (chat message while idle) is
  one floor interval. Input/resize/door-proxy wakes remain instant.

Enablers shipped with it:
- `marquee_tick` is derived from wall clock (elapsed/66ms), so phase
  consumers (marquee_text, shimmer_phase, blink) stay correct under
  sparse ticks; every `is_multiple_of` EDGE check became a
  period-index-vs-previous-tick compare (shared `one_hz` edge in tick(),
  clubhouse /4, marquee step). `is_multiple_of` on the tick counter is
  now a bug pattern: it misses boundaries under sparse ticking.
- Bonsai passive growth REMOVED entirely (product decision): classic
  bonsai grows from watering only (tick keeps only the death check),
  bonsai_v2 lost GrowthCause::Passive + the activity-window machinery.
  This deleted the only wall-time accumulators that adaptive cadence
  would have broken.
- Budget-stall loop re-polls at HOT_TICK (a past deadline would spin);
  dropped-frame repaint retry pinned to HOT_TICK.
- Kept: OutputBudget guard, MIN_RENDER_GAP throttle, biased ordering
  (world deadline wins ties), dirty/Notify two-primitive invariant,
  `notify_turn_edges` running every tick (now at the session's cadence).
- ssh_internal_test loop tests rewritten for the deadline signature;
  `wake_hint_idles_when_settled_and_heats_on_input` pins the tier
  contract.

Result: idle sessions = 2 cheap clean ticks/sec, ~1 render/min. Renders
were already gated by PR1/PR2; phase 2 removes the idle wakeup churn.

### Experiment in flight (2026-07-23)

- One shared half-rate frame edge (`anim_half` = /2 of marquee_tick,
  steady ~7.5fps on any grid): pet strip, roam overlay, bonsai modal
  sway, clubhouse ambience (raised from 4fps), and both aquarium
  surfaces paint on it. The viz first went back to full hot rate, then
  (same day) was replaced by the stateless ambient wave riding this
  same edge; nothing paints hot for music anymore.
- The aquarium lost its private clock entirely ("two clocks through the
  app"): SIMULATION_STEP/last_step_at deleted, tick() = exactly one sim
  step, driven on the `anim_quarter` /4 edge (264ms, ~3.8fps, close to
  the old 220ms feel; 132ms made the fish visibly too fast) - tray in
  App::tick, profile reef via step_reef in App::tick, draw no longer
  ticks it.
- Wake tiers are now HOT 66ms / ANIM_HALF_TICK 132ms (Clubhouse) /
  ANIM_QUARTER_TICK 264ms (aquarium surfaces) / IDLE 500ms. All frame
  edges divide the one wall clock. Pet + bonsai wake hot (their
  per-call steppers are tuned for 66ms) but paint on the half edge.
- Per-session debug stats: each loop logs drawn vs skipped_clean every
  5s at debug level ("render stats, last 5s"); run with
  RUST_LOG=late_ssh=debug to feel the skip ratio locally.
- Revert: drop the anim_half/anim_quarter gates in tick.rs, restore the
  aquarium 220ms self-throttle + draw-time reef tick.

### Phase 2 follow-ups (open, all optional tightening)

- [ ] HouseTable hot tier is coarse (screen == HouseTable). Per-game
      "round running" predicates (tron/ssnake phase, asterion hero_count,
      blackjack phase, poker deadline) would let a quiet table idle.
- [ ] Artboard screen rides the 500ms floor; remote strokes lag up to
      0.5s. Bump to AMBIENT while on-screen if it feels laggy.
- [ ] Push wakes for chat's targeted mpsc would cut the ≤500ms idle chat
      latency to instant; needs the sender side to hold the RenderSignal.
- [ ] Load governor (raise the floor when node CPU is high) not built.

## Working rules

- No commits; user commits constantly (vanishing diff = they committed).
- Another LLM may be editing this repo concurrently. If an unrelated file
  breaks the build, wait and retry, don't fix it.
- Order: PR2 sweep is done. Next: Phase 2 as its own PR; sanity-check the
  prod skip-ratio metrics after PR2 deploys before starting it.
