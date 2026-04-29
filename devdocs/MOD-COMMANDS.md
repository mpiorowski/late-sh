# Plan: `/mod` Command Seam

Target repo: `/Users/mclark/p/my/mevanlc-late-sh`

Target branch: `mevanlc--mod-commands`

Reference repo/branch: `/Users/mclark/p/my/mevanlc-late-sh-cc` on `feat/admin-and-mod-tools`

## Goal

Split a smaller, reviewable PR out of `feat/admin-and-mod-tools` that introduces the moderation foundations and exposes them through one focused terminal surface:

```text
/mod
```

The bare `/mod` chat command opens a moderation modal. The modal contains a command input and a status log. Moderation subcommands are executed inside that modal first, rather than from normal chat, so command output can be multiline and durable without fighting chat banners/overlays.

## Non-goals

- Do not port the full Control Center UI from the reference branch.
- Do not add normal chat execution for `/mod <subcommand> ...` in this PR, beyond optionally printing "open /mod first" if arguments are supplied in chat.
- Do not import unrelated reference-branch work such as broad prototype docs, large UI navigation changes, or visual Control Center layouts.
- Do not make a perfect long-term command language. Build a small parser that is easy to replace once the final mod surface settles.

## Source Map From Reference Branch

Use the reference branch as a source, but cherry-pick in spirit rather than copying every file wholesale.

Keep/adapt:

- `late-ssh/src/authz.rs`
  - `Permissions`, `Tier`, `TargetTier`, `Decision`, `Action`, audit log rules, and permission matrix tests.
  - Add `pub mod authz;` to `late-ssh/src/lib.rs`.
- DB/model foundations:
  - `users.is_moderator`
  - `moderation_audit_log`
  - `room_bans`
  - `server_bans`
  - `artboard_bans`
  - model files for moderation audit log, room ban, server ban, and artboard ban.
- Service/action functions from `late-ssh/src/app/chat/svc.rs`:
  - staff/user/room/audit query helpers
  - room kick/ban/unban
  - room rename/visibility/delete where permissions allow
  - server disconnect/ban/unban
  - artboard ban/unban
  - tier changes for grant/revoke moderator and grant admin if included
  - privileged delete/edit audit logging as needed by the imported action set.
- Permission checks/gates:
  - use `Permissions` at call sites instead of passing bare `is_admin` where the PR touches behavior.
  - announcements posting remains admin-only.
  - privileged message deletion/editing routes through the matrix.
  - room join/invite/send checks respect room bans.
  - artboard edit checks respect artboard bans if this can be kept scoped.

Leave behind:

- `late-ssh/src/app/control_center/*`
- `late-ssh/src/app/confirm_dialog/*`, unless a tiny reusable confirmation is truly needed.
- Large render/input changes whose only purpose is Control Center navigation.
- Prototype/planning docs from `docs/`.
- Reference branch migration `037_create_ssh_session_events.sql` unless a later implementation step proves it is required for a `/mod sessions` query. Prefer the existing in-memory `SessionRegistry` for live session info in this PR.

## Migration Numbering

The target repo already has migrations through `036_collapse_mention_reads_to_cursor.sql`. The reference branch has a conflicting `031_add_moderation_foundations.sql`.

Use new target-side migration names:

- `late-core/migrations/037_add_moderation_foundations.sql`
  - add `users.is_moderator`
  - create `moderation_audit_log`
  - create `room_bans`
  - create `server_bans`
- `late-core/migrations/038_create_artboard_bans.sql`
  - create `artboard_bans`

If another migration appears before implementation lands, renumber forward instead of creating duplicate prefixes.

## Command Surface

The command modal should accept commands with or without a leading `/mod`. For example, inside the modal these are equivalent:

```text
help
/mod help
```

Start with this subcommand set:

```text
help
status
whoami
users [filter]
user @name
sessions [@name]
audit [filter]
rooms [filter]
room #slug
room kick #slug @name [reason...]
room ban #slug @name [duration] [reason...]
room unban #slug @name
room rename #old #new
room public #slug
room private #slug
room delete #slug
server disconnect @name [reason...]
server ban @name [duration] [reason...]
server unban @name
artboard ban @name [duration] [reason...]
artboard unban @name
grant mod @name
revoke mod @name
grant admin @name
```

Duration grammar can match the reference Control Center parser:

```text
30s
15m
24h
7d
```

An omitted duration means permanent where the action supports permanence. If this is too risky for moderator-level actions, the matrix should deny permanent server bans for moderators and allow them only for admins.

Output rules:

- Every command appends at least one status-log entry.
- Mutating commands append a progress line immediately, then a success/failure line when the async service event returns.
- Query commands append formatted rows directly into the modal log.
- Keep output plain text and compact; avoid adding a second complex table renderer in this PR.

## Modal Design

Add a small module:

```text
late-ssh/src/app/mod_modal/
  mod.rs
  state.rs
  input.rs
  ui.rs
```

State shape:

- `open: bool` can live on `App`, matching existing modal booleans.
- `ModModalState` owns:
  - command input `TextArea<'static>`
  - `VecDeque<ModLogEntry>` or `Vec<String>` status log
  - scroll offset
  - optional busy/pending request ids if useful
- Input supports:
  - `Esc`: close modal
  - `Enter`: submit current command
  - `Ctrl+L`: clear log
  - arrows/Home/End/readline-like movement for the input
  - PageUp/PageDown or mouse wheel for log scroll

Rendering:

- Reuse existing modal visual conventions from `settings_modal`, `profile_modal`, and `help_modal`.
- Layout:
  - title line: `Moderation`
  - upper area: scrollable status log
  - lower one-line command input
- No marketing/help card in the main app. `help` is a command inside the modal.

Chat entrypoint:

- In `ChatState::submit_composer`, recognize exactly bare `/mod`.
- Clear composer and set a request flag, similar to existing `/settings`, `/binds`, and `/music` flows.
- Let `App` open the modal from that request flag so chat state does not own top-level modal booleans.
- If normal chat receives `/mod anything`, return a banner such as `Open /mod first, then run: anything`.

## Execution Architecture

Prefer service methods over database access from modal state.

Suggested flow:

1. `ModModalState` parses input into a `ModCommand`.
2. `App`/input handler calls a `ChatState` method such as `submit_mod_command(command)`.
3. `ChatState` calls existing/imported `ChatService` async task methods with:
   - current user id
   - current `Permissions`
   - target ids/slugs/usernames
4. `ChatEvent` carries command results back to `ChatState`.
5. `ChatState` records a structured `ModCommandResult`.
6. `App` drains those results into `ModModalState` log entries during tick/event handling.

This keeps the modal as presentation/input state and keeps moderation behavior close to the existing chat/domain service.

If introducing a `ModService` is cleaner during implementation, keep it thin and backed by the same DB models. Avoid duplicating logic between chat service and mod commands.

## Permissions Integration

Current target code mostly passes `is_admin`. Convert narrowly:

- Add `Permissions` to `AppConfig` and `App`.
- Keep `is_admin` as a compatibility field only where existing UI still expects it, deriving from `permissions.is_admin()`.
- Update SSH login/session construction to build `Permissions::new(user.is_admin, user.is_moderator)`.
- Update web/chat paths that do not have a logged-in staff identity to use `Permissions::default()`.
- Update tests/helpers to accept permissions where admin/mod behavior is exercised.

Important matrix expectations:

- Regular users cannot open/use the modal beyond an access-denied log line.
- Moderators can moderate regular users.
- Moderators cannot moderate admins.
- Moderator-on-moderator actions require the matrix cell chosen from the reference branch; likely admin-only for server bans/tier changes.
- Admins can exercise all implemented mod/admin-like actions except explicitly denied self/admin destructive actions.
- Permanent server bans are admin-only.
- All privileged mutations write `moderation_audit_log` rows when the matrix says they should.

## Data/Model Work

Add or adapt:

- `late-core/src/models/moderation_audit_log.rs`
- `late-core/src/models/room_ban.rs`
- `late-core/src/models/server_ban.rs`
- `late-core/src/models/artboard_ban.rs`

Register the modules in `late-core/src/models/mod.rs`.

Update:

- `late-core/src/models/user.rs`
  - include generated `is_moderator`
  - add helper methods only when they simplify command implementation, such as staff listing or role updates.
- `late-core/src/models/chat_room_member.rs`
  - reject joining room when an active room ban exists.
- `late-core/src/models/chat_room.rs`
  - import only room helper methods needed by `/mod rooms` and room actions.

## Service Work

Port a scoped subset from reference `ChatService`:

- Query commands:
  - list users/staff-ish user records
  - user detail
  - list rooms
  - room detail
  - audit log list
  - live sessions from `SessionRegistry`
- Mutations:
  - `moderate_room_member_task`
  - `admin_room_task`
  - `admin_user_task`
  - `artboard_user_task`
  - `change_user_tier_task`

Do not expose large Control Center snapshot structures if the modal can render simple returned lines instead. If a reference helper returns rich structs and it is easier to test, keep the struct but render it plainly in the modal.

## Parser Plan

Create a small parser module, probably under `mod_modal/state.rs` or `chat/state.rs` if service access is easier.

Parser rules:

- Trim whitespace.
- Drop an optional leading `/mod`.
- Split shell-like enough for usernames/slugs/durations, but keep reason text as the rest of the line.
- Support `@username` and `username`.
- Support `#slug` and `slug`.
- Return `Result<ModCommand, String>` so parse errors become log entries.

Initial tests should cover:

- optional `/mod` prefix
- bare/empty command maps to help or an error
- duration parsing
- reason capture
- rejecting unknown command families
- rejecting missing target username/slug

## Event/Result Handling

Reference branch already adds many `ChatEvent` variants for moderation. For this smaller PR, consider replacing UI-specific events with a command-oriented event:

```rust
ModCommandOutput {
    user_id: Uuid,
    request_id: Uuid,
    lines: Vec<String>,
    success: bool,
}
```

This can coexist with existing chat events and avoids copying Control Center-specific event plumbing.

Mutating service methods should still be tested directly, independent of modal output.

## Implementation Phases

### Phase 1: Authz and DB Foundations

- Add `authz.rs` and tests.
- Add migrations with corrected numbering.
- Add moderation model files and register them.
- Update `User` for `is_moderator`.
- Run:
  - `cargo test -p late-ssh authz`
  - `cargo check`

### Phase 2: Permission Plumbing

- Introduce `Permissions` into app/session config.
- Update SSH login construction.
- Preserve old `is_admin` reads as derived compatibility.
- Update test helpers.
- Run:
  - `cargo check`
  - focused app/helper tests if compile fallout is broad.

### Phase 3: Core Mod Actions

- Port service functions for:
  - room kick/ban/unban
  - server disconnect/ban/unban
  - artboard ban/unban
  - room rename/public/private/delete
  - grant/revoke mod and grant admin if time permits
- Ensure audit logging happens in service functions, not modal code.
- Add focused service tests from the reference branch, trimmed to this PR.
- Run:
  - `cargo test -p late-core moderation`
  - `cargo test -p late-ssh chat::svc`

### Phase 4: Modal UI Shell

- Add `mod_modal` module.
- Add `App` fields and render/input routing.
- Add bare `/mod` chat command that opens the modal.
- Add modal log, command input, close/clear/scroll behavior.
- Run:
  - `cargo check`
  - existing app input smoke tests.

### Phase 5: Command Parser and Execution

- Implement `ModCommand` parser.
- Wire commands to service tasks.
- Add modal output event plumbing.
- Add info-gathering commands first (`help`, `status`, `whoami`, `users`, `rooms`, `audit`) so the modal can be exercised before destructive commands.
- Add mutating commands.
- Run:
  - parser unit tests
  - service tests
  - app input flow tests for opening modal and one representative command.

### Phase 6: Audit and Ban Gates

- Wire room ban checks into message send/join/invite call sites.
- Wire artboard edit ban check if it can be done without dragging in unrelated UI.
- Wire privileged delete/edit message audit logging for the actions included in the matrix.
- Run:
  - `cargo test -p late-core moderation`
  - `cargo test -p late-ssh chat::svc`
  - `cargo test -p late-ssh app_input_flow`

### Phase 7: Final Validation

- Run `cargo fmt`.
- Run `cargo check`.
- Run a focused test set:
  - `cargo test -p late-ssh authz`
  - `cargo test -p late-core moderation`
  - `cargo test -p late-ssh chat::svc`
  - `cargo test -p late-ssh app_input_flow`
- If time allows, run full `cargo test`.

## Review Shape

Keep the PR story tight:

1. A permissions matrix exists in code.
2. The DB can represent moderation bans and audit records.
3. Service functions enforce permissions and write audit records.
4. A small `/mod` modal provides one place to exercise those functions.

Avoid letting the PR become a second Control Center PR. The modal is the seam; richer staff UI can come later on top of the same foundations.

## Open Assumptions

- The bare `/mod` chat command is allowed for all users, but non-staff users see an access-denied message inside the modal or get denied on open. Prefer denying on open if that is simpler.
- Admin-like commands live under `/mod` for this PR, even if they later move to `/admin` or a richer staff surface.
- We should not import `ssh_session_events` yet; live session information can come from the existing `SessionRegistry`.
- `server ban @name` with no duration is a permanent ban and should be admin-only.
- The first implementation should optimize for testable service behavior over beautiful modal formatting.
