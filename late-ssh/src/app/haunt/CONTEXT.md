# Haunt Context (late-ssh/src/app/haunt)

## Metadata
- Domain: first contact for deadchannel (GAME.md), currently stage 3 only:
  the splash whisper, "the held door". Admin-scoped scaffolding.
- Last updated: 2026-08-31 (initial build: whisper state machine, splash
  integration, `/haunt` admin controls, kill switch, once-ever mark)
- Status: Active, admin-only by design
- Parent context: `../../../../CONTEXT.md`; design source: `GAME.md`,
  "First contact (the haunting)"

## 1. Summary

The game is never announced; it arrives. This module is the machinery for
the haunting's escalation ladder. Built so far: **stage 3, the whisper**,
delivered on the splash screen once per person ever. That one time the
splash does not skip: input is acknowledged (a static surge, the skip hint
dissolving) but control is withheld, a voiced line types itself in answer,
and a hard cap releases the door no matter what.

**While first contact is admin-scoped scaffolding it must never fire for
real users** (it is a nonrenewable resource, GAME.md). The only gate today
is `is_admin`; the real eligibility campaign (bio, settings, tenure) comes
at design review before the breach.

## 2. Module map

| File | Owns |
|---|---|
| `state.rs` | `WhisperState`, the pure splash-tick state machine: phases (Held -> Typing -> Linger -> Released), the answer beat, the hard cap, surge/dissolve windows, the voiced-line pool. No I/O, no clock reads. |
| `ui.rs` | Pure render helpers: `whisper_frame` (typed line + dissolving hint + surge for one frame), `draw_static_surge` (deterministic scattered block cells, stateless like the sidebar equalizer). |

Integration lives with the owners: `App` arms/owns the whisper
(`state.rs::App::{whisper, replay_whisper, tick_haunt}`), the splash block
in `app/tick.rs` drives it and holds off the 90-tick auto-expiry, the
splash branch of `app/input.rs::handle` routes input to `note_input`
instead of skipping, and the splash branch of `app/render.rs::draw` paints
the line, the hint override, and the surge.

## 3. The contract (hard rules, stated as architecture)

- **Render-only, TUI-only.** No DB chat rows, no IRC projection; the only
  persistence is the once-ever mark
  (`users.settings.first_contact_whisper_done`, late-core `User`).
- **Aesthetic, never system.** Input is never silently ignored (that reads
  as a hung terminal, the exact panic the design forbids): every keypress
  surges static the same frame. Corruption is block glyphs in theme
  colors, obviously voiced; no fake errors, no fake disconnects.
- **The door always opens.** Natural release is linger after the line
  lands (~7.5s worst case); `HARD_CAP_TICKS` (150 splash ticks, ~10s)
  releases whatever the phase, undelivered if the line never finished.
- **Once per person ever.** The mark is spent only on a delivered whisper
  (the line finished); a kill-switch drop or a session lost mid-scene
  leaves it unspent, so the whisper plays again next login.
- **Kill switch from day one.** `State.haunt_enabled` (process-global,
  in-memory, defaults on; a restart re-enables, safe while admin-scoped).
  Checked at arming and on every splash tick: `/haunt off` drops a live
  whisper mid-scene.

## 4. `/haunt` (admin composer command)

Parsed in `chat/state.rs::submit_composer` **only when `is_admin`**; for
everyone else the line posts as plain text, exactly as if the command did
not exist (no usage banner, no autocomplete, no help entry: the mystery is
the feature). Drained by `App::tick_haunt`.

- `/haunt` - status: kill switch, once-ever mark, door armed/idle.
- `/haunt on` / `/haunt off` - the process-global kill switch.
- `/haunt replay` - re-run the splash whisper now, ignoring the mark (a
  completed replay re-spends it through the normal release path).
- `/haunt reset` - clear this user's once-ever mark; arms again next
  session.

## 5. Gotchas

- The machine's clock is `App::splash_ticks` (66ms world ticks while the
  splash is up, hot cadence), not `marquee_tick`: the whisper is splash
  theater and shares the splash's typing clock.
- Any input swallowed by the held door leaves the VT parser mid-escape;
  both the input path and the release path call `vt_input.reset()`, same
  as the normal splash skip (see the Alt-chord comment there).
- The whisper line pool in `state.rs` obeys the screenshot test (static /
  signal / city / channel vocabulary, never Unix internals) and needs
  feed-template-grade variety before it ever leaves admin scope (GAME.md,
  Open questions).
- Stages 1 (deniable chrome corruption) and 2 (echo flicker, composer
  placeholder) are not built; when they land they belong in this domain.
- Test apps pass `first_contact_whisper_done: true` in `test_helpers` so
  the whisper can never arm unless a test replays it on purpose.
