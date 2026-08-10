# late.sh Cyberspace Context

## Metadata
- Domain: late.sh as a personal client for cyberspace.online: the Cyberspace rail entry/pane, `/cs` commands, account linking, and the typed v1 API client
- Primary audience: LLM agents working in `late-ssh/src/app/chat/cyberspace`, the `/cs` commands, the `cyberspace_accounts` table, or the AI blocklist for cyberspace.online URLs
- Last updated: 2026-08-09 (room unread dots: the 10-minute badge poll also fetches the chat roster while rooms are pinned, and each rail room row shows a `●` when its `last_message_at` passed our per-room read cursor, migration 139; see section 9)
- Status: Active (v1)
- Parent context: `../CONTEXT.md` (chat), root `../../../../../CONTEXT.md`
- Related context: `../news/` (`is_ai_blocklisted_url` lives in `news/svc.rs`)

---

## 1. Scope

Owned by this domain:
- The typed reqwest client for the cyberspace.online v1 API (`api.rs`): login/refresh, feed, threads, replies, posting, notifications, unread count, and the cIRC roster/history/send/presence calls, all through the `{data}/{error}` envelope, plus the pure SSE frame parser for their realtime database.
- `CyberspaceService` (`svc.rs`): fire-and-forget tasks, the `CsEvent` broadcast, the in-memory per-user id-token cache (with the realtime-database URL that came with it), and `CircRoomSession`, the handle whose lifetime *is* a room's stream and presence.
- The `cyberspace_accounts` row model (`late-core/src/models/cyberspace_account.rs`, migrations 133 + 136 + 137 + 139): one row per user, storing the Firebase refresh token (never the password), the feed read cursor, the pinned cIRC room slugs, and the per-room read cursors behind the rail's unread dots.
- Per-session pane state (`state.rs`): feed/thread/notifications views, the pinned chat rooms and the open one, the link/compose/reply/rooms modals, the unread badge and its poll gating.
- Pane and room input (`input.rs`) and rendering (`ui.rs`), including the unlinked pitch + login funnel.

Out of scope (deliberate boundaries):
- C-Mail (their DMs) and guilds: same mechanism as cIRC, no surface here yet (section 9).
- Their chat, cIRC: the roster, the pinned-room rail rows, the live room surface, and `CircRoomSession`. Section 9 owns it.
- The `/cs` (alias `/cyberspace`) commands themselves are parsed and dispatched from `chat/state.rs` (`parse_cyberspace_command`, handled inline on `ChatState`), and the rail entry is built in `chat/ui.rs`; see `../CONTEXT.md`.

---

## 2. File Map

```text
late-ssh/src/app/chat/cyberspace/
├── mod.rs       # declarations only
├── api.rs       # CsApi: typed reqwest client, envelope parsing, CsApiError, the cIRC SSE frame parser
├── svc.rs       # CyberspaceService: tasks, CsEvent broadcast, id-token cache, CircRoomSession
├── state.rs     # per-session State: views, modals, poll gating, notification grouping, pinned rooms, the open room, event drain
├── input.rs     # pane/room byte+arrow routing, the room composer, modal keystrokes
└── ui.rs        # pane views, the room surface, the four modals, the unlinked funnel
```

Cross-crate/cross-module touchpoints:
- `late-core/migrations/133_create_cyberspace_accounts.sql` (+ 136, 137, 139), `late-core/src/models/cyberspace_account.rs`: the one table, `ON DELETE CASCADE` to `users`, upsert replaces on re-link and keeps the cursors and pins.
- `late-ssh/src/main.rs`: constructs `CyberspaceService::new(db, api::BASE_URL)` once (the base URL is a const, not config) and attaches the `ActivityPublisher` via `with_activity`.
- `late-ssh/src/state.rs`, `session_bootstrap.rs`, `app/state.rs`: thread the service through root `State` → `SessionConfig` → `ChatState`, which owns the pane `State`.
- `chat/state.rs` / `chat/input.rs`: `cyberspace_selected`, `cyberspace_room_selected`, `clear_synthetic_selection` (which is where leaving a room happens), `/cs` command dispatch, and routing arrows/bytes into `cyberspace::input` for the pane, the room, and the room composer.
- `chat/ui.rs`: the `cyberspace` rail section (`RoomSlot::Cyberspace` + `RoomSlot::CyberspaceRoom`) in both rail builders, and pane/room render dispatch.
- `app/render.rs`: modal draw arm + `modal_active()` in the input-capture gates.
- `chat/commands.rs`: `/cs` and `/cyberspace` autocomplete entries.
- `app/activity/event.rs` / `publisher.rs`: `ActivityKind::CyberspacePosted` and `cyberspace_posted_task`.
- `chat/news/svc.rs`: `is_ai_blocklisted_url` (the AI wall, see section 3).

Keep `mod.rs` declaration-only.

---

## 3. The API Terms Are Load-Bearing

Their API terms ban bots, scraping/caching for redistribution, and feeding their content to AI systems. Every design decision below follows from that, and changes must not erode it:

1. **Every call runs under the linked user's own bearer token.** There is no global poller over their API. Two things recur, both tied to a human being present: a chat room's live stream and presence heartbeat, which exist only while the user is inside that room (section 9), and the per-session badge refresh (`refresh_unread`, 10-minute interval): the notification counter, the newest `UNREAD_PROBE_LIMIT` (10) entries for the unread count, and, only while chat rooms are pinned, the one roster call whose `last_message_at` stamps feed the rail's room dots (section 9). It dies with the session. A live client refreshing its own signed-in user's feed on a timer is what a client does; what the terms are about is fetching without a human behind it, which is why the interval is per session and the probe page is kept at badge size rather than a full page.
2. **Nothing fetched is cached server-side or shown to another user.** The service holds no content; everything lives in the fetching session's UI state and renders only for that user. This is why there is no shared snapshot: `CsEvent` broadcasts carry their data and sessions filter on `user_id`.
3. **No AI touches their content.** `news/svc.rs::is_ai_blocklisted_url` hard-stops cyberspace.online URLs (host and subdomains) before the News summarizer ever sees them, with an explanatory error. The `CyberspacePosted` activity line names our user's own action and title, never their content.
4. **Entering the pane is rate-limited** (`FEED_RELOAD_INTERVAL`, 30s): cycling the room rail lands on the slot, and every landing would otherwise be an authenticated call to a third party, which is exactly the traffic shape their anti-bot terms are about. `r` is the user explicitly asking and bypasses the interval.

---

## 4. Linking and the Token Model

- `/cs link` opens the login modal → `POST /v1/auth/login` → `GET /v1/users/me` for identity → upsert `cyberspace_accounts` with **only the refresh token**. The password is used once and never stored; a re-link replaces the row.
- id tokens (Firebase, ~60 min lifetime) are cached in-memory per user for `TOKEN_CACHE_TTL` (50 min) and re-minted via `POST /v1/auth/refresh`. Caching a token sweeps expired entries, so live third-party bearer tokens do not accumulate for the life of the process. Concurrent callers on a cold or expired cache serialize on a per-user refresh guard (opening a room fires three token-hungry tasks at once), so one refresh serves them all instead of three POSTs carrying the same stored refresh token.
- `TokenError` is the closed set of "no usable token" outcomes: `NotLinked` (renders as the login funnel), `Broken` (the stored refresh token was rejected: password change or revocation; the user is told to `/cs link` again), `Transport`.
- Errors never carry credentials: `transport()` uses `reqwest::Error::without_url`, and reqwest errors never embed request bodies or headers.
- `/cs unlink` deletes the row and drops the cached token. The `Unlinked` event clears all pane content.

## 5. Service and Event Model

`CyberspaceService` is orchestration only: every public entry is a `*_task` that spawns, does the API/DB work under a span, and publishes a `CsEvent`. One closed event enum, one `apply_event` match in `state.rs` with an arm per variant; failures the user should see funnel through `ActionFailed` and land either in the open modal's error line (if one is busy) or as a banner.

- Session init answers `LinkStatus` (link state **and** the stored feed read cursor, so the cursor lands before any entries do) and, for linked users, fetches the unread badge; later refreshes ride the session tick (`State::poll_unread_if_due`, `UNREAD_POLL_INTERVAL` 10 min) or pane actions.
- `RecentEntries` carries the probed posts, not a count: the read cursor never leaves the session that owns it, so the service stays ignorant of anyone's reading position and there is no svc → state dependency.
- The API envelope is always `{ "data": ... }` or `{ "error": { code, message } }`; the error branch wins whenever present. Write-only endpoints go through `parse_void`, which treats a bodyless 2xx as success: routing them through the data parser reported landed replies as failures, and the user sent them twice.
- Page limits are consts in `api.rs`: feed 30, unread probe 10, replies 50, notifications 20; request timeout 15s; user agent names us as a personal client.

## 6. Pane State, Views, and Modals

Three views (`View::Feed`/`Thread`/`Notifications`) and four modals (`Modal::Link`/`Compose`/`Reply`/`Rooms`, boxed because the first three carry their own `TextArea`s). `Rooms` is the odd one: a list that moves and toggles rather than a form, so `handle_modal_input` resolves it before the text-field arms and it is never "busy".

- Keys (linked): `j/k` move, `g` top of the current view, Enter opens the selected thread (or the notification's entry), `r` refreshes the feed / opens reply in a thread / reloads notifications, `p` compose, `n` notifications, `b` (or Esc via the shell's escape chain, `escape_to_feed`) back to the feed. Unlinked, the pane is the pitch funnel: Enter opens the link modal, everything else falls through so global keys keep working.
- **Entering the pane always lands on the newest entry** (`opened()` clears the thread and zeroes both selections). Keeping a selection across visits points it into a feed that has been refetched since, so it lands on whichever entry now occupies that row.
- **The rail badge is notifications + new entries, one sum.** `unread_count()` adds `unread_notifications` (their counter endpoint) to `unread_entries` (counted locally, below). The pane header is where the sum is split back into its halves, because they open with different keys: `@user on cyberspace.online · 12 new` on the left, `● N unread notifications · n to open` on the right.
- Opening notifications marks all read server-side (opening the view is reading them, same contract as the RSS inbox) and zeroes that half of the badge.
- **The notifications view is one row per event, not one per notification** (`dedupe_notifications`, keyed on kind + actor + `target_id` + `reply_id()`, first occurrence wins so the row keeps the newest stamp). Their API notifies more than once for a single event: one reply observed as three `reply` notifications with distinct ids. `metadata.replyId` is what makes the fold safe, since it separates "the same reply notified again" from "a second real reply on the same entry"; kinds that carry no reply id fall back to the entry. Deliberately **no `×N` count** on a row: a count would have to be trusted to mean something, and duplicates are not repeats.
- A notification's `targetId` is the **post** id for both `post` and `reply` targets (a reply notification puts the reply's own id in `metadata.replyId`); the entry is fetched via `GET /v1/posts/{id}` rather than looked up locally, since it is usually older than the feed page in memory. Ids only: that route 404s on slugs. Follows and pokes target a user, so they open nothing.
- `State::thread_target` holds the post the thread view is for, set before the post exists; a `ThreadLoaded` for anything else is stale and dropped, which is what stops a slow fetch from yanking a thread the user already left.
### 6a. The Feed Read Cursor

`cyberspace_accounts.feed_read_at` (migration 136) is one timestamp per user, the same shape both in-repo precedents converged on (`rss_feed_reads.last_read_at`; migration 023 threw away `article_feed_reads`' post-id half). Unread entries are the ones published after it.

- **It costs no recurring DB traffic.** It is read once per session, inside the `find_by_user_id` that `session_init_task` already runs, and written only when a human reads. The 10-minute clock touches their API, never ours.
- **It advances only when a feed the user asked for arrives.** Entering the pane and `r` set `mark_read_on_load`, and the `FeedLoaded` that answers them stamps the cursor at the newest entry on that page, never at the wall clock: a "now" stamp swallowed entries the 30s reload interval kept off the page (re-entering inside the interval shows the stale page) and entries a failed load never fetched. A `FeedLoaded` on its own marks nothing, because publishing an entry from another room also loads the feed, and that is not reading it. The cursor never moves backwards.
- `feed_marker_at` is the cursor frozen at the start of the visit, and it is what the `●` row marks compare against. Without it, entering the pane would wipe the marks off the very entries the user came in to read.
- **A `None` cursor counts as zero unread, not a full page.** A user opening the pane for the first time to "10 new" would be told they missed entries that were never theirs to miss.
- An entry with no `created_at` is never new: a badge counting rows the user cannot then find is worse than one that misses them.
- The count saturates at the probe page (10). The badge is a nudge, not an inventory, and the alternative is pulling their whole feed on a timer.

- Compose caps: title 100 chars, topics line 80 chars (comma/whitespace separated, lowercased, deduped, `#` stripped, max 3), body 32,768 chars of markdown. Validation happens at submit in `state.rs` (the boundary); Enter in a metadata field walks down to the body, only the body's submit publishes.
- **Modals stay open and busy while a submit is in flight, so a failed publish, reply, or login never eats the draft.** Esc still closes; a busy modal ignores every other keystroke.
- A created entry publishes `ActivityKind::CyberspacePosted`: a #lounge story line naming the title, throttle-keyed on it so retries collapse but distinct entries both announce.

## 7. Invariants

1. **The terms contract in section 3.** Per-user token on human action, no server-side content cache, no cross-user rendering, no AI on their content. Treat an erosion of any of these as a correctness bug.
2. **Only the refresh token and our own read cursor are persisted.** Never the password, never id tokens, never any of their content. A timestamp of when this user looked is ours, not theirs, which is what keeps the cursor clear of the caching their terms forbid. Error strings never carry credentials.
3. **The rail entry is gated on `cyberspace_linked` in both `visual_order_for_rooms` (navigation) and the rail builders (rendering).** Gating one and not the other leaves a slot the user can arrow onto but never see. `/cs` and `/cs post` for an unlinked user open the link modal over the current room instead of switching to a pane the rail does not list; `State::is_unlinked` (known-unlinked, not `Unknown`) is what lets the shell drop a pane the rail stopped listing without firing on "not sure".
4. **No shared snapshot.** Events carry their data; sessions filter on `user_id`.
5. **Poll clocks stamp at request time, not response time** (`poll_unread_if_due`, `load_feed`), so a hung fetch cannot queue a fresh request every tick.
6. **Migrations 133, 136, 137, and 139 are history.** Any schema change ships as a new forward migration.
7. **A chat room fetches only while its `CircRoomSession` is alive.** Every exit path drops it (see section 9); a stream, heartbeat, or poll that outlives the user's presence in the room is a terms bug, not a leak.
8. **`mod.rs` stays declaration-only.**

---

## 8. Known Gaps / Backlog

- **Nothing invalidates a cached id token on an `UNAUTHORIZED` response**, so a token revoked on their side mid-TTL (password change, session revoke) fails every pane action until the 50 minutes are up, even though a re-mint would recover it. Fixing it means dropping the cache entry and retrying once at the call sites in `svc.rs`.
- No C-Mail or guild surfaces; cIRC's own gaps are listed at the end of section 9.
- The unread-entry count saturates at the probe page of 10, so a user back from a long absence sees `10` rather than the true number. Raising it is one const, at the cost of a bigger recurring fetch.
- `me()` parses the profile leniently (`userId`/`uid`/`id`) because their docs pin the endpoint but not the field names.
- The thread view pre-wraps its text (`thread_lines` → `wrap_paragraph`, budgeted by display column via `unicode-width`, not char count) instead of handing `Wrap` to the paragraph, so one `Line` is one rendered row. The renderer writes the resulting ceiling into `State::thread_max_scroll` (a `Cell`, same pattern as the composer viewport slot) and `move_selection` clamps against it. Counting unwrapped lines instead put the ceiling at zero for the normal shape of a markdown entry (a few long paragraphs), which made the replies unreachable; wrapping by char count truncated CJK/emoji rows at the pane edge.

## 9. cIRC: Their Chat As A Rail Section

Their API docs are public at https://api.cyberspace.online/docs (markdown at `/docs.md`; their WAF 403s non-browser user agents, so fetch with a browser UA).

**Shape.** Linking cyberspace gives the rail its own collapsible `cyberspace` section (`RoomSection::Cyberspace`, shortcut `y`) holding **`feeds`** (`RoomSlot::Cyberspace`, the feed/thread/notifications pane, moved out of Core and relabelled, because the chat rooms beside it are cyberspace too) plus one row per added chat room. The section renders only while `cyberspace_linked`, so an unlinked user's rail is untouched.

**Rooms are added through a picker, and adding one is what creates its rail entry.** There is no join/leave over there: `GET /v1/circ` returns the rooms this account may read and that roster is what it is, so our list is a bookmark. `/cs chat` (alias `/cs rooms`) opens `Modal::Rooms`, a checklist of their roster with online counts; Enter adds the highlighted room to the section or takes it off. Command only, deliberately: a single letter would have to be pressed on the feeds pane specifically, which is the one place a user looking for rooms is least likely to be standing, and the pane's letters are otherwise all reading actions. The picker deliberately does **not** open rooms: a room is entered by selecting its rail entry, like every other room in the rail.

**The conversation is pre-wrapped, same contract as the thread view.** `room_lines(messages, width)` returns one `Line` per rendered row, wrapping each message's text under a hanging indent so the author is named once. Handing `Wrap` to the paragraph instead makes the scroll window lie: it counts rows, and a wrapped message occupies more rows than the one it is counted as, which pushes the newest messages off the bottom. The renderer writes the row ceiling into `OpenRoom::max_scroll` (a `Cell`, like `thread_max_scroll`) and `room_scroll` clamps against it, because only the renderer knows the width the text wrapped to.

**A room writes through the chat composer slot, not its own box.** `draw_room` renders only the header and the conversation; the composer `TextArea` lives on `OpenRoom` but is drawn by `chat::ui` into the footer block it shares with the pane's hint line (titled with the room's name while one is open). Drawing a second bordered input inside the pane stacked two boxes over each other and made a room look unlike every other room. It is single-line for the same reason: that slot is one row, and their API takes one message per send.

**Persistence is ours and tiny.** `cyberspace_accounts.circ_rooms` (migration 137) is the ordered pinned slugs, replaced wholesale on every change, and `circ_room_reads` (migration 139) is one jsonb map beside it: slug → the newest message timestamp seen while the user was inside, on their clock, pruned to the pinned list whenever that list is rewritten. Nothing else lands in our DB: names and online counts come from their roster on demand, and their own read state stays on their side (`POST /v1/circ/:roomId/read`, written when a room opens; it is write-only, which is why the dot cursor is ours). `LinkStatus` carries the list and the cursors at session init, beside the feed cursor.

**Fetch only where the user is.** `CircRoomSession` (svc.rs) is the whole contract: holding one is what makes a room fetch anything, and `Drop` aborts its three tasks and announces the user out of the room. Entering loads history (`GET /v1/circ/:roomId?limit=50`), opens one SSE stream on their realtime database (`<rtdbUrl>/chat_messages/<roomId>.json?auth=<idToken>`, always `orderBy="timestamp"` and `limitToLast` ≤ 100; frames are reassembled from raw bytes by `CircStreamBuffer` and decoded once per complete frame, so a chunk boundary inside a multi-byte character cannot mangle text), and heartbeats presence at the `heartbeatMs` their own response names, never a hard-coded one, floored at `CIRC_PRESENCE_MIN_HEARTBEAT_MS` when parsed so a misbehaving response cannot dictate a hot loop. Every path out of a room drops the session: `ChatState::clear_synthetic_selection` calls `leave_room` (selecting anything else in the rail is leaving), `App::set_screen` calls `ChatState::close_cyberspace_room` (leaving Home entirely is leaving too, and Esc is not the only way off the screen), and the chat tick leaves a room the pinned list can no longer name. A background stream would be a fetch with no human behind it, which is the whole of section 3. The stream reopens when the ~60 min id token expires and gives up after `CIRC_STREAM_MAX_FAILURES`, publishing `CircStreamEnded` rather than reconnecting in a loop (their docs ask for exactly that). `rtdbUrl` rides the login/refresh response into the token cache.

**The `feeds` badge reads as a floor once the probe page fills.** Unread entries are counted out of a page of `UNREAD_PROBE_LIMIT` (10), so ten means "at least ten"; `room_slot_badge` renders `9+` in that case rather than a total it cannot stand behind (rooms keep their own `99+` SQL cap).

**A chat room's unread state is a dot, never a count.** Their roster reports `last_message_at` per room but never reads back read state, and a number (even a saturated one) would take a per-room history fetch every poll: authenticated calls with nobody behind them, growing with every pin. The dot costs exactly one roster call riding the existing 10-minute badge refresh, fetched only while rooms are pinned, and compares each room's `last_message_at` against our own read cursor (`circ_room_reads`, above; same clock on both sides, so skew never enters into it). **Entering a room always clears its dot**, before a single message has loaded: `stamp_open_room_read` runs on entering, on history landing, on a roster arriving while the room is open, and on leaving, and it takes the roster's own `last_message_at` as a floor under the newest message on screen. Stamping from the messages alone was the original bug: a room whose history call failed, came back empty, or carried nothing stampable kept its dot forever, and only writing a message (which arrives back through the stream) could clear it. Reading the roster's own value also keeps the comparison like for like, so the two halves of the dot can never drift apart. The cursor only ever moves forward and is persisted fire-and-forget. The open room never shows a dot, because a room is read by being in it, the way an IRC channel is; a room never visited shows nothing rather than claiming a backlog the user was never behind on. Mentions still ride the existing notification badge.

**Navigation lands in five mirrors**, and invariant 3 covers all of them: `visual_order_for_rooms`, `build_cozy_room_rail_rows` and `build_room_list_rows` (the two rail builders, whose `hit_slots` are the click mirror), `RoomSection` (label/shortcut/`from_label`), and the `Ctrl+/` jump modal (`room_search_modal/state.rs`). `Space` room-jump comes free once the slots are in `visual_order`.

**The slot is dynamic, unlike every synthetic entry before it.** `SelectedRoomSlotState` is `Copy` and a slug is not, so `RoomSlot::CyberspaceRoom(usize)` carries the index into the pinned list and `ChatState::cyberspace_room_selected` is the same index. The pinned list can change under that index (`toggle_selected_room` in this session, a `CircPinned` broadcast from another session of the same account, an unlink), so the chat tick re-derives it from the open room's slug every frame: the cursor follows the room through a reorder, and a room the rail can no longer name is left, landing the user back on the pane. The bool-per-entry selection now clears through `clear_synthetic_selection`, so a new entry cannot half-clear the others.

**Rendering, from their docs** (`CircMessage::display_text`, tested in `api_test.rs`): `content` can be empty (an image, GIF or song is the whole message, and a website post sometimes repeats the attachment URL as `content`, which prints the link twice if taken literally); a delete arrives as a `patch` that rewrites a message already on screen, so it is applied in place and never appended; `style: "art"` means `content` is base64 ASCII art; `isAction` renders `* username content`. `/me` and friends expand server-side, so they are sent as plain text. Messages cap at 2,048 chars; sending is 15/min, 150/hour, 300/day.

**Not built:** C-Mail (identical mechanism, would follow), guild threads, deleting your own messages (`DELETE /v1/circ/:roomId/messages/:id`, we only render the tombstones), scrollback paging (`before` is plumbed through `read_circ_room` but nothing calls it yet), the live presence stream (`chat_presence/<roomId>`), and any generic "integrations" abstraction. There is one integration; the section is named for it and its code stays in this slice.

## 10. Testing Guidance

Run via `ARGS="-p late-ssh -E 'test(cyberspace)'" make test-llm`.

- `api_test.rs`: envelope parsing (data/error/neither), `parse_void` on bodyless 2xx, error mapping, notification `post_id()` target shapes.
- `state_test.rs`: topic parsing, `feed_reload_due`/`unread_poll_due` gating, modal validation, stale-thread drop, notification dedupe, the reset on re-entering the pane, the unread-entry count and the badge sum, and the marks surviving the visit that clears the count.
- `ui_test.rs`: `thread_lines` height for a long entry (the scroll ceiling that pins the wrapping fix) and rows staying inside the pane for wide (CJK) glyphs.
- `svc_test.rs`: DB-backed link status/unlink against a dead base URL so nothing touches the network, plus the shared-refresh dedup against a counting fake HTTP server (the one exception to "dead base URL", and still never their real API).
- cIRC: `api_test.rs` covers the style shapes, `display_text` (art decode, duplicated caption, tombstone), the SSE frames (window/upsert/patch/removal/keep-alive), `CircStreamBuffer` chunk reassembly (the multi-byte split), and the presence floor; `state_test.rs` covers the history+window merge, deletion applied in place, frames for another room being ignored, pin toggling, unlink closing the room, and the unread-dot lifecycle (`room_dots_follow_the_roster_against_the_read_cursor`); `late-core` covers the cursor round-trip and prune (`circ_room_read_cursors_round_trip_and_prune_with_the_pins`); `chat/state_internal_test.rs::the_cyberspace_section_carries_the_pane_and_the_pinned_rooms` pins the rail section against navigation, and `remote_pin_changes_re_derive_the_rail_room_selection` pins the tick reconcile; `app/input_flow_test.rs::switching_screens_drops_the_open_cyberspace_room` pins the screen-switch exit path.
- `late-core/src/models/cyberspace_account_test.rs`: upsert/replace/delete, owner scoping, the read cursor round-tripping and surviving a re-link (use microsecond-precision stamps: `timestamptz` truncates a nanosecond `Utc::now()`).
- `app/input_flow_test.rs`: the unlinked funnel vs the linked rail entry + pane.
- `chat/news/svc_internal_test.rs`: the AI blocklist host matching.

Never write a test that calls the real cyberspace.online API.

## 11. References

- cyberspace.online API docs: https://api.cyberspace.online/docs (markdown: `/docs.md`; needs a browser user agent)
- Chat context (commands, rail, keys table): `../CONTEXT.md`
- Root context: `../../../../../CONTEXT.md`
- RSS/News read-contract precedent: `../news/svc.rs`, `late-core/src/models/rss_feed.rs`
