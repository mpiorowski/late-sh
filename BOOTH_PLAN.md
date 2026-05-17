# Music Booth — implementation plan

Self-contained plan for adding the music booth feature on top of the
existing YouTube queue. Read this before touching code; the design itself
lives in `AUDIO.md` §14 and is already final (decisions are baked in, do
not re-ask the user).

## Pre-state when this plan was written

- `main` branch.
- `AUDIO.md` §14 holds the full booth design. Do not relitigate the
  decisions there.
- `late-ssh/src/app/audio/CONTEXT.md` reflects the current
  implementation; update it at the end (§14 has a "still out of scope"
  note that gets shorter as steps land).
- An uncommitted fix sits in the worktree: the clipboard-image upload
  bug where `snapshot()` returned the browser entry once a browser was
  paired, shadowing the CLI's `supports_clipboard_image()`. The fix
  added `PairedClientRegistry::request_clipboard_image(token)` and
  collapsed the two-step check in `App::request_paired_clipboard_image_upload`.
  Tests for that fix were rejected by the user — do not add them back.
  Commit this fix on its own before starting the booth work so the
  blast radius is small.

## Repo conventions to honor

- uuidv7 for all PKs (see other migrations under `late-core/migrations/`).
- Error strings start lowercase (`anyhow!`, `bail!`, `tracing::*!`). UI
  banners keep sentence case.
- No em dash in UI copy or prose.
- No `Co-Authored-By` trailer on commits; match the terse `update`-style
  commits in the log.
- Do not run tests. Do not add tests unless the user explicitly asks.
  Verify changes by `cargo build` and by reading diffs.

## Implementation order

Each step is independent enough to be one commit. Build after each.

### Step 1 — `media_queue_votes` migration and model

Files:
- `late-core/migrations/049_create_media_queue_votes.sql` (new)
- `late-core/src/models/media_queue_vote.rs` (new)
- `late-core/src/models/mod.rs` (declare)

Schema (matches AUDIO.md §14.6 exactly):

```sql
CREATE TABLE media_queue_votes (
    id        UUID PRIMARY KEY DEFAULT uuidv7(),
    created   TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    updated   TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    item_id   UUID NOT NULL REFERENCES media_queue_items(id) ON DELETE CASCADE,
    user_id   UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    value     SMALLINT NOT NULL CHECK (value IN (-1, 1))
);

CREATE UNIQUE INDEX idx_media_queue_votes_user_item
    ON media_queue_votes (user_id, item_id);

CREATE INDEX idx_media_queue_votes_item
    ON media_queue_votes (item_id);
```

Model surface (`MediaQueueVote`):
- `upsert(client, user_id, item_id, value) -> Result<i32>` — returns the
  new aggregate score for the item. Implement via
  `INSERT … ON CONFLICT (user_id, item_id) DO UPDATE SET value = $3, updated = now()`
  then a follow-up `SELECT COALESCE(SUM(value), 0) FROM media_queue_votes
  WHERE item_id = $2`.
- `delete(client, user_id, item_id) -> Result<i32>` — same return shape.
- `aggregate_for_item(client, item_id) -> Result<i32>` for ad-hoc lookups.
- `user_vote(client, user_id, item_id) -> Result<Option<i16>>` for UI
  highlighting (whether the current user has already voted).

Verify: `cargo build -p late-core` cleanly compiles.

### Step 2 — score-aware queue reads

Files:
- `late-core/src/models/media_queue_item.rs` (modify)

Change `first_queued` and `list_snapshot` to LEFT JOIN
`media_queue_votes`, group by item id, and order by
`(COALESCE(SUM(media_queue_votes.value), 0) DESC, media_queue_items.created ASC)`.
Return the score alongside the row so callers can include it in
`queue_update` payloads.

Concretely add a `vote_score: i32` field to whatever struct the snapshot
returns. If the existing struct is shared with WS serialization, default
it to 0 with `#[serde(default)]` for forward-compat — the browser
ignores unknown fields today, but a zero default is the cheaper write.

Verify: `cargo build` of both `late-core` and `late-ssh` succeeds.
Inspect callers (`AudioService::advance_to_next_with_guard`,
`initial_ws_messages`, `resume_from_db`, `snapshot`) and confirm the
new field is plumbed sensibly. They do not need to USE the score — just
not panic on it.

### Step 3 — `AudioService` vote and skip-vote APIs

Files:
- `late-ssh/src/app/audio/svc.rs` (modify)

Add to `QueueState`:
```rust
skip_votes: std::collections::HashSet<uuid::Uuid>,
```
Cleared in every transition that changes `current_item_id`. Add the
clear in `mark_playing`-success branches, in `finish_item*`, and in any
explicit advance path. There are a few — read each transition site.

Add to `AudioService`:

```rust
pub fn cast_vote(&self, user_id: Uuid, item_id: Uuid, value: i16)
    -> oneshot::Receiver<Result<i32>>;
pub fn clear_vote(&self, user_id: Uuid, item_id: Uuid)
    -> oneshot::Receiver<Result<i32>>;
pub fn cast_skip_vote(&self, user_id: Uuid)
    -> oneshot::Receiver<Result<SkipProgress>>;
```

Where `SkipProgress { votes: u32, threshold: u32, fired: bool }`.

Follow the existing `submit_trusted_url` / `submit_trusted_url_task`
pattern: a public method that spawns onto the audio runtime, plus an
internal `_task` that does the real work. Reuse the same `db` pool and
the same `state` mutex.

Vote rules:
- `cast_vote` rejects if the target item is not `status='queued'`
  (return an error whose message classifies cleanly for UI banners —
  see `trusted_submit_error_message` for the existing pattern).
- After a successful upsert/delete, bump `state.sequence`, recompute a
  fresh `QueueSnapshot`, and broadcast a new `AudioWsMessage::QueueUpdate`
  through `ws_tx`. The browser ignores unknown fields, so adding
  `vote_score` and `skip_progress` to the payload is forward-compat.

Skip-vote rules:
- Add the user id to `state.skip_votes`. Compute the threshold from
  the `PairedClientRegistry`'s total entry count across all `client_kind`
  values: `threshold = ((paired_total as f32 * 0.1).ceil() as u32).max(1)`.
  Expose a getter on the registry if needed (`fn total_pairings(&self) -> usize`).
- If `state.skip_votes.len() as u32 >= threshold`, advance the current
  item to `STATUS_SKIPPED` via `MediaQueueItem::update_status`, broadcast
  the next item via the existing advance path, set `fired = true`.
- Also re-evaluate the threshold on paired-client disconnect. The
  registry's `unregister_if_match` currently has no notify hook. Add a
  callback channel from the registry to the audio service (broadcast or
  watch) and run a recount when entries change. Keep this narrow: the
  callback only fires on register/unregister, not on every state update.

WS payload extensions (add to existing `QueueUpdate` serialization):
- Each queue item entry: `vote_score: i32`.
- Current item: `skip_progress: { votes: u32, threshold: u32 }`.

Verify: `cargo build` succeeds. Read every transition that changes
`current_item_id` and confirm `skip_votes` is cleared. Read the WS
serialization site and confirm the JSON shape is what the browser will
forward-tolerate.

### Step 4 — `AudioState` chat-facing API

Files:
- `late-ssh/src/app/audio/state.rs` (modify)

Add per-session shim methods that proxy to the service and produce
banners via `AudioEvent`:

```rust
pub fn booth_submit_public(&self, url: String);
pub fn booth_vote(&self, item_id: Uuid, value: i16);
pub fn booth_clear_vote(&self, item_id: Uuid);
pub fn booth_skip_vote(&self);
```

Add new `AudioEvent` variants in `svc.rs` for booth banners:
- `BoothSubmitQueued { user_id, position }`
- `BoothSubmitFailed { user_id, message }`
- `BoothVoteApplied { user_id, item_id, score }`
- `BoothVoteFailed { user_id, message }`
- `BoothSkipFired { user_id }`
- `BoothSkipProgress { user_id, votes, threshold }`

`AudioState::tick` already routes per-user banners — extend the match
to render these as success/error banners with sentence-case copy and
no em dash. Reuse `Banner::success` / `Banner::error`.

Verify: `cargo build` succeeds.

### Step 5 — revive `submit_url_task` for public booth

Files:
- `late-ssh/src/app/audio/svc.rs` (modify)
- `late-ssh/src/app/audio/state.rs` (modify)
- `late-ssh/src/main.rs` (verify `LATE_YOUTUBE_API_KEY` plumbing)

`AudioService::submit_url_task` already exists and validates via the
YouTube Data API. Wire `booth_submit_public` to it (not to
`submit_trusted_url_task`).

If `LATE_YOUTUBE_API_KEY` is missing at startup, the service today still
constructs — keep it that way. Add a public method
`booth_submit_enabled(&self) -> bool` that returns whether the key is
configured. The modal disables the submit field when it returns false
and shows a banner ("submissions disabled while server youtube key is
unset" or similar — sentence case, no em dash).

Staff `/audio` continues to use the trusted path; do not change
`submit_trusted_url_task`.

Verify: `cargo build` succeeds.

### Step 6 — TUI modal

Files (all new under `late-ssh/src/app/audio/booth/`):
- `mod.rs` — declarations only (`state`, `input`, `render`).
- `state.rs` — `BoothState { items: Vec<BoothItemRow>, submit_input: String, selected: usize, focus: BoothFocus }`. `BoothFocus = { Submit, List }`.
- `input.rs` — keybind table while modal is open: arrow keys to navigate
  list, `+` / `-` to vote, `Enter` on the submit field to call
  `booth_submit_public`, `Esc` to close.
- `render.rs` — modal layout: title, submit input row (disabled if
  `!booth_submit_enabled()`), current-track row (no vote controls,
  shows `skip: N/M`), queue list with score column.

Open the modal via the existing modal-stack mechanism (read another
modal under `late-ssh/src/app/` for the pattern — there are several).
Hide it cleanly when the chat compose mode reactivates.

Pull queue state directly from the DB through `MediaQueueItem` queries
(not via the audio service `snapshot`, because we want fresh reads on
every poll). Subscribe to `AudioEvent` for live changes so the modal
refreshes without polling.

Verify: `cargo build` succeeds. Open the modal manually if a dev env
is available; if not, leave a comment that a smoke test is the next
step.

### Step 7 — keybinds

Files:
- `late-ssh/src/app/chat/input.rs` (modify) or wherever the chat
  keybind table lives — read it first; the existing `/audio` dispatch
  is around line 131-136.

Register a `v` prefix-armed handler analogous to the existing
`vote_prefix_armed` (see `late-ssh/src/app/input.rs:1792`). On
`v` + `v` open the booth modal. On `v` + `s` call `booth_skip_vote`.

Existing `v`-prefix usage in `app/input.rs:1792` is for the genre vote.
Make sure the new chat-side `v+v` / `v+s` does not collide — check
whether the genre vote keybind also fires in chat. If they overlap,
add the new prefix only when chat is the active screen.

Verify: `cargo build` succeeds.

### Step 8 — browser `queue_update` payload extension

Files:
- `late-web/src/pages/connect/page.html` (verify — likely no change needed)

Confirm the connect page already ignores unknown fields on
`queue_update`. If it does (it should, the existing JS parses by event
name), no change is needed for this iteration. If it logs on unknown
fields, gate that log or silence it.

The browser does NOT vote in this iteration; do not add UI for it.

Verify: open the connect page manually if a dev env is available.

### Step 9 — doc cleanup

Files:
- `AUDIO.md` (modify §10 done list)
- `late-ssh/src/app/audio/CONTEXT.md` (modify §2 file map, §13 gaps)

Move the relevant bullets from "deferred" to "done" in `AUDIO.md` §10.
Update the file map in CONTEXT.md to include the new `booth/`
subdirectory and the new model. Update the gap list — remove any
items the booth now addresses.

Do not touch §14 of AUDIO.md other than turning "(planned)" in the
heading into "(shipped)" if all steps landed.

## What is explicitly NOT in scope

(repeating from AUDIO.md §14.11 for grep convenience)

- Browser-side voting UI.
- Weighted votes by role.
- Vote history / reputation.
- Public `POST /api/queue/submit` HTTP route.
- `GET /api/queue` HTTP route — TUI reads DB directly, browser uses WS
  catch-up.

## Open questions left for the implementer

None. AUDIO.md §14 has answers for everything the user has been asked
about. If something genuinely seems undecided when you read this, the
right move is to read §14 again before asking — the answer is probably
there.
