# late.sh Paired Scratchpad Context

## Metadata
- Domain: `/pair @user` invites and the two-person shared live text scratchpad (`Screen::Scratchpad`)
- Primary audience: LLM agents working in `late-ssh/src/app/scratchpad`, the `/pair` chat command, or `Screen::Scratchpad`
- Last updated: 2026-07-25
- Status: Active (v1)
- Parent context: `../../../../CONTEXT.md`
- Related context: `../chat/CONTEXT.md` (command parsing/dispatch), `../clubhouse/CONTEXT.md` (the `SharedLobby` registry idiom this mirrors)

---

## 1. Scope

Owned by this domain:
- The process-global `SharedScratchpadRegistry`: pending `/pair` invites and live pairings, keyed by user_id.
- The shared `ScratchpadBuffer` two paired users edit together (full-buffer replace + revision counter, no merge).
- Per-session `ScratchpadState`: the local `TextArea`, sync/publish, and Drop-triggered leave notification.
- `Screen::Scratchpad` input/render.

Out of scope (deliberate v1 boundaries, see root roadmap's "Micro-collab tools" line):
- DB persistence. The pairing is in-memory only, same tier as `SessionRegistry`/`active_users`/`clubhouse::lobby::SharedLobby`; it is lost if both sides disconnect or the process restarts.
- Operational-transform/CRDT merge. Every publish overwrites the whole buffer; concurrent edits from both sides can clobber each other. Acceptable for a two-person scratchpad, not for a real collaborative editor.
- More than two participants.
- Syntax highlighting, line numbers, or mouse click targets (keyboard-only editing via `ratatui-textarea`).
- An open/anyone-can-claim invite lobby (unlike `/challenge`): `/pair` only supports the directed `@user` form.

The `/pair @user` command itself is parsed and dispatched from `chat/state.rs`/`chat/input.rs` (composer has no handle on `active_users` or the registry), same shape as directed `/challenge @user`.

---

## 2. File Map

```text
late-ssh/src/app/scratchpad/
├── mod.rs       # declarations only
├── registry.rs  # SharedScratchpadRegistry, ScratchpadBuffer, PendingPairInvite (pub: touched from main.rs)
├── invite.rs    # request_pair_invite, poll_invite, find_active_user_by_username
├── state.rs     # per-session ScratchpadState: TextArea, sync_from_shared, publish, Drop -> registry.leave
├── input.rs     # invite accept/decline prompt + Screen::Scratchpad key routing
└── ui.rs        # header ("paired with @user") + the TextArea widget
```

Cross-crate/cross-module touchpoints:
- `late-ssh/src/state.rs` — root `State::scratchpad_registry` (constructed once in `main.rs`, alongside `clubhouse_lobby`).
- `late-ssh/src/app/state.rs` — `SessionConfig::scratchpad_registry`, `App::{scratchpad_registry, scratchpad, pair_invite_pending}`, and the `Screen::Scratchpad` leave hook in `App::set_screen`.
- `late-ssh/src/session_bootstrap.rs`, `late-ssh/src/ssh.rs`, `late-ssh/src/test_helpers.rs` — thread `scratchpad_registry` through `SessionConfig` construction, same sites as `clubhouse_lobby`.
- `late-ssh/src/app/tick.rs` — polls `invite::poll_invite` and `scratchpad.sync_from_shared()` once per tick.
- `late-ssh/src/app/input.rs` — the `pair_invite_pending` prompt gate (owns input like `show_quit_confirm`) and the `Screen::Scratchpad` dedicated-input dispatch.
- `late-ssh/src/app/render.rs` — `DrawContext::scratchpad`, the `Screen::Scratchpad` draw arm, and the page-title match.
- `late-ssh/src/app/common/textarea_input.rs` — `handle_freeform_edit` (Enter inserts a newline instead of submitting).
- `late-ssh/src/app/chat/commands.rs`, `chat/state.rs`, `chat/input.rs` — `/pair` autocomplete entry, `PairRequest`/`parse_pair_command`, and the post-submit dispatch into `invite::request_pair_invite`.

Keep `mod.rs` declaration-only.

---

## 3. Registry and Buffer Model

`SharedScratchpadRegistry` (`registry.rs`) is a single-replica, in-process `Arc<Mutex<..>>`, modeled directly on `clubhouse::lobby::SharedLobby`:
- `pending_invites: HashMap<Uuid, PendingPairInvite>` — directed invites keyed by *target* user_id. Overwritten by a newer invite (last-one-wins), same as directed daily challenges.
- `pairings: HashMap<Uuid, SharedScratchpad>` — one shared buffer indexed under *both* participants' user_ids.

`ScratchpadBuffer` holds `content: String`, `revision: u64`, both participants' `(Uuid, String)`, a presence-only cursor per side, and `left: Option<Uuid>`. `leave(user_id)` marks `left` on first departure (partner's next sync sees it) and only removes both registry entries once both sides have left.

There is no live cross-session push in this codebase for this kind of invite (`/challenge` is DB-row-based, not a push). Invites and partner edits are both picked up by polling once per tick (`tick.rs`), same cadence as the `session_rx` drain — worst case latency is the idle tick floor (~500ms).

---

## 4. Per-Session State and Editing

`ScratchpadState` wraps a `ratatui-textarea::TextArea` seeded from the shared buffer's content. `publish()` writes the local editor's full content + own cursor to the shared buffer and bumps `revision`; `sync_from_shared()` pulls the partner's content when `revision` has advanced past what this session last saw (so publishing your own edit does not immediately bounce back and reset your cursor). Its own `Drop` impl calls `registry.leave(...)`, so a hard disconnect notifies the partner exactly like an explicit Esc-leave — no separate `leave_scratchpad` method is needed; `App::set_screen` just drops the `Option<ScratchpadState>` field.

Editing reuses `handle_freeform_edit` (`common/textarea_input.rs`), a third sibling to `handle_single_line_edit`/`handle_multiline_edit`: identical keymap, except Enter inserts a newline (real-editor convention) instead of submitting, and Esc yields `Cancel`, which the caller treats as "leave the pairing."

---

## 5. Invariants

1. **In-memory only.** No DB row, no migration. Do not add persistence without re-scoping this domain.
2. **Registry keyed by user_id, not session token.** A user can have multiple simultaneous SSH sessions; the registry only cares about the human.
3. **Full-replace, no merge.** `publish` always overwrites the whole buffer. Do not add per-keystroke op diffing or OT/CRDT without a real design pass.
4. **Teardown only after both sides have left.** `leave()` must not remove the buffer while the other participant might still be reading `left`/content.
5. **`mod.rs` stays declaration-only.** Free functions (`invite::request_pair_invite`, `invite::poll_invite`) live in their own file, per the repo-wide rule.

---

## 6. Known Gaps / Backlog

- No partner-cursor glyph rendered inline (only a "(line N)" hint in the header) — the plan flagged this as optional for v1.
- No accept/decline audit trail or notification if an invite is dismissed.
- A target with multiple concurrent SSH sessions only has the first session that ticks after the invite lands enter `Screen::Scratchpad`; other sessions of that user are unaffected (same rough edge as `DailyMatch`/`HouseTable` being per-session screen state over a shared record).
- This is the "shared scratchpad" half of the root roadmap's "Micro-collab tools (shared scratchpad, snippet paste, pairing ping)" line; snippet paste is still open.

---

## 7. Testing Guidance

- `registry_test.rs` — invite/take/accept/leave/teardown/overwrite semantics on `SharedScratchpadRegistry`.
- `state_test.rs` — seeding from the shared buffer, publish/sync round-trips, own-publish does not bounce back, `partner_left` after Drop.
- `invite_test.rs` — `find_active_user_by_username` case-insensitivity and not-found paths.
- `chat/state_internal_test.rs` — `parse_pair_command` accept/reject/ignore cases.
- `common/textarea_input_test.rs` — `handle_freeform_edit` newline-on-Enter and Cancel-on-Esc.

---

## 8. References

- Root context: `../../../../CONTEXT.md`
- Chat context: `../chat/CONTEXT.md`
- Clubhouse lobby (registry idiom precedent): `late-ssh/src/app/clubhouse/lobby.rs`
- Daily challenges (directed-invite parsing precedent): `late-ssh/src/app/lobby/daily/CONTEXT.md`
