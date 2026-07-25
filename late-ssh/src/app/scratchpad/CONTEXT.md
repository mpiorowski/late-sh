# late.sh Paired Scratchpad Context

## Metadata
- Domain: the mutual `/pair @user` handshake and the two-person shared live text scratchpad (`Screen::Scratchpad`)
- Primary audience: LLM agents working in `late-ssh/src/app/scratchpad`, the `/pair` chat command, or `Screen::Scratchpad`
- Last updated: 2026-07-25
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
- Syntax highlighting, line numbers, or mouse click targets (keyboard-only editing via `ratatui-textarea`).
- An open/anyone-can-claim lobby (unlike `/challenge`): `/pair` only supports the directed `@user` form.
- Undo inside the editor. The buffer is replaced wholesale whenever the other side publishes, so the undo stack holds remote edits and one undo would rewind work that was never yours.

The `/pair @user` command itself is parsed and dispatched from `chat/state.rs`/`chat/input.rs` (composer has no handle on `active_users` or the registry), same shape as directed `/challenge @user`.

---

## 2. File Map

```text
late-ssh/src/app/scratchpad/
├── mod.rs       # declarations only
├── registry.rs  # SharedScratchpadRegistry, ScratchpadBuffer, PairSide/PairOutcome (pub: touched from main.rs)
├── pair.rs      # request_pair (the /pair command), poll (per-tick notice + join)
├── state.rs     # per-session ScratchpadState: TextArea, sync_from_shared, publish, Drop -> registry.leave
├── input.rs     # Screen::Scratchpad key routing (no prompt: pairing is mutual)
└── ui.rs        # header ("paired with @user") + the TextArea widget
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

## 5. Invariants

1. **In-memory only.** No DB row, no migration. Do not add persistence without re-scoping this domain.
2. **Pairings are bound to the session that asked.** A user can hold several concurrent SSH sessions, and only the one that ran `/pair` is pulled into the editor. `poll` filters on the session token; drop that filter and every tty a user has open gets yanked into the same buffer.
3. **Full-replace, no merge.** `publish` always overwrites the whole buffer. Do not add per-keystroke op diffing or OT/CRDT without a real design pass.
4. **Teardown after both sides have left, or immediately if the other side never joined.** `leave()` must not remove the buffer while the other participant might still be reading `left`/content, and must not leave an entry behind for a session that will never poll.
5. **`mod.rs` stays declaration-only.** Free functions (`pair::request_pair`, `pair::poll`) live in their own file, per the repo-wide rule.

---

## 6. Known Gaps / Backlog

- No partner-cursor glyph rendered inline (only a "(line N)" hint in the header); the plan flagged this as optional for v1.
- The notice is a 5-second banner drained by the first session of that user to tick. If they are inside a door game or looking away, they miss it and the intent just expires. Deliberate: a passive miss is the correct failure mode for something another user triggered.
- Concurrent typing loses text. Both sides publish the whole buffer on every keystroke, so whoever publishes second wins the round. This is the no-merge scope boundary from section 1, not a bug to patch piecemeal.
- If the first asker's session dies inside the 10 minute window, the mirror still creates a pairing they never join. It is torn down as soon as the other side leaves (`joined`), but until then that user reads as busy.
- This is the "shared scratchpad" half of the root roadmap's "Micro-collab tools (shared scratchpad, snippet paste, pairing ping)" line; snippet paste is still open.

## 7. Testing Guidance

- `registry_test.rs`: the handshake (one-sided ask waits, mirror pairs, expired ask does not), notice drain-once, the ping cooldown (re-ask is quiet, alternating targets cannot reopen it, a suppressed ask still pairs, it lapses, one asker cannot mute another, pairing clears it), session-scoped pairings, busy refusals on both sides, and leave/teardown including the never-joined partner.
- `state_test.rs`: seeding from the shared buffer, publish/sync round-trips, own-publish does not bounce back, yank register survives a remote sync, `partner_left` after Drop.
- `pair_test.rs`: `find_active_user_by_username` case-insensitivity and not-found paths.
- `chat/state_internal_test.rs`: `parse_pair_command` accept/reject/ignore cases.
- `common/textarea_input_test.rs`: `handle_freeform_edit` newline-on-Enter, Cancel-on-Esc, Tab indents, and no undo.

## 8. References

- Root context: `../../../../CONTEXT.md`
- Chat context: `../chat/CONTEXT.md`
- Clubhouse lobby (registry idiom precedent): `late-ssh/src/app/clubhouse/lobby.rs`
- Daily challenges (directed `@user` command parsing precedent): `late-ssh/src/app/lobby/daily/CONTEXT.md`
