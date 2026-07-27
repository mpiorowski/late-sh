# late.sh Paired Scratchpad Context

## Metadata
- Domain: the mutual `/pair @user` handshake and the two-person shared live text scratchpad (`Screen::Scratchpad`)
- Primary audience: LLM agents working in `late-ssh/src/app/scratchpad`, the `/pair` chat command, or `Screen::Scratchpad`
- Last updated: 2026-07-27
- Status: Active (v1)
- Parent context: `../../../../CONTEXT.md`
- Related context: `../chat/CONTEXT.md` (command parsing/dispatch), `../clubhouse/CONTEXT.md` (the `SharedLobby` registry idiom this mirrors)

---

## 1. Scope

Owned by this domain:
- The process-global `SharedScratchpadRegistry`: one-sided `/pair` intents, the notices they post (and their per-pair cooldown), and live pairings.
- The shared `ScratchpadBuffer` two paired users edit together (full-buffer replace + revision counter, no merge).
- Per-session `ScratchpadState`: the local `TextArea`, sync/publish, and Drop-triggered leave notification.
- `Screen::Scratchpad` input/render.

Out of scope (deliberate v1 boundaries, see root roadmap's "Micro-collab tools" line):
- DB persistence. The pairing is in-memory only, same tier as `SessionRegistry`/`active_users`/`clubhouse::lobby::SharedLobby`; it is lost if both sides disconnect or the process restarts.
- Operational-transform/CRDT merge. Every publish overwrites the whole buffer; concurrent edits from both sides can clobber each other. Acceptable for a two-person scratchpad, not for a real collaborative editor.
- More than two participants.
- Mouse click targets (keyboard-only editing via `ratatui-textarea`).
- An open/anyone-can-claim lobby (unlike `/challenge`): `/pair` only supports the directed `@user` form.
- Per-theme syntax color matching. Highlighting uses one fixed bundled `syntect` theme (`base16-ocean.dark`) regardless of which of this app's 20+ user-selectable `theme::` themes the viewer has picked. A real per-theme mapping was considered and explicitly declined in favor of highlighting accuracy; see section 5 below.
- Undo inside the editor. The buffer is replaced wholesale whenever the other side publishes, so the undo stack holds remote edits and one undo would rewind work that was never yours.

The `/pair @user` command itself is parsed and dispatched from `chat/state.rs`/`chat/input.rs` (composer has no handle on `active_users` or the registry), same shape as directed `/challenge @user`.

---

## 2. File Map

```text
late-ssh/src/app/scratchpad/
├── mod.rs        # declarations only
├── registry.rs   # SharedScratchpadRegistry, ScratchpadBuffer, PairSide/PairOutcome (pub: touched from main.rs)
├── pair.rs       # request_pair (the /pair command), poll (per-tick notice + join)
├── state.rs      # per-session ScratchpadState: TextArea, sync_from_shared, publish, Drop -> registry.leave
├── input.rs      # Screen::Scratchpad key routing (no prompt: pairing is mutual)
├── highlight.rs  # Language cycle, syntect-backed highlighting, the hand-built line-number gutter
└── ui.rs         # header + the custom gutter/highlight render + manual cursor placement
```

Cross-crate/cross-module touchpoints:
- `late-ssh/src/state.rs`: root `State::scratchpad_registry` (constructed once in `main.rs`, alongside `clubhouse_lobby`).
- `late-ssh/src/app/state.rs`: `SessionConfig::scratchpad_registry`, `App::{scratchpad_registry, scratchpad}`, and the `Screen::Scratchpad` leave hook in `App::set_screen`.
- `late-ssh/src/session_bootstrap.rs`, `late-ssh/src/ssh.rs`, `late-ssh/src/test_helpers.rs`: thread `scratchpad_registry` through `SessionConfig` construction, same sites as `clubhouse_lobby`.
- `late-ssh/src/app/tick.rs`: calls `pair::poll` and `scratchpad.sync_from_shared()` once per tick.
- `late-ssh/src/app/input.rs`: the `Screen::Scratchpad` dedicated-input dispatch. Nothing in this domain gates global input.
- `late-ssh/src/app/render.rs`: `DrawContext::scratchpad`, the `Screen::Scratchpad` draw arm, and the page-title match.
- `late-ssh/src/app/common/textarea_input.rs`: `handle_freeform_edit` (Enter inserts a newline, Tab indents, no undo).
- `late-ssh/src/app/chat/commands.rs`, `chat/state.rs`, `chat/input.rs`: `/pair` autocomplete entry, `PairRequest`/`parse_pair_command`, and the post-submit dispatch into `pair::request_pair`.
- `late-ssh/Cargo.toml`: the `syntect` dependency (`default-fancy` feature — see section 5).

Keep `mod.rs` declaration-only.

---

## 3. Registry and Buffer Model

Pairing is a **mutual handshake**, and that is the load-bearing decision here. `/pair @b` from a records a one-sided intent and leaves b a banner; b's session is otherwise untouched. The pairing exists only once b answers `/pair @a`. Nothing in this domain can push state onto a session that did not ask for it, which is why there is no accept/decline prompt and nothing that gates global input. Do not reintroduce a one-sided invite: a remote-triggered prompt that owns the keyboard wedges the session it lands on.

`SharedScratchpadRegistry` (`registry.rs`) is a single-replica, in-process `Arc<Mutex<..>>`, modeled on `clubhouse::lobby::SharedLobby`:
- `intents: HashMap<Uuid, PairIntent>`: one-sided asks keyed by the user who ran `/pair` first, carrying that session's token and the timestamp. A newer ask from the same user overwrites the old one.
- `notices: HashMap<Uuid, PairNotice>`: undrained "@x wants to pair" banners keyed by the target. Purely informational.
- `notified_at: HashMap<(Uuid, Uuid), Instant>`: when each asker last pinged each target, so re-running `/pair` in a loop cannot spam a banner slot. Keyed by both ends on purpose: one asker must not be able to mute anyone else's ask.
- `pairings: HashMap<Uuid, PairedSession>`: one entry per participant, each holding the shared buffer and the **session token** that asked for it.

All three maps expire and are pruned inside `try_pair`, the only place any of them grows. `PAIR_NOTICE_COOLDOWN` is deliberately equal to `PAIR_INTENT_TTL` (10 minutes): you may nudge someone again exactly when your previous ask has lapsed, so the spam guard never outlives the thing it guards and a genuine retry (they were in a door game and missed the banner) is never refused.

The cooldown rate limits the **ping only**. A suppressed ask still records the intent, so the handshake completes normally if the target mirrors it, and `try_pair` reports `AlreadyAsked` rather than `Waiting` so the asker is told they did not nudge anyone. Pairing clears the cooldown between those two, since a completed pairing answered the ask.

`try_pair` does every busy check under the same lock that creates the pairing, so two simultaneous `/pair` commands cannot both win. It returns a closed `PairOutcome` (`Waiting`, `AlreadyAsked`, `Paired`, `AlreadyPaired`, `TargetBusy`), one arm per banner at the call site.

`ScratchpadBuffer` holds `content: String`, `revision: u64`, both participants' `(Uuid, String)`, a presence-only cursor per side, a `joined` flag per side, and `left: Option<Uuid>`. `leave(user_id)` marks `left` on first departure (partner's next sync sees it) and normally removes both registry entries only once both sides have left. The exception is a partner that never joined: their session died between `/pair` and the mirror command, so nobody will ever read `left`, and keeping the entry would mark them paired forever.

There is no live cross-session push in this codebase (`/challenge` is DB-row-based, not a push). Notices, completed pairings, and partner edits are all picked up by polling once per tick (`tick.rs`), same cadence as the `session_rx` drain. Worst case latency is the idle tick floor (~500ms). `pair::poll` answers the notice and the pairing under one lock, and a session already inside a scratchpad skips the registry entirely.

## 4. Per-Session State and Editing

`ScratchpadState` wraps a `ratatui-textarea::TextArea` seeded from the shared buffer's content, and marks its side `joined` as it opens. `publish()` writes the local editor's full content + own cursor to the shared buffer and bumps `revision`; `sync_from_shared()` pulls the partner's content when `revision` has advanced past what this session last saw (so publishing your own edit does not immediately bounce back and reset your cursor), preserving the local yank register across the replace. Its own `Drop` impl calls `registry.leave(...)`, so a hard disconnect notifies the partner exactly like an explicit Esc-leave. No separate `leave_scratchpad` method is needed; `App::set_screen` just drops the `Option<ScratchpadState>` field.

Editing reuses `handle_freeform_edit` (`common/textarea_input.rs`), a third sibling to `handle_single_line_edit`/`handle_multiline_edit`. It departs from them in three places, all because this is a full-screen shared editor rather than a composer:
- Enter inserts a newline instead of submitting.
- Tab indents (four spaces, not a literal tab, since the buffer is shared verbatim and tab width differs per terminal). An unhandled Tab would reach the global page cycle and silently end the pairing; `scratchpad::input` swallows Shift+Tab for the same reason.
- There is no undo. See the scope note in section 1.

Esc leaves the pairing, and it needs **two** handlers, which is easy to get wrong. A lone Esc never reaches the keymap at all: the parser holds it as `pending_escape` to rule out a longer sequence, and `tick.rs` resolves it through `flush_pending_escape` straight into `dispatch_escape`. That is the common case, and the `Screen::Scratchpad` arm there is what actually leaves. The keymap's `EditOutcome::Cancel` covers the other case, an Esc arriving mid-chunk alongside other bytes. Deleting either one leaves Esc broken on a path no unit test exercises; `pair_test.rs` covers the lone-Esc path end to end.

Unlike `Screen::DailyMatch`/`Screen::HouseTable`, the scratchpad's arm in `handle_dedicated_screen_input` (`app/input.rs`) does **not** call `door_games_allows_global_help` before forwarding to the editor. Those two are game boards where `?` never means anything to type, so letting it escape to the global help guide is correct there; the scratchpad is a free-typing text editor, where `?` is just a character (a comment, a question in code) that must be inserted like any other. This was a real bug in the initial implementation, copied verbatim from the daily-board/house-table shape without noticing the difference; `pair_test.rs` covers it. Relatedly, `handle_freeform_edit` needs a `ParsedInput::Byte(byte) if byte.is_ascii_graphic()` fallback arm (matching `handle_single_line_edit`'s convention, unlike `handle_multiline_edit`): some terminals send plain punctuation as a raw byte rather than a parsed `Char`, and without the fallback those keystrokes were silently dropped instead of typed.

## 5. Highlighting and the Line-Number Gutter

`ratatui-textarea` cannot render either of these itself: its `Widget` impl (checked against the vendored 0.9.2 source) supports exactly one uniform `Style` for the whole buffer plus cursor/selection/search overlays, with no hook for per-token colors, and its built-in `set_line_number_style` gutter only applies to that same internal render. Both features therefore live in `ui.rs::draw`, which no longer calls `frame.render_widget(&state.editor, body_area)` — it reads `state.editor.lines()` and `state.editor.screen_cursor()` and builds the display by hand via `highlight.rs`. `TextArea` still owns every bit of editing state (insert/delete/cursor/undo); only its *rendering* was replaced.

- **`Language`** (`highlight.rs`) is a small curated cycle — `Plain, Rust, Python, JavaScript, TypeScript, Go, C, Cpp, Java, Ruby, Bash, Json, Yaml` — not free-form, since syntect bundles ~200 syntaxes and cycling through all of them with one key would be unusable. It lives as a field on the shared `ScratchpadBuffer`, not a `/pair @user <language>` command arg: pairing is a mutual handshake between two independent commands, so a per-command language could leave the two sides disagreeing, where a field on the buffer they already both read cannot. `Ctrl+L` (unbound anywhere else — root `CONTEXT.md` already notes it's no longer a global help key) calls `ScratchpadState::cycle_language`, which mutates the shared buffer directly and bumps `revision` so the partner's next tick picks it up, same mechanism as the `left` bump.
- **Highlighting** uses `syntect`, loaded once process-wide (`OnceLock`s in `highlight.rs`, not per-session) via `SyntaxSet::load_defaults_newlines()` and a single fixed `Theme` (`base16-ocean.dark` from `ThemeSet::load_defaults()`). Both loads are ~0.6ms, so paying them lazily on the first non-`Plain` render is fine. `Language::Plain` skips syntect entirely.
- **Highlighting is the most expensive thing this app draws, so it is cached.** Measured on a release build: a full 8,000-char buffer (`MAX_CHARS`, ~200 lines) costs **~23ms per frame**, against ~15us for the plain-text path, while the render loop's whole input-to-frame budget (`MIN_RENDER_GAP`) is 15ms. Two mitigations, both in `highlight.rs`:
  - `HighlightCache` (owned by `ScratchpadState`, behind a `RefCell` because `ui::draw` only gets `&self`) keys on the **lines themselves**, not `revision`: cursor moves, scrolling, resizes, and partner-cursor updates all re-render without changing text, and arrow keys go through `publish()` and bump `revision` with identical content. Only a real edit re-parses.
  - `ui.rs` passes `visible_end` (`vscroll + viewport height`), so lines below the fold are never styled. syntect's parser is stateful across lines (multi-line strings, block comments), so it must always parse from line 0 — the tail is what can be trimmed, never the head. Measured split: parsing is ~92% of the cost and span-building ~8%, which is why trimming the tail helps and skipping span-building alone would not.
  - Still unfixed: typing near the *bottom* of a large buffer re-parses everything above it, ~23ms per keystroke. The real fix is periodic `HighlightState` checkpoints so an edit resumes from the nearest one. Not worth it at `MAX_CHARS` 8,000; revisit if that cap ever rises.
- **Dependency note:** `syntect` is pulled in with `default-features = false, features = ["default-fancy"]` in `late-ssh/Cargo.toml` — this swaps in the pure-Rust `fancy-regex` engine instead of the default `onig` (C/oniguruma) backend, deliberately avoiding a C build-toolchain dependency (the exact class of cross-compilation risk the door-game prebuilt images caused on Apple Silicon).
- **Theme mismatch is a known, accepted trade-off**, not an oversight: this app has 20+ user-selectable `theme::` themes (Catppuccin variants, Solarized Light/Dark, Gruvbox, Tokyo Night, ...), and a real per-theme color mapping for syntax highlighting was considered and declined in favor of one fixed accurate syntect theme. Colors may clash on some of the lighter themes.
- **Cursor placement without the widget's viewport.** `TextArea::screen_cursor()` returns the cursor's display row/col computed purely from document content + wrap mode (`WrapMode::None` here) — verified in the vendored source that this does **not** depend on the widget having been rendered (the internal `Viewport` scroll-tracking is separate and only updated inside the widget's own render pass, which we no longer call). `ui.rs` computes its own scroll offset (a simple "scroll only enough to keep the cursor visible" clamp, both axes, stateless — recomputed fresh every frame from the current cursor position) and places the real terminal cursor with `frame.set_cursor_position(...)`, the same API already used in `artboard/ui.rs` and `hub/admin/ui.rs`.
- **No selection to reproduce.** `handle_freeform_edit` never puts the textarea into select mode from a keystroke (only `Ctrl+U`'s internal `clear()` helper uses `select_all`+`cut`, non-interactively), so the custom render doesn't need a selection-highlight overlay.

## 6. Invariants

1. **In-memory only.** No DB row, no migration. Do not add persistence without re-scoping this domain.
2. **Pairings are bound to the session that asked.** A user can hold several concurrent SSH sessions, and only the one that ran `/pair` is pulled into the editor. `poll` filters on the session token; drop that filter and every tty a user has open gets yanked into the same buffer.
3. **Full-replace, no merge.** `publish` always overwrites the whole buffer. Do not add per-keystroke op diffing or OT/CRDT without a real design pass.
4. **Teardown after both sides have left, or immediately if the other side never joined.** `leave()` must not remove the buffer while the other participant might still be reading `left`/content, and must not leave an entry behind for a session that will never poll.
5. **`mod.rs` stays declaration-only.** Free functions (`pair::request_pair`, `pair::poll`) live in their own file, per the repo-wide rule.
6. **`highlight.rs` never touches `TextArea`'s rendering.** It only reads `lines()`/`screen_cursor()`. If a future change reintroduces `frame.render_widget(&state.editor, ..)` for the body, highlighting and the gutter silently stop appearing (the widget renders over them), and the manual cursor placement in `ui.rs` and the widget's own cursor would fight over the same terminal cursor.

---

## 7. Known Gaps / Backlog

- No partner-cursor glyph rendered inline (only a "(line N)" hint in the header); the plan flagged this as optional for v1.
- The notice is a 5-second banner drained by the first session of that user to tick. If they are inside a door game or looking away, they miss it and the intent just expires. Deliberate: a passive miss is the correct failure mode for something another user triggered.
- Concurrent typing loses text. Both sides publish the whole buffer on every keystroke, so whoever publishes second wins the round. This is the no-merge scope boundary from section 1, not a bug to patch piecemeal.
- If the first asker's session dies inside the 10 minute window, the mirror still creates a pairing they never join. It is torn down as soon as the other side leaves (`joined`), but until then that user reads as busy.
- This is the "shared scratchpad" half of the root roadmap's "Micro-collab tools (shared scratchpad, snippet paste, pairing ping)" line; snippet paste is still open.
- Highlighting colors do not adapt to the viewer's chosen `theme::` theme (see section 5). No horizontal-scroll indicator: a line pushed off the left/right edge by the cursor gives no visual cue that content is hidden there.

## 8. Testing Guidance

- `registry_test.rs`: the handshake (one-sided ask waits, mirror pairs, expired ask does not), notice drain-once, the ping cooldown (re-ask is quiet, alternating targets cannot reopen it, a suppressed ask still pairs, it lapses, one asker cannot mute another, pairing clears it), session-scoped pairings, busy refusals on both sides, leave/teardown including the never-joined partner, and `cycle_language` bumping `revision`.
- `state_test.rs`: seeding from the shared buffer, publish/sync round-trips, own-publish does not bounce back, yank register survives a remote sync, `partner_left` after Drop, and `cycle_language`/`language()` being visible to the partner without a separate sync step.
- `pair_test.rs`: `find_active_user_by_username` case-insensitivity and not-found paths, plus full two-session flow tests (`two_sessions`/`run_command` helpers) covering the handshake, a one-sided ask leaving the target's screen alone, content sync, Esc leaving, and `?` typing into the buffer instead of opening the global guide.
- `highlight_test.rs`: the language cycle wraps, `Plain` never calls into syntect, a real snippet produces more than one distinct style, and the gutter width matches the digit count of the total line count.
- `chat/state_internal_test.rs`: `parse_pair_command` accept/reject/ignore cases.
- `common/textarea_input_test.rs`: `handle_freeform_edit` newline-on-Enter, Cancel-on-Esc, Tab indents, no undo, and the raw-byte fallback for plain punctuation.

## 9. References

- Root context: `../../../../CONTEXT.md`
- Chat context: `../chat/CONTEXT.md`
- Clubhouse lobby (registry idiom precedent): `late-ssh/src/app/clubhouse/lobby.rs`
- Daily challenges (directed `@user` command parsing precedent): `late-ssh/src/app/lobby/daily/CONTEXT.md`
- Manual `frame.set_cursor_position` precedent: `late-ssh/src/app/artboard/ui.rs`, `late-ssh/src/app/hub/admin/ui.rs`
