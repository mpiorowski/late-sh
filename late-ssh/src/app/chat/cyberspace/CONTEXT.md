# late.sh Cyberspace Context

## Metadata
- Domain: late.sh as a personal client for cyberspace.online: the Cyberspace rail entry/pane, `/cs` commands, account linking, and the typed v1 API client
- Primary audience: LLM agents working in `late-ssh/src/app/chat/cyberspace`, the `/cs` commands, the `cyberspace_accounts` table, or the AI blocklist for cyberspace.online URLs
- Last updated: 2026-08-07
- Status: Active (v1)
- Parent context: `../CONTEXT.md` (chat), root `../../../../../CONTEXT.md`
- Related context: `../news/` (`is_ai_blocklisted_url` lives in `news/svc.rs`)

---

## 1. Scope

Owned by this domain:
- The typed reqwest client for the cyberspace.online v1 API (`api.rs`): login/refresh, feed, threads, replies, posting, notifications, unread count, all through the `{data}/{error}` envelope.
- `CyberspaceService` (`svc.rs`): fire-and-forget tasks, the `CsEvent` broadcast, and the in-memory per-user id-token cache.
- The `cyberspace_accounts` row model (`late-core/src/models/cyberspace_account.rs`, migration 133 + 136): one row per user, storing the Firebase refresh token (never the password) and the feed read cursor.
- Per-session pane state (`state.rs`): feed/thread/notifications views, the link/compose/reply modals, the unread badge and its poll gating.
- Pane input (`input.rs`) and rendering (`ui.rs`), including the unlinked pitch + login funnel.

Out of scope (deliberate boundaries):
- Nothing here now: the unread-entry count that was the v2 deferral shipped, see section 6a.
- **v3 idea, not investigated: their chat, cIRC.** Their IRC-flavored chat surface (cIRC is their name for it; their API docs are behind auth, so the endpoints for reading or sending are unknown to this repo). What we do know is transcribed from their notification docs: `describe_notification` handles `chat_mention`, `dm_message` ("c-mail"), and `guild_new_thread`, so chat, DMs, and guilds all exist over there. The blocker is not plumbing, it is the terms: fetched content renders only for the user who fetched it, so a bridged channel cannot live in a shared late.sh room where other members would read one linked user's content. A per-user private surface (like this pane, another view inside it) is the shape that fits. Read their cIRC endpoints with a linked account before designing anything.
- The `/cs` (alias `/cyberspace`) commands themselves are parsed and dispatched from `chat/state.rs` (`parse_cyberspace_command`, handled inline on `ChatState`), and the rail entry is built in `chat/ui.rs`; see `../CONTEXT.md`.

---

## 2. File Map

```text
late-ssh/src/app/chat/cyberspace/
├── mod.rs       # declarations only
├── api.rs       # CsApi: typed reqwest client, envelope parsing, CsApiError
├── svc.rs       # CyberspaceService: tasks, CsEvent broadcast, id-token cache
├── state.rs     # per-session State: views, modals, poll gating, notification grouping, event drain
├── input.rs     # pane byte/arrow routing + modal keystroke handling
└── ui.rs        # pane views, the three modals, the unlinked funnel
```

Cross-crate/cross-module touchpoints:
- `late-core/migrations/133_create_cyberspace_accounts.sql`, `late-core/src/models/cyberspace_account.rs`: the one table, `ON DELETE CASCADE` to `users`, upsert replaces on re-link.
- `late-ssh/src/main.rs`: constructs `CyberspaceService::new(db, api::BASE_URL)` once (the base URL is a const, not config) and attaches the `ActivityPublisher` via `with_activity`.
- `late-ssh/src/state.rs`, `session_bootstrap.rs`, `app/state.rs`: thread the service through root `State` → `SessionConfig` → `ChatState`, which owns the pane `State`.
- `chat/state.rs` / `chat/input.rs`: `cyberspace_selected`, `/cs` command dispatch, routing arrows/bytes into `cyberspace::input` when the pane is selected.
- `chat/ui.rs`: the synthetic rail entry (`RoomSlot::Cyberspace`, Core section below rss) and pane render dispatch.
- `app/render.rs`: modal draw arm + `modal_active()` in the input-capture gates.
- `chat/commands.rs`: `/cs` and `/cyberspace` autocomplete entries.
- `app/activity/event.rs` / `publisher.rs`: `ActivityKind::CyberspacePosted` and `cyberspace_posted_task`.
- `chat/news/svc.rs`: `is_ai_blocklisted_url` (the AI wall, see section 3).

Keep `mod.rs` declaration-only.

---

## 3. The API Terms Are Load-Bearing

Their API terms ban bots, scraping/caching for redistribution, and feeding their content to AI systems. Every design decision below follows from that, and changes must not erode it:

1. **Every call runs under the linked user's own bearer token.** There is no global poller over their API. The only recurring fetch is the per-session badge refresh (`refresh_unread`, 10-minute interval): the notification counter, plus the newest `UNREAD_PROBE_LIMIT` (10) entries for the unread count. It dies with the session. A live client refreshing its own signed-in user's feed on a timer is what a client does; what the terms are about is fetching without a human behind it, which is why the interval is per session and the probe page is kept at badge size rather than a full page.
2. **Nothing fetched is cached server-side or shown to another user.** The service holds no content; everything lives in the fetching session's UI state and renders only for that user. This is why there is no shared snapshot: `CsEvent` broadcasts carry their data and sessions filter on `user_id`.
3. **No AI touches their content.** `news/svc.rs::is_ai_blocklisted_url` hard-stops cyberspace.online URLs (host and subdomains) before the News summarizer ever sees them, with an explanatory error. The `CyberspacePosted` activity line names our user's own action and title, never their content.
4. **Entering the pane is rate-limited** (`FEED_RELOAD_INTERVAL`, 30s): cycling the room rail lands on the slot, and every landing would otherwise be an authenticated call to a third party, which is exactly the traffic shape their anti-bot terms are about. `r` is the user explicitly asking and bypasses the interval.

---

## 4. Linking and the Token Model

- `/cs link` opens the login modal → `POST /v1/auth/login` → `GET /v1/users/me` for identity → upsert `cyberspace_accounts` with **only the refresh token**. The password is used once and never stored; a re-link replaces the row.
- id tokens (Firebase, ~60 min lifetime) are cached in-memory per user for `TOKEN_CACHE_TTL` (50 min) and re-minted via `POST /v1/auth/refresh`. Caching a token sweeps expired entries, so live third-party bearer tokens do not accumulate for the life of the process.
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

Three views (`View::Feed`/`Thread`/`Notifications`), three modals (`Modal::Link`/`Compose`/`Reply`, boxed because each carries its own `TextArea`s).

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
6. **Migrations 133 and 136 are history.** Any schema change ships as a new forward migration.
7. **`mod.rs` stays declaration-only.**

---

## 8. Known Gaps / Backlog

- **Nothing invalidates a cached id token on an `UNAUTHORIZED` response**, so a token revoked on their side mid-TTL (password change, session revoke) fails every pane action until the 50 minutes are up, even though a re-mint would recover it. Fixing it means dropping the cache entry and retrying once at the call sites in `svc.rs`.
- No chat/DM/guild surfaces (v3, section 1).
- The unread-entry count saturates at the probe page of 10, so a user back from a long absence sees `10` rather than the true number. Raising it is one const, at the cost of a bigger recurring fetch.
- `me()` parses the profile leniently (`userId`/`uid`/`id`) because their docs pin the endpoint but not the field names.
- The thread view pre-wraps its text (`thread_lines` → `wrap_paragraph`, budgeted by display column via `unicode-width`, not char count) instead of handing `Wrap` to the paragraph, so one `Line` is one rendered row. The renderer writes the resulting ceiling into `State::thread_max_scroll` (a `Cell`, same pattern as the composer viewport slot) and `move_selection` clamps against it. Counting unwrapped lines instead put the ceiling at zero for the normal shape of a markdown entry (a few long paragraphs), which made the replies unreachable; wrapping by char count truncated CJK/emoji rows at the pane edge.

## 9. Testing Guidance

Run via `ARGS="-p late-ssh -E 'test(cyberspace)'" make test-llm`.

- `api_test.rs`: envelope parsing (data/error/neither), `parse_void` on bodyless 2xx, error mapping, notification `post_id()` target shapes.
- `state_test.rs`: topic parsing, `feed_reload_due`/`unread_poll_due` gating, modal validation, stale-thread drop, notification dedupe, the reset on re-entering the pane, the unread-entry count and the badge sum, and the marks surviving the visit that clears the count.
- `ui_test.rs`: `thread_lines` height for a long entry (the scroll ceiling that pins the wrapping fix) and rows staying inside the pane for wide (CJK) glyphs.
- `svc_test.rs`: DB-backed link status/unlink against a dead base URL so nothing touches the network.
- `late-core/src/models/cyberspace_account_test.rs`: upsert/replace/delete, owner scoping, the read cursor round-tripping and surviving a re-link (use microsecond-precision stamps: `timestamptz` truncates a nanosecond `Utc::now()`).
- `app/input_flow_test.rs`: the unlinked funnel vs the linked rail entry + pane.
- `chat/news/svc_internal_test.rs`: the AI blocklist host matching.

Never write a test that calls the real cyberspace.online API.

## 10. References

- Chat context (commands, rail, keys table): `../CONTEXT.md`
- Root context: `../../../../../CONTEXT.md`
- RSS/News read-contract precedent: `../news/svc.rs`, `late-core/src/models/rss_feed.rs`
