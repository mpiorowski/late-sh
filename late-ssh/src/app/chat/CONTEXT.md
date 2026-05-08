# late-ssh Chat Context

## Metadata
- Domain: late.sh SSH chat, synthetic chat feeds, and dashboard/room chat surfaces
- Primary audience: LLM agents working in `late-ssh/src/app/chat`
- Last updated: 2026-05-05
- Status: Active
- Parent context: `../../../../CONTEXT.md`

---

## 1. Scope

This file owns chat-specific context that used to make the root `CONTEXT.md` too large.

Included here:
- Main chat rooms, DMs, public/private topic rooms, and game-backed room chat.
- Dashboard chat and embedded Rooms chat surfaces.
- Message composer, replies, edits, deletes, reactions, pinned messages, ignores, overlays, and autocomplete.
- Synthetic chat entries: Feeds, News, Mentions/Notifications, Showcase, Work, and Discover.
- Chat service refresh/tail/event contracts, DB model constraints, keybindings, tests, and gotchas.

Global SSH, audio, games, profile, rooms/blackjack, observability, and repo-wide test policy stay in the root context.

---

## 2. File Map

```text
late-ssh/src/app/chat/
|-- mod.rs                       # Module declarations only
|-- svc.rs                       # ChatService: DB boundary, snapshots, events, room/message tasks
|-- state.rs                     # ChatState: local UI state, receivers, composer, room/message selection
|-- input.rs                     # Chat-page input plus shared message actions used by Dashboard/Rooms
|-- ui.rs                        # Full chat, dashboard chat, embedded room chat, room list, composer, row cache
|-- ui_text.rs                   # Message/news/reaction wrapping into ratatui Lines
|-- discover/                    # Synthetic Discover entry: public rooms not yet joined
|-- feeds/                       # Synthetic Feeds entry: private per-user RSS/Atom inbox
|-- news/                        # Synthetic News entry: articles + #general announcement
|-- notifications/               # Synthetic Mentions entry: mention notifications
|-- showcase/                    # Synthetic Showcase entry: user project links
`-- work/                        # Synthetic Work entry: one public work profile per user
```

Related tests:

```text
late-ssh/tests/chat/
|-- main.rs
|-- svc.rs                       # Broad ChatService integration coverage
|-- news.rs                      # ArticleService integration coverage
|-- showcase.rs                  # ShowcaseService integration coverage
|-- work.rs                      # WorkService integration coverage
`-- state.rs                     # Placeholder; direct ChatState integration tests need more accessors
```

Core models used by chat live in `late-core/src/models/`:
`chat_room.rs`, `chat_room_member.rs`, `chat_message.rs`, `chat_message_reaction.rs`,
`notification.rs`, `rss_feed.rs`, `rss_entry.rs`, `article.rs`, `article_feed_read.rs`, `showcase.rs`,
`showcase_feed_read.rs`, `work_profile.rs`, and `work_feed_read.rs`.
Chat-owned moderation commands also use `room_ban.rs`,
`server_ban.rs`, `artboard_ban.rs`, and `moderation_audit_log.rs`.

---

## 3. Ownership Split

- `svc.rs` is the async boundary between TUI state, DB models, mention notifications, and broadcast/watch channels.
- `state.rs` owns local chat data, room/message selection, composer state, reply/edit/reaction/pin state, overlays, synthetic-entry substates, unread/read tracking, and cache inputs.
- `input.rs` maps chat keys to state/service actions. `handle_message_action_in_room` is shared by the full Chat screen, Dashboard chat, and embedded Rooms chat.
- `ui.rs` renders chat surfaces and owns `ChatRowsCache`.
- `ui_text.rs` centralizes wrapping for normal messages, the small Markdown subset, reply quotes, `---NEWS---` cards, and reaction footers.

Keep `mod.rs` declaration-only; no `pub use` re-export layer.

---

## 4. Service And Data Flow

`ChatService` channels:
- Per-session `watch<ChatSnapshot>` for low-frequency room summary data.
- `broadcast<ChatEvent>` for live message, reaction, room-command, tail, and error events.
- Shared `watch<Arc<Vec<String>>>` username directory for mention autocomplete, refreshed every 30s.
- A service-owned refresh scheduler that refreshes registered sessions every 10s and on explicit signals.
- `read_permits: Semaphore(8)` to cap concurrent snapshot, tail, discover, and pinned-message reads.

Important constants in `svc.rs`:
- `HISTORY_LIMIT = 500`
- `DELTA_LIMIT = 256`
- `PINNED_MESSAGES_LIMIT = 100`
- `CHAT_REFRESH_INTERVAL = 10s`
- `USERNAME_DIRECTORY_TTL = 30s`

Normal display flow:
1. `ChatState::new` subscribes to chat events/usernames and calls `ChatService::start_user_refresh_task`.
2. The per-user snapshot loads joined rooms, unread counts, `#general` id, DM/current-user metadata, bonsai glyphs for those users, and ignored user ids.
3. Snapshots intentionally carry empty message vectors. They do not load history.
4. Visible-room changes call `App::sync_visible_chat_room()`, which stores `visible_room_id`, marks the room read, and requests a room tail.
5. `load_room_tail_task` fetches the newest 500 messages, reaction summaries, author usernames, and author bonsai glyphs for the visible room.
6. Broadcast `MessageCreated`/`MessageEdited`/`MessageDeleted`/reaction events patch local state. Broadcast lag triggers a tail reload for the visible room.

`ChatSnapshot` is summary data. `RoomTailLoaded` is history data. Do not merge those responsibilities back together.

---

## 5. DB Contracts

Room model:
- `chat_rooms.kind`: `general`, `language`, `dm`, `topic`, `game`.
- `chat_rooms.visibility`: `public`, `private`, `dm`.
- `general` must have slug `general`, is public, auto-join, and permanent.
- `language` rooms are public, opt-in, unique by `language_code`, with slug `lang-{code}`.
- `topic` rooms are unique by `(visibility, slug)`.
- `game` rooms are public, opt-in, require `game_kind + slug`, are unique by `(game_kind, slug)`, and DB constraints require `auto_join = false`.
- DMs canonicalize endpoint UUIDs by text order and are unique by `(dm_user_a, dm_user_b)`.

Membership:
- `chat_room_members` primary key is `(room_id, user_id)`.
- `last_read_at` drives unread counts.
- Unread counts exclude messages authored by the current user.
- `join` is idempotent and preserves original `joined_at` on conflict.
- Membership is the authorization check for reading tails, syncing deltas, marking read, sending, reacting, listing members, and inviting.

Messages:
- `chat_messages.body` must be trimmed non-empty and length <= 2000.
- Messages are hard-deleted. There are no tombstones.
- Recent/tail queries return newest-first: `ORDER BY created DESC, id DESC`.
- Delta queries return ascending after `(created, id)` and are inserted into newest-first local state.
- `reply_to_message_id` is nullable and uses `ON DELETE SET NULL`.
- `pinned` is a global message-level flag with a partial pinned index.

Reactions:
- `chat_message_reactions` primary key is `(message_id, user_id)`.
- Each user has at most one numeric reaction kind `1..=8` per message.
- Message/user deletion cascades remove reactions.

Notifications:
- Mentions are stored in `notifications`.
- Mention unread state is cursor-based through `mention_feed_reads`.
- Mention resolution excludes the actor; DMs only notify DM participants, private rooms only members, and non-game public rooms may mention any user. Game-room chat does not create Mentions feed notifications.

---

## 6. Rooms And Selection

`RoomSlot` represents either a real room or one of the synthetic entries: News, Notifications, Discover, Showcase, or Work.

Visual order is defined in `state.rs` and mirrored by room-list rendering in `ui.rs`:
1. Core permanent rooms: `general`, `announcements`, `suggestions`, `bugs`.
2. Other permanent rooms.
3. Feeds, when the current user has at least one RSS/Atom subscription.
4. News.
5. Showcase.
6. Work.
7. Notifications/Mentions.
8. Discover.
9. Public topic rooms, sorted by slug.
10. Private topic rooms, sorted by slug.
11. DMs, sorted by peer display name.

Feeds:
- Feed subscriptions are per-user and managed in `Settings -> Feeds`.
- `rss_feeds` stores connected RSS/Atom URLs; `rss_entries` stores private pending entries.
- The background `FeedService` polls active feeds, parses a conservative RSS/Atom subset, stores unseen entries, and publishes per-user events.
- The `feeds` synthetic room is private. Press `s` on an entry to share it through `ArticleService::process_url`; only then does it become a public News article and `#general` announcement.
- Enter copies the selected feed entry URL, `d` dismisses it, and `r` asks the feed poller to refresh.

Game rooms stay in `ChatState.rooms` for embedded Rooms chat, but `is_chat_list_room` hides them from the main Chat room list/navigation and favorite-room picker.

Room navigation:
- `h`/`l`, left/right arrows, `Ctrl+P`/`Ctrl+N` switch room selection.
- `Space` activates room-jump mode, assigning keys from `ROOM_JUMP_KEYS`. Jumping to the already selected room/synthetic entry still re-runs the entry's read/list side effects so stale unread badges clear.
- While composing on the Chat page, `Ctrl+N`/`Ctrl+P` switch real rooms while preserving draft text and dropping reply/edit state.
- Synthetic entries are selected with booleans (`news_selected`, `notifications_selected`, `discover_selected`, `showcase_selected`, `work_selected`), not `selected_room_id`.

---

## 7. Dashboard And Embedded Chat

Dashboard chat displays `App::dashboard_active_room_id()`:
- No favorites: `#general`.
- One favorite: that room if still joined, otherwise `#general`.
- Two or more favorites: `favorites[dashboard_favorite_index]`, clamped and falling back to `#general`.

Dashboard favorite controls:
- `[` / `]` cycle.
- `,` jumps to the previously active favorite.
- `g<digit>` jumps to slot 1..9.
- The favorite strip renders only when at least two resolvable favorites exist.

Dashboard box row:
- The three dashboard boxes are always the top dashboard body when the content width/height can fit them, even if the stream/vote header is hidden.
- `b` then `1` enters the left Blackjack room slot.
- `b` then `2` launches the currently displayed unfinished daily game.
- `b` then `3` opens the Chat screen with News selected and the currently displayed wire article selected.
- `b` then `4` opens the Chat screen with `#announcements` selected; its chip renders on pinned dashboard messages.
- The daily-game and wire-news boxes rotate every 60 seconds; the wire rotates through at most five articles.
- Dashboard pinned messages render with the dashboard box row: a detached top pin strip when roomy, otherwise embedded into the bottom grid rule. There is no separate pinned strip above dashboard chat.

`App::sync_visible_chat_room()` is the read/tail-load bridge. It computes the visible chat room from Dashboard, Chat, or Rooms screen, stores it in `ChatState`, marks it read, and requests a tail on change. Call it after screen, room, favorite, or active-room changes.

There are separate `ChatRowsCache` instances on `App` for:
- Dashboard chat.
- Full Chat screen active room.
- Rooms embedded chat.

Do not share a row cache across surfaces unless width and visible messages are guaranteed identical.

---

## 8. Composer, Commands, Reply, Edit

The main composer is a `ratatui_textarea::TextArea<'static>`.

`composer_room_id` is the authoritative send target while composing. This matters because Dashboard and Rooms do not necessarily drive `selected_room_id`.

Starting compose in a room:
- Clears message selection.
- Clears reply target.
- Clears edit target.
- Stores `composer_room_id`.

Submit flow in `ChatState::submit_composer`:
- Commands are handled before normal send.
- `/leave` and `/invite` are refused from Dashboard because they depend on Chat-page room selection.
- `/members` resolves the target before clearing the composer because clearing removes `composer_room_id`.
- Normal send calls `send_message_with_reply_task`.
- Edit calls `edit_message_task`.
- Enter submits and closes.
- `Alt+S` submits and keeps the composer open.
- `Alt+Enter` and `Ctrl+J` insert a newline in the main chat composer.

User commands:
- `/active` opens an overlay from in-memory `active_users`, including repeated-session counts.
- `/binds` opens the Chat help topic.
- `/dm @user` opens/creates a DM.
- `/exit` opens quit confirm.
- `/ignore [@user]` mutes a user or lists muted users.
- `/invite @user` adds a user to the selected non-DM room.
- `/leave` leaves the selected non-permanent room.
- `/list` lists public rooms.
- `/members` lists selected-room members.
- `/mod` opens the moderation command modal; `/mod ...` in chat is rejected because commands run only in the modal.
- `/music` opens music help.
- `/private #room` creates a private topic room and joins the caller.
- `/public #room` opens or creates an opt-in public room for the caller only (`auto_join=false`).
- `/settings` opens settings.
- `/unignore [@user]` removes an ignored user.

Admin commands:
- `/create-room #room` creates/promotes a permanent auto-join room and bulk-adds existing users.
- `/delete-room #room` deletes a permanent room.
- `/fill-room #room` bulk-adds all users to an existing public room and flips `auto_join=true`; private rooms cannot be filled.

Moderation modal commands:
- `help [command]`
- `user @name`
- `bans [server|artboard|room #slug] [limit]`
- `audit [limit]`
- `room kick #slug @name [reason...]`
- `room ban #slug @name [duration] [reason...]`
- `room unban #slug @name`
- `server kick @name [reason...]`
- `server ban @name [duration] [reason...]`
- `server unban @name`
- `artboard ban @name [duration] [reason...]`
- `artboard unban @name`
- `grant mod @name`
- `revoke mod @name`

Moderation list limits default to 25 and cap at 100. Durations use positive `s/m/h/d` suffixes.

Reply mode:
- Captures `ReplyTarget { message_id, author, preview }`.
- Enters compose mode and clears edit.
- On submit, stores `reply_to_message_id` and prefixes the stored body with a visible quote line for backward-compatible rendering.
- Enter on a selected reply jumps only if the target is already loaded in the current room tail.

Edit mode:
- Allowed for the message author or admins.
- Loads the message body into a fresh composer.
- Clears reply.
- Empty edits fail.

Autocomplete:
- `@` filters the shared username directory.
- `/` filters static non-admin chat commands.
- Arrow keys move selection.
- Tab/Enter confirms.
- Esc dismisses popup without leaving compose mode.
- Pressing `/` while not composing on Dashboard/Chat starts command compose for the active room.

---

## 9. Message Actions

Shared message actions live in `chat::input::handle_message_action_in_room`.

Keys:
- `j` / `k` and arrows move selected message.
- `Ctrl+D` / `Ctrl+U` move by an approximate half-page in message units.
- `r` replies.
- `e` edits.
- `d` deletes and moves selection to an adjacent message.
- `p` opens the selected author's read-only profile modal.
- `c` copies the selected message body.
- Enter jumps from a reply to its loaded target.
- `f` enters reaction leader mode.
- `f` again while reaction leader is active opens reaction-owner overlay.
- Digits `1..8` while reaction leader is active toggle reactions, exit reaction leader mode, and keep the message selected.
- `Ctrl+P` toggles selected-message pin state; admin only.

Selection deltas are message-based, not row-based. Positive means older, negative means newer.

---

## 10. Reactions, Pins, Ignores

Reactions:
- One reaction per `(message_id, user_id)`.
- Reaction kinds are `1..8`.
- UI appends reaction footer chips under the message body or news card.
- Reaction summaries live in `message_reactions: HashMap<Uuid, Vec<ChatMessageReactionSummary>>`.
- Reaction-owner overlay waits for a matching `ReactionOwnersListed` event keyed by `pending_reaction_owners_message_id`.

Pins:
- `chat_messages.pinned` is global, not scoped to a room or user.
- Only admins can toggle pins.
- Toggling pin does not optimistically update local pinned dashboard state.
- Dashboard pinned stack comes from `load_pinned_messages_task` through a separate watch channel, not from the 10s summary snapshot.

Ignores:
- `users.settings.ignored_user_ids` stores UUIDs, not usernames.
- `/ignore @user` and `/unignore @user` resolve usernames at command time.
- Ignore filtering applies to non-DM rooms only.
- DMs intentionally bypass ignored-user filtering; leaving the DM room is the dismissal path.
- `IgnoreListUpdated` refilters local non-DM messages in place with no DB refetch.
- `unignore` does not retroactively restore already-filtered local messages until a future tail/snapshot naturally reloads them.

---

## 11. Synthetic Entries

Synthetic entries are selected from the room list but are not normal `ChatRoom`s.

### News

- Backed by persisted `articles`.
- `ArticleService::process_url` extracts title/summary/image, stores an article, and posts a compact `---NEWS---` announcement into `#general`.
- Announcement payload format is `NEWS_MARKER title || summary || url || ascii`.
- Rendering/parsing of announcement cards lives in `ui_text.rs`.
- Delete removes the article and attempts to delete the matching news announcement by marker/user/url; article deletion can succeed even if chat cleanup only logs a warning.
- URL processing has a 5-minute timeout. Image ASCII fetch has byte, pixel, and time limits.
- News snapshot is global and lists recent articles; unread count is per user through `article_feed_reads`.

### Showcase

- Backed by persisted `showcases`.
- It is a separate feed and does not mirror posts into chat messages.
- Composer fields: title, URL, tags, description.
- `i` creates; `e` edits selected owned/admin entry; `d` deletes owned/admin entry; Enter copies selected URL when not composing.
- Validation requires title, `http://` or `https://` URL, and description.
- Title max is 120 chars; description max is 800 chars.
- Tags normalize lowercase, split on comma/whitespace, strip leading `#`, allow ASCII alnum plus `-_.`, cap each tag at 24 chars and total tags at 8.
- Snapshot is global and lists recent showcases; unread count is per user through `showcase_feed_reads`.

### Work

- Backed by persisted `work_profiles` and `work_feed_reads`.
- It is a separate feed and does not mirror posts into chat messages.
- Each user has at most one work profile; creating again updates the existing profile and preserves its public random slug (`w_` plus 12 lowercase alphanumeric chars).
- Composer fields: headline, status, type, location, contact, links, skills, summary.
- Status must be `open`, `casual`, or `not-looking`; aliases normalize in `work/state.rs`.
- Links require `http://` or `https://`, cap at 6, and are stored for later web rendering.
- Skills normalize lowercase, split on comma/whitespace, strip leading `#`, allow ASCII alnum plus `-_.`, cap each skill at 24 chars and total skills at 12.
- Public profiles show bio and showcases when the author has data for them. The composer does not expose include toggles.
- `i` creates or edits the caller's own profile; `e` edits selected owned/admin entry; `d` deletes owned/admin entry; Enter or `c` copies a recruiter-friendly profile summary when not composing.
- Snapshot is global and lists recent work profiles by latest update; unread count is per user through `work_feed_reads`.

### Notifications / Mentions

- Backed by `notifications` joined with actor, room, and message preview data.
- Snapshot is user-targeted; consumers must ignore snapshots where `snapshot.user_id != current_user`.
- Selecting Mentions lists notifications and marks all read optimistically; re-selecting Mentions through room-jump or mouse does the same.
- Enter jumps to the referenced room/message when possible.

### Discover

- Lists public topic rooms the current user has not joined.
- Uses `ChatService` events, not a separate service.
- `DiscoverRoomsLoaded { user_id, rooms }` and `DiscoverRoomsFailed { user_id, message }` are user-targeted.
- `start_loading()` clears stale rows until results arrive; empty loaded state is distinct from loading.
- Enter joins the selected public room.

---

## 12. Rendering Constraints

Full Chat layout:
- 26-column room sidebar.
- Fill-width message area.
- Composer at the bottom.
- Composer height is dynamic but capped at 8 lines.

Dashboard chat:
- Uses `DashboardChatView`.
- Composer is capped at 5 visible lines.

Embedded Rooms chat:
- Uses `EmbeddedRoomChatView`.
- Composer is capped at 4 visible lines.
- Game-backed chat rooms are joined through Rooms flow, not the main Chat list.

Message rendering:
- Local message storage is newest-first.
- Rendering reverses to oldest-first rows with newest at the bottom.
- Selected messages replace the leading pad with a selection marker.
- Highlighted reply targets get background styling across the whole row range.
- Message wrapping is word-aware; hard splits are only valid for a single word longer than width.
- Display author labels are plain usernames without leading `@`; mention syntax still uses `@username`.
- The small Markdown subset supports headings, bold, italic, inline code, blockquotes, and simple `- ` list items.
- `---NEWS---` cards use special boxed rendering.

Cache:
- `ChatRowsCache` stores wrapped rows plus selected/highlighted row ranges.
- Its fingerprint includes width, current user, current minute, message fields, usernames, countries, badges, bonsai glyphs, and reactions.
- Composer wrapped rows are cached separately in `ChatState`; invalidate when text or width changes.

---

## 13. Keybindings

### Chat Screen

| Key | Action |
|-----|--------|
| `h` / `l` / `left` / `right` | Switch room/synthetic selection |
| `Ctrl+N` / `Ctrl+P` | Next/previous room |
| `Space` | Room-jump mode |
| `j` / `k` / arrows | Move message selection or synthetic-list selection |
| `Ctrl+D` / `Ctrl+U` | Approximate half-page message selection |
| `i` | Start composing in selected room, or start News/Showcase/Work composer when selected |
| `/` | Start command composer in selected room |
| `Enter` | Submit composer; open selected chat news preview; jump reply target; copy URL in News/Showcase; copy Work summary; join Discover; jump Mention |
| `Alt+Enter` / `Ctrl+J` | Insert newline in main chat composer |
| `Alt+S` | Submit main chat composer and keep it open |
| `Esc` | Cancel compose/overlay/autocomplete/room jump |
| `r` | Reply to selected message |
| `e` | Edit selected own/admin message, Showcase entry, or Work profile |
| `d` | Delete selected own/admin message, News article, Showcase entry, or Work profile |
| `p` | Open selected author's read-only profile |
| `c` | Copy selected message body |
| `f` then `1..8` | React to selected message |
| `f` then `f` | Open reaction-owner overlay |
| `Ctrl+P` | Admin toggle selected-message pin |
| `C` | Open web chat QR/copy URL |
| `Ctrl+]` | Open icon picker; inserts only into main chat composer |

### Dashboard Chat

| Key | Action |
|-----|--------|
| `i` | Compose in dashboard active room |
| `j` / `k` / arrows | Move dashboard message selection |
| `r` / `e` / `d` / `p` / `c` / `f` | Same selected-message actions as Chat |
| `Enter` | Open selected news preview, or jump selected reply target when loaded |
| `[` / `]` | Cycle dashboard favorite rooms |
| `,` | Toggle previous dashboard favorite |
| `g<digit>` | Jump favorite slot 1..9 |

### Synthetic Entries

| Entry | Keys |
|-------|------|
| News | `j/k` navigate, `i` paste URL, Enter copy/submit URL, `d` delete own/admin article, `/` toggle filter to mine, `Esc` cancel |
| Showcase | `j/k` navigate, `i` create, `e` edit own/admin, `d` delete own/admin, Enter copy/submit, Tab cycle fields, `/` toggle filter to mine, `Esc` cancel |
| Work | `j/k` navigate, `i` create/edit own, `e` edit own/admin, `d` delete own/admin, Enter/`c` copy profile summary, Tab cycle fields, `/` toggle filter to mine, `Esc` cancel |
| Mentions | `j/k` navigate, Enter jump to referenced room/message |
| Discover | `j/k` navigate, Enter join selected public room |

Showcase and Work reshuffle their listing on every visit (entering or re-entering the entry via room list, room-jump, or mouse). News keeps its chronological order — only mine-only filtering applies. The slash-command composer in `app/input.rs` skips itself when News/Showcase/Work is selected so `/` reaches the synthetic-entry handler.

When changing keybindings, update root `CONTEXT.md`'s keybinding checklist plus the relevant input handler, help modal, footer hints, and tests.

---

## 14. Critical Flows

### Send/Edit/Delete

1. Composer submit creates a `request_id`.
2. `send_message_with_reply_task` or `edit_message_task` runs async DB work.
3. Service enforces membership. Reply targets must be in the same room.
4. `#announcements` is admin-only in the send path.
5. Message create/edit broadcasts full `ChatMessage` plus optional `target_user_ids`.
6. Sender receives success/failure ack keyed by `request_id`.
7. Delete hard-deletes by author or admin and broadcasts `MessageDeleted`.

`target_user_ids = None` means public event. `Some(ids)` means scoped event. Consumers rely on this for privacy and notifications.

### Tail And Delta Recovery

1. Visible-room changes request a tail.
2. Tail checks membership and loads newest 500 messages plus reactions and author metadata.
3. Tail merge dedupes by id, sorts newest-first, truncates to 500, and preserves ignored-user filtering.
4. Broadcast lag requests a visible-room tail reload.
5. Delta sync checks membership and loads up to 256 messages after `(created, id)`.

### Room Membership Commands

1. `/public #room` gets or creates a public topic room, forces `auto_join=false`, and joins only caller.
2. `/private #room` creates a private topic room and joins caller.
3. `/invite @user` requires caller membership and rejects DMs.
4. `/leave` rejects permanent rooms.
5. Admin `/fill-room #room` works only for public rooms, bulk-adds all users, and sets `auto_join=true`.
6. DMs always preserve canonical endpoints; sending repairs membership for both endpoints.

### Notifications

1. `send_message` calls `notification_svc.create_mentions_task`.
2. `ChatState` also queues desktop notifications locally for DMs and direct mentions.
3. Render drains `pending_notifications` through user settings in root `render.rs`.

---

## 15. Performance Notes

Landed/scoped-loading state:
- Username autocomplete is one shared directory watch.
- Per-user snapshots contain summaries only.
- Per-room tails are explicit and capped at 500.
- Discover metadata loads only when Discover is selected.
- Events patch local state and tail loads merge with already-applied live events.

Known risks:
- `ChatRowsCache` fingerprint still hashes visible message bodies and metadata. Keep row cache invalidation correct if changing wrapping/reactions/badges.
- Summary snapshot merge clones preserved message vectors for rooms with empty incoming message lists.
- Unread count SQL counts rows newer than `last_read_at`; if message volume grows, run `EXPLAIN ANALYZE`.
- Tail reload is the recovery path for lagged broadcasts, so keep it bounded and membership-protected.

Do not reintroduce the old per-session "load every room's history every 10s" behavior.

---

## 16. Tests

Repo-wide rule from root context still applies:
- Pure unit tests stay inline under `src/`.
- DB/service/network tests go in `late-ssh/tests/chat/`.
- LLM agents must not run `cargo test`, `cargo nextest`, or `cargo clippy`; note expected commands for the human owner instead.

Existing integration coverage:
- `tests/chat/svc.rs`: send, reactions, pins, summaries, room tails, ignored users, discover listing/joining, public room create/fill, delete events, ignore/unignore.
- `tests/chat/news.rs`: article snapshots, empty list, author resolution, duplicate URL failure, direct DB inserts appearing after list refresh.
- `tests/chat/showcase.rs`: create event/snapshot, non-owner update failure, admin delete, unread cursor behavior.
- `tests/chat/work.rs`: profile create/update snapshot behavior, public slug preservation, non-owner update failure, admin delete, unread cursor behavior.
- `tests/chat/state.rs`: placeholder; direct `ChatState` tests need accessors or indirect UI/input tests.

Existing unit coverage:
- `state.rs`: command parsing, autocomplete ranking, visual order, reply preview/target helpers, DM sort keys, textarea theme behavior.
- `input.rs`: room navigation aliases and reaction leader key parsing.
- `ui.rs`: title fitting, composer title degradation, visible rows, room-list rows, hit testing, scroll helpers.
- `ui_text.rs`: news parsing/rendering, reaction footer, wrapping, composer rows.
- Synthetic modules: selection clamp/move helpers, tag parsing, URL validation, payload sanitation, loading transitions.

Test gaps:
- Dedicated notification-service integration tests for mention creation/list/mark-read.
- Direct input-handler tests for News/Showcase/Work/Notifications/Discover.
- Direct `ChatState` synthetic-panel integration tests.
- Full News process success path is hard to cover because extraction depends on AI/search/network behavior.

---

## 17. Gotchas

- `selected_room_id` is not always the send target. Use `composer_room_id` for active composer submissions.
- `visible_room_id` drives read markers and tail loading.
- Snapshots may contain empty message vectors; empty means preserve existing local tail, not clear history.
- Message storage, recent queries, and tails are newest-first. Delta queries are ascending.
- `(created, id)` is the catch-up cursor.
- Any operation exposing room contents must check membership first.
- DM/private message bodies must not leak to non-members through broadcast handling.
- Ignore filtering is non-DM only.
- `#announcements` admin-only currently depends on the provided `room_slug`; stale/missing slug is a fragile path.
- Reaction and pin tasks are async; UI should not assume optimistic success.
- Pinned messages are loaded separately from summary snapshots and chat events.
- Room visual order must stay consistent between state and UI hit-testing/row-building.
- Mouse hit-testing reconstructs a temporary `ChatRenderInput`; room-list layout changes must keep hit tests in sync.
- News payload fields must sanitize the separator and newlines.
- Showcase and Work posts do not create chat messages; News posts do.
- Game rooms must remain opt-in and `auto_join=false`.
