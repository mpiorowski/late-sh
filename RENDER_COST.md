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
- Product decision: the only animations worth full rate are cats (pet), fish
  (aquarium), music (visualizer), bonsai. Everything else slow/static.
  Marquee: 1 column/sec (`MARQUEE_STEP_TICKS = 15`, hold 45); every marquee
  transition lands on a multiple of the step (tested).
- Metrics: `late_ssh_renders_total{reason=input|tick}` vs
  `late_ssh_renders_skipped_clean_total` (metrics.rs, `RenderReason` closed
  enum) observe the skip ratio in prod; they do not gate the work.

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
aquarium (entitlement-gated, 220ms self-throttle) + viz settle
(SETTLE_EPSILON). `Outbox::has_pending()` forces a frame for outbox/terminal
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
- Visualizer: still ticks every frame (decay must settle) but only reports
  changed while the right sidebar or a bonsai modal is visible.
  `sidebar_visible` computed once in tick, shared with the marquee block.
- Pet strip: `PetState::tick() -> bool` (feedback expiry, roam end,
  day-rollover mood/needs flips via `last_visual` compare). Animation pays
  exactly on frame boundaries: `pet::ui::strip_frame_changed(mood, tick,
  travel)` vs the `last_pet_strip_travel` Cell slot recorded at draw (same
  pattern as the click-target rect slots; slot reset each render, None =
  strip not drawn = no frames). Roaming overlay stays full-rate. Sad parked
  pet = fully static. Tests: pet/ui_test.rs.
- Modals now event-driven via their tick paths: settings (`SettingsTick`),
  profile modal (`tick() -> bool`), hub admin (`AdminTick.changed`), audio
  (`AudioTick.changed` = queue-snapshot peek, marked seen in tick; covers
  booth modal + sidebar stage), bonsai care (`tick() -> bool` while watering
  animation plays). Dropped from the coarse list entirely: hub, poll, icon
  picker, room search (results ride chat events), booth, bonsai_v2 (growth
  via bonsai_v2_state.tick, sway via viz gate).
- Scoped cadences kept: lobby modal 1Hz (live occupancy read at draw),
  ultimate modal 1Hz only while `has_cooldown_running()`, profile modal hot
  only while `aquarium_animating()` (live reef ticks during draw),
  DailyMatch screen 1Hz (deadline clock; board is otherwise event-driven).
- Artboard: `DartboardState::tick() -> bool` (snapshot watch + event_rx +
  archive loader peeks; archive view deliberately excludes the latched
  snapshot peek).
- World Cup screen removed entirely (separate concurrent work), so its
  tightening item is moot.

## PR2 — still open: domain-by-domain sweep

No Grafana gating (user decision): work through the remaining domains one
at a time, tightening each to real change signals. Per domain: read its
CONTEXT.md, find every path that can dirty the frame, convert coarse spots,
add the settle-style test, tick the box here.

Scouted 2026-07-22 (5 read-only agents); findings below are the plan.

- [ ] Clubhouse (blanket at tick.rs:53-56): DECIDED, sub-rate it. Walker
      positions are input-driven, dog step already wall-clock (600ms);
      only cosmetic ambience loops on anim_tick (jukebox EQ t/2 fastest,
      emote arms/arcade/neon t/4, fire/candles/stars t/6-t/10). Gate:
      `anim_tick % 4 == 0` OR discrete signals (input handled, door
      events, new bubble/banner lines, roster diff at ROSTER_REFRESH_TICKS).
- [ ] House tables (blanket at tick.rs:336-339): all five runtimes already
      peek watch channels with `has_changed()` in their state.rs tick();
      server loops go quiet when idle (tron/ssnake self-terminate off
      Running, asterion gates on hero_count>0, blackjack republishes 1s
      countdowns + 900ms dealer sweep steps). Convert to per-game changed
      = watch peek, plus TWO local additions: poker 1Hz while
      `action_deadline.is_some()` (clock computed at draw, never
      re-pushed; copy the tick.rs ultimate-modal 1Hz pattern) and
      asterion FLASH_TTL (1500ms) expiry poll.
- [ ] Lateania (shared blanket at tick.rs:358-361): fully snapshot-driven.
      One `watch::Receiver<MudSnapshot>`; server publishes only on
      mark_world_dirty, no client-side animation. `State::tick()`
      (lateania/state.rs:122) already does has_changed/borrow_and_update,
      just discards it: return bool (snapshot landed | join_pending
      transition | reset_elsewhere).
- [ ] Green Dragon (same shared blanket): NO chatter animation exists;
      that suspicion was inherited from Lateania's comment, not verified.
      All tick_* fns are one-shot watch drains created by menu opens;
      only timer is a 4-min presence save (renders nothing). Return bool
      from `State::tick()` = any drain landed.
- [ ] Traffic arcade game: the one arcade holdout. tick.rs:305-309 forces
      changed=true even while paused; `traffic/state.rs:468 tick()`
      returns (). Convert to `-> bool` like tetris/snake.
- [ ] Heartbeat nit: tick.rs:47-49 counts SessionMessage::Heartbeat
      (no-op) as a change; exclude it from the emptiness check.
- [ ] Chat image modal (blanket at tick.rs:1020, justified for now): Sixel
      fetch runs inside render (render.rs:390) and needs
      `image_modal_capacity` recorded by the PREVIOUS draw
      (chat/ui.rs:1712 set_modal_capacity -> render.rs:1054 feedback ->
      chat/state.rs:3009 consumer). Restructure: compute popup dims from
      layout ahead of draw, move fetch into chat.tick(); only then
      un-coarsen. The one substantial remaining task.

Audit: everything else non-excluded is already real signals or documented
fixed cadences (lobby modal/ultimate/DailyMatch 1Hz, pet roam full-rate
by design). Proxy doors (rebels/nethack/dcss/usurper/dopewars) dirty
solely via pending_terminal_commands: correct, leave alone. The
now_playing/radio_meta peek (tick.rs:992) is safe only because render
unconditionally marks both seen (render.rs:374-384); keep that pairing.

## Phase 2 — event-driven loop (kill the 66ms tick), own PR

Loop shape: `select! { input, RenderSignal wake, per-session channels,
sleep_until(next deadline) }` -> drain -> advance state by ELAPSED WALL TIME
-> render if dirty, coalesced to 100ms min-gap, defer-not-drop -> recompute
deadline.
- `App::next_frame_at() -> Option<Instant>`: min over VISIBLE deadlines:
  marquee next boundary while scrolling (1s), shimmer 1s (only when flair
  present), pet blink/wander boundaries, clock next-minute, splash,
  ultimates expiry, aquarium 220ms while tray visible, viz while
  has_viz/procedural, banner expiry. Static screens -> None -> 0fps idle.
- Tick counters -> wall-clock: marquee_tick consumers (marquee_text,
  shimmer_phase, splash_ticks, pet animation_ticks, bonsai
  GROWTH_TICK_INTERVAL 9000 ticks ~= 10min, bonsai_v2
  PASSIVE_GROWTH_ACTIVE_TICK_INTERVAL, blink tick%64). Keep per-session
  bonsai growth semantics (CONTEXT §8.4).
- Push wakes already exist: input, resize, all five door proxies call
  `RenderSignal::wake()`. Chat has per-session targeted mpsc. Remaining
  services need forwarder arms or ride deadlines.
- The per-subsystem changed bools from PR1/PR2 are the dirty inputs; reuse
  as-is.
- Keep: OutputBudget guard, MIN_RENDER_GAP throttle, biased-equivalent
  ordering (input flood must not starve deadline renders),
  `HouseState::notify_turn_edges` off-screen behavior (runs every tick
  today deliberately).
- Rewrite ssh_internal_test.rs loop tests (stale_permit_does_not_arm_throttle
  etc.) for the new select shape; keep the dirty/Notify two-primitive
  invariant in CONTEXT §2.6; update §2.5/§2.6 diagrams; change ssh.rs AND
  web_tunnel.rs.
- Load governor: global watch<Duration> min-gap raised when node CPU high.

Expected: idle 0-1 fps worst case; mc/session from ~41 toward 5-15; ceiling
~170 -> 400+.

## Working rules

- No commits; user commits constantly (vanishing diff = they committed).
- Another LLM may be editing this repo concurrently. If an unrelated file
  breaks the build, wait and retry, don't fix it.
- Order: finish the PR2 domain sweep -> Phase 2 as its own PR. No Grafana
  gating between steps.
