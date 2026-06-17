# late-ssh Chat Context

## Metadata
- Domain: late.sh SSH chat, synthetic chat entries, and dashboard/room chat surfaces
- Primary audience: LLM agents working in `late-ssh/src/app/chat`
- Last updated: 2026-06-15
- Status: Active
- Parent context: `../../../../CONTEXT.md`

---

## 1. Scope

This file owns chat-specific context that used to make the root `CONTEXT.md` too large.

Included here:
- Home chat rooms, DMs, public/private topic rooms, synthetic entries, and game-backed room chat.
- Home/Dashboard chat center, room rail, and embedded Rooms chat surfaces.
- Message composer, replies, edits, deletes, reactions, pinned messages, ignores, overlays, and autocomplete.
- Synthetic chat entries: RSS, News, Mentions/Notifications, Voice, and Discover. Showcase/Projects and Work/Profiles still use chat-adjacent services/state, but their UI is hosted on Directory page 7.
- Chat service refresh/tail/event contracts, DB model constraints, keybindings, tests, and gotchas.

Global SSH, audio, games, profile, rooms/blackjack, observability, and repo-wide test policy stay in the root context.

---

## 2. File Map

```text
late-ssh/src/app/chat/
|-- mod.rs                       # Module declarations only
|-- svc.rs                       # ChatService: DB boundary, snapshots, events, room/message tasks
|-- state.rs                     # ChatState: local UI state, receivers, composer, room/message selection
|-- input.rs                     # Home chat input plus shared message actions used by Dashboard/Rooms
|-- ui.rs                        # Home room rail/chat center, dashboard-lounge view, embedded room chat, composer, row cache
|-- ui_text.rs                   # Message/news/reaction wrapping into ratatui Lines
|-- discover/                    # Synthetic Discover entry: public rooms not yet joined
|-- feeds/                       # Synthetic RSS entry: private per-user RSS/Atom inbox
|-- news/                        # Synthetic News entry: articles + #lounge announcement
|-- notifications/               # Synthetic Mentions entry: mention notifications
|-- polls/                       # /poll modal state/input/UI
|-- showcase/                    # Projects service/state/UI reused by Directory page 7
`-- work/                        # Profiles service/state/UI reused by Directory page 7
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
`showcase_feed_read.rs`, `work_profile.rs`, `work_feed_read.rs`, and `chat_poll.rs`.
Chat-owned moderation commands also use `room_ban.rs`,
`server_ban.rs`, `artboard_ban.rs`, and `moderation_audit_log.rs`.

---

## 3. Ownership Split

- `svc.rs` is the async boundary between TUI state, DB models, mention notifications, and broadcast/watch channels.
- `state.rs` owns local chat data, room/message selection, composer state, reply/edit/reaction/pin state, overlays, synthetic-entry substates, unread/read tracking, and cache inputs.
- `input.rs` maps Home chat keys to state/service actions. `handle_message_action_in_room` is shared by Home chat and embedded Rooms chat.
- `ui.rs` renders Home room rail/chat center surfaces and owns `ChatRowsCache`.
- `ui_text.rs` centralizes wrapping for normal messages, the small Markdown subset, reply quotes, `---NEWS---` cards, and reaction footers.

Keep `mod.rs` declaration-only; no `pub use` re-export layer.

---

## 4. Service And Data Flow

`ChatService` channels:
- Per-session `watch<ChatSnapshot>` for low-frequency room summary data.
- `broadcast<ChatEvent>` for live message, reaction, room-command, tail, and error events.
- Shared `watch<Arc<Vec<String>>>` username list for mention autocomplete, refreshed every 30s.
- Plain username display is centralized outside Chat in `State.username_directory` (`Uuid -> username`), loaded at startup, refreshed every 30 minutes, and updated on login/profile save/mod rename/account delete. Chat still owns richer author metadata such as bonsai glyphs, countries, badges, reactions, and unread state.
- A service-owned refresh scheduler that refreshes registered sessions every 10s and on explicit signals.
- `read_permits: Semaphore(8)` to cap concurrent snapshot, tail, discover, and pinned-message reads.
- `send_lounge_message_task` is the shared internal producer for custom `#lounge` announcements. It resolves `#lounge`, optionally joins the author first, then sends through the normal `send_message` path. News uses it with a request id so normal composer-style send success/failure events are preserved.

Important constants in `svc.rs`:
- `HISTORY_LIMIT = 500`
- `DELTA_LIMIT = 256`
- `PINNED_MESSAGES_LIMIT = 100`
- `CHAT_REFRESH_INTERVAL = 10s`
- `USERNAME_DIRECTORY_TTL = 30s`

Normal display flow:
1. `ChatState::new` subscribes to chat events/usernames and calls `ChatService::start_user_refresh_task`.
2. The per-user snapshot loads joined rooms, unread counts, latest-message activity timestamps, `#lounge` id, DM/current-user metadata, bonsai glyphs for those users, and ignored user ids.
3. Snapshots intentionally carry empty message vectors. They do not load history; activity timestamps are summary metadata used for stable room ordering.
4. Visible-room changes call `App::sync_visible_chat_room()`, which stores `visible_room_id`, marks the room read, and requests a room tail.
5. `load_room_tail_task` fetches the newest 500 messages, reaction summaries, author usernames, and author bonsai glyphs for the visible room. Render-time display names prefer the app-wide username directory over this per-session chat cache when both know the same UUID.
6. Broadcast `MessageCreated`/`MessageEdited`/`MessageDeleted`/reaction events patch local state. Broadcast lag triggers a tail reload for the visible room.

`ChatSnapshot` is summary data. `RoomTailLoaded` is history data. Do not merge those responsibilities back together.

Login announcements:
- `app::announcements::load_login_announcements` runs during SSH session bootstrap, outside `ChatState`.
- If public `#announcements` exists, the user is idempotently joined, up to the latest unread messages from other users are loaded from `chat_messages`, and `chat_room_members.last_read_at` advances to the newest displayed message.
- The resulting modal is stored on `App`, appears only after splash/settings are gone, consumes input while visible, scrolls with j/k, and closes on Enter/Esc/q.

---

## 5. DB Contracts

Room model:
- `chat_rooms.kind`: `lounge`, `language`, `dm`, `topic`, `game`.
- `chat_rooms.visibility`: `public`, `private`, `dm`.
- `lounge` must have slug `lounge`, is public, auto-join, and permanent.
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
- Each user has at most one icon-picker reaction per message.
- Message/user deletion cascades remove reactions.

Notifications:
- Mentions are stored in `notifications`.
- Mention unread state is cursor-based through `mention_feed_reads`.
- Mention resolution excludes the actor and recipients who ignore the actor; DMs only notify DM participants, private rooms only members, and non-game public rooms may mention any user. Game-room chat does not create Mentions feed notifications.

---

## 6. Rooms And Selection

`RoomSlot` represents either a real room or one of the Home synthetic entries: RSS (`RoomSlot::Feeds`), News, Notifications/Mentions, Voice, or Discover. `RoomSlot::Showcase` and `RoomSlot::Work` remain in code for state compatibility and focused helpers, but they are no longer emitted by Home visual order, room rail, or room jump.

Visual order is defined in `state.rs::visual_order_for_rooms` and mirrored by cozy room-rail rendering in `ui.rs`. The base navigation order is:
1. Favorite real rooms in `users.settings.favorite_room_ids` order.
2. Core permanent rooms: `lounge`, `announcements`, `suggestions`, `bugs`.
3. Notifications/Mentions.
4. News.
5. RSS, when the current user has at least one RSS/Atom subscription.
6. Other non-DM chat-list rooms/channels, excluding favorites.
7. DMs, sorted by unread status, then snapshot latest-message activity, then peer display name. Do not derive this order from lazily loaded room tails.
8. Discover / `+ browse rooms`.

Hub Shop room effects add render-time top sections in the cozy room rail. Active `room_bump` effects on non-permanent public topic rooms render first under a dedicated `bumped` section as plain synthetic `join #slug` text rows; the synthetic row never shows glow/spark/pulse/hack/bump suffixes. The real room stays in its normal navigation section if the viewer has it, and pressing Enter on the synthetic row joins/moves through the existing public-room join path. `room_spark`, `room_glow`, and `room_pulse` are one-minute page-level visuals over the selected room content; they must not add top text, promote rooms, or restyle room-list rows. `pinned_vibe` is sold as Hack Room: for one hour it is the only effect allowed to change real room-list text/color, adding the `hacking` suffix for every viewer. Active effects flow through `ChatRoomListView.active_room_effects`. Hit testing uses the same visual slot list, so bumped room clicks stay aligned with rendering.

RSS:
- RSS subscriptions are per-user and managed in `Settings -> RSS`.
- `rss_feeds` stores connected RSS/Atom URLs; `rss_entries` stores private pending entries.
- The background `FeedService` polls active feeds, parses a conservative RSS/Atom subset, stores unseen entries, and publishes per-user events.
- The RSS synthetic room (`RoomSlot::Feeds`) is private. Press `s` on an entry to share it through `ArticleService::process_url`; only then does it become a public News article and `#lounge` announcement.
- Enter copies the selected RSS entry URL, `d` dismisses it, and `r` asks the RSS poller to refresh.

Game rooms stay in `ChatState.rooms` for embedded Rooms chat, but `is_chat_list_room` hides them from the Home room rail/navigation and favorite-room picker.

Room navigation:
- `h`/`l`, left/right arrows, `Ctrl+P`/`Ctrl+N` switch room selection.
- `Space` activates room-jump mode, assigning keys from `ROOM_JUMP_KEYS`. Jumping to the already selected room/synthetic entry still re-runs the entry's read/list side effects so stale unread badges clear.
- Global `Ctrl+/` opens the room jump modal. Rows include unread counts and synthetic entries for RSS, News, Voice, Mentions, and custom room browse. Showcase/Projects and Work/Profiles live on Directory page 7 instead. Results are ordered favorites first, then unread entries, then latest message/activity; typed `@` and `#` prefixes filter to DMs or rooms while keeping that ordering.
- While composing on Home, `Ctrl+N`/`Ctrl+P` switch real rooms while preserving draft text and dropping reply/edit state.
- Synthetic entries are selected with booleans (`news_selected`, `notifications_selected`, `discover_selected`, `showcase_selected`, `work_selected`), not `selected_room_id`.

---

## 7. Home Shell And Embedded Chat

There is no top-level `Screen::Chat`. `Screen::Dashboard` renders as Home and owns both the room rail and the chat center:
- If `chat.selected_room_id` is `#lounge` and no synthetic entry is selected, the center renders `dashboard::ui::draw_dashboard`: optional top activity/quest/shop strip, pinned row when present, then lounge chat. Pinned messages have priority and render whenever present; when vertical space is tight, the top strip hides before chat.
- If any other real room or synthetic entry is selected, the center renders `chat::ui::draw_chat_center`.
- On wide terminals, `chat::ui::draw_room_list_rail` renders a borderless left rail. On narrow terminals, the center owns the available width.

Room favorites:
- Press `f` on a selected real room to toggle it in `ProfileState::toggle_favorite_room`.
- Press `[` / `]` on a selected favorite to move it up/down via `ProfileState::move_favorite_room`. No-op when the selection isn't a favorite or is already at the edge.
- Favorites are stored in `users.settings.favorite_room_ids` and the vec order drives both the Home room rail and the global picker.
- Favorites are no longer edited through a Settings tab.
- Active Shop room highlights are not favorites; they temporarily render above favorites and expire from `shop_consumable_effects`.

Home hot-room shortcuts:
- The room top boxes render up to four recent multiplayer seat joins from `dashboard::ui::recent_dashboard_rooms(..., 4)`. They are always visible for #lounge and optional on other Home rooms through the Settings "Activity boxes" row.
- `b1`, `b2`, `b3`, and `b4` enter those rooms through the same `rooms::input::enter_room` path used by the Rooms directory.

`App::sync_visible_chat_room()` is the read/tail-load bridge. It computes the visible chat room from Home/Dashboard or Rooms, stores it in `ChatState`, marks it read, and requests a tail on change. Call it after screen, selected room/synthetic entry, room favorite, or active-room changes.

There are separate `ChatRowsCache` instances on `App` for:
- Home lounge dashboard chat.
- Home chat center for the selected real room/synthetic entry.
- Rooms embedded chat.

Do not share a row cache across surfaces unless width and visible messages are guaranteed identical.

---

## 8. Composer, Commands, Reply, Edit

The main composer is a `ratatui_textarea::TextArea<'static>`.

`composer_room_id` is the authoritative send target while composing. This matters because Home and Rooms do not necessarily drive `selected_room_id` in the same way.

Starting compose in a room:
- Clears message selection.
- Clears reply target.
- Clears edit target.
- Stores `composer_room_id`.

Submit flow in `ChatState::submit_composer`:
- Commands are handled before normal send.
- `/leave` and `/invite` resolve through the active composer room or selected real room. Synthetic entries do not fall back to stale `selected_room_id` values; `/leave` on a selected synthetic entry exits that entry back to the last real room.
- `/members` uses the same real-room resolver as `/leave` and `/invite`.
- Normal send calls `send_message_with_reply_task`.
- Edit calls `edit_message_task`.
- Enter submits and closes.
- `Alt+S` submits and keeps the composer open.
- The `keep_composer_focused` Tweaks setting flips Enter to behave like
  `Alt+S` (send and stay) and disables the `Alt+S` binding while on; the
  composer title hint and Chat help section collapse to match.
- `Alt+Enter` and `Ctrl+J` insert a newline in the main chat composer.

User commands:
- `/active` opens an overlay from in-memory `active_users`, including repeated-session counts.
- `/friend @user` privately marks a user as a friend; `/unfriend @user` removes the mark; `/friends` lists marked users.
- `/binds` opens the Chat help topic.
- `/dm @user` opens/creates a DM.
- `/exit` opens quit confirm.
- `/icons` opens the icon picker (same as `Ctrl+]`).
- `/poll` opens a modal for the currently visible real room. Polls are room-scoped, support two or three options, can run for 10, 20, or 30 minutes, and are limited to one active poll per room. Active polls render at the top of the room message pane; while one is visible, `va`, `vb`, and `vc` vote for poll options. `v1`, `v2`, and `v3` remain music stream/station selectors. Failed starts show the remaining active wait in the banner.
- `/ignore [@user]` mutes a user or lists muted users.
- `/invite @user` adds a user to the selected non-DM room.
- `/leave` leaves the selected non-permanent room.
- `/list` lists public rooms.
- `/members` lists selected-room members.
- `/mod` opens the moderation command modal; `/mod ...` in chat is rejected because commands run only in the modal.
- `/paste-image` asks a paired `late` CLI with `clipboard_image` capability to read the local system clipboard image, sends it back over `/api/ws/pair`, uploads the PNG bytes through the normal image upload path, and inserts the resulting public URL into the composer. Pending clipboard requests time out after 15s so a dead paired client cannot wedge the command.
- `/petname [name]` shows or sets the user's cat name; `/petname clear` removes it.
- `/brb [message]` posts a short away message to the active composer room, marks the session away in the sidebar, publishes a moon badge next to that user's chat name for everyone while any active session is away, and mutes paired audio if it was not already muted. Sending a normal chat message clears away state for that session and only unmutes paired audio when `/brb` performed the mute.
- `/coffee` and `/tea` post a small ASCII-cup chat message to the current room as a coffee/tea-break ritual. No arguments. Steam pattern rotates per invocation through `CUP_VARIANT_COUNT` variants tracked on `ChatState::next_cup_variant` (session-local, not persisted). Routes through the normal `send_message_with_reply_task` send path — the body is a regular chat message subject to the same length/visibility rules.
- `/private #room` creates a private topic room and joins the caller.
- `/profile [@user]` opens a user's read-only profile modal. Bare `/profile` opens the caller's own profile as others see it. `@username` autocompletion is available after `/profile `.
- `/public #room` opens or creates an opt-in public room for the caller only (`auto_join=false`).
- `/sheet [@user]` (room-scoped to `#dnd`) opens the character sheet modal: bare form opens your own sheet editable (name + freeform body, saved per user per room on field submit via `ChatService::save_sheet_task`); targeted form opens another user's sheet read-only, or banners if they have none. Resolution and fetch happen in `ChatService::open_sheet_task`; saves and reads validate the shared `RoomScopedCommand` metadata plus room membership in `ChatService::ensure_room_scoped_command_access`; the modal lives in `app/sheet_modal`.
- `/settings` opens settings.
- `/unignore [@user]` removes an ignored user.
- `/upload <url>` downloads a public image URL server-side, reuploads it to configured public file storage, and inserts the resulting URL into the composer for the user to send.

Admin commands:
- `/create-room #room` creates/promotes a permanent auto-join room and bulk-adds existing users.
- `/delete-room #room` deletes a permanent room.
- `/fill-room #room` bulk-adds all users to an existing public room and flips `auto_join=true`; private rooms cannot be filled.

Moderation modal commands:
- `rename-room <#oldname> <#newname>`
- `rename-user <@oldname> <@newname>`
- `view <@user|#room|bans|audit|artboard|help> [pagenumber]`
- `artboard curate <live|YYYY-MM-DD> [reason...]`
- `artboard restore [YYYY-MM-DD] [reason...]`
- `kick <server|#room> @name [reason...]`
- `ban <server|#room|artboard|audio> @name [duration] [reason...]`
- `unban <server|#room|artboard|audio> @name [reason...]`
- `admin`
- `admin grant mod @name`
- `admin revoke mod @name`

Moderation list pages show 15 rows. Durations use positive `s/m/h/d` suffixes.

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
- Pressing `/` while not composing on Home starts command compose for the active room, except on News where `/` is a synthetic-entry filter toggle. Directory Profiles/Projects use `/` as the mine-only filter inside page 7.

Image uploads and inline rendering:
- File-upload storage is optional. It is enabled only when `LATE_FILES_S3_ENDPOINT`/`S3_ENDPOINT`, `LATE_FILES_S3_BUCKET`, `LATE_FILES_PUBLIC_BASE_URL`, and S3 credentials are present. Infra variable details live in `infra/README.md`.
- Pasting raw PNG/JPEG/GIF/WebP bytes into the chat composer starts an upload because there is no stable URL to preview until the bytes are hosted.
- Pasting an image URL does not upload or rehost it. It is inserted as normal composer text; after send, inline rendering previews that URL best-effort.
- `/upload <url>` is the explicit URL upload path: it downloads a public image URL server-side, reuploads it to configured public file storage, and inserts the resulting URL into the composer for the user to send and preview.
- `/paste-image` is the explicit paired-CLI clipboard path. It requires an updated `late` paired client, not just browser pairing or plain `ssh`.
- Non-admin uploads use a per-session `ChatState` cooldown. This is intentionally lightweight, not a server-side quota.
- URL downloads for upload and inline rendering must go through `files::image_upload::download_url_bytes`: validate `http(s)`, reject localhost/private/link-local/reserved resolved IPs, pin reqwest DNS to the validated addresses, disable redirects, and stream with a hard byte cap. Do not add new ad hoc `reqwest.get(url).bytes()` paths for chat images.
- Inline image rendering detects likely image URLs in visible room messages, fetches them through the same secure downloader, rejects oversized decoded dimensions, retries transient failures with backoff, and caches an `InlineImagePreview` by message id. Inline previews are only the RGB block fallback used by scrolling chat rows. Kitty/iTerm2/Sixel native image data is fetched separately, lazily, only while the explicit selected-message image modal is open on a supported terminal. Inline previews are best-effort; failures are intentionally silent/noisy only at trace level.
- Kitty, iTerm2, and Sixel image support is intentionally narrow and modal-only. `files::terminal_image` detects Kitty-family terminals from PTY `TERM`, XTVERSION, and forwarded env hints: Kitty, Ghostty, Rio, Warp, and Konsole. It detects iTerm2-family support from `TERM_PROGRAM`/`LC_TERMINAL`, XTVERSION, `TERM_FEATURES`, `OSC 1337;Capabilities`, and env hints for iTerm2, WezTerm, mintty, and hterm-style identities. It detects Sixel from explicit identities (`windows terminal`, `foot`, `contour`, `mlterm`, `sixel`), `WT_SESSION`/`WT_PROFILE_ID` env hints, and DA1 (Primary Device Attributes) replies advertising attribute 4 — the DA1 probe is sent last at alt-screen entry and only fills in Sixel when no richer protocol was detected, so Kitty/iTerm2 always win over Sixel. If `TERM` is tmux, full image previews are intentionally disabled and chat uses the RGB block fallback; no tmux graphics passthrough is attempted. Unsupported or undetected terminals, including stock Alacritty, keep the RGB block preview. Kitty images use late.sh-owned ids in the `0x4C000000..0x4CFFFFFF` range plus a dedicated z-index so cleanup can target them by range/z-index as well as by visible placement. Sixel payloads are generated only for Sixel sessions, use adaptive palette fallback, and fail back to the RGB block preview if the final payload still exceeds the hard byte cap. Because Sixel has no terminal-side scaling, the image modal reports its image cell capacity into `TerminalImageFrame` during draw, the render loop feeds it back into chat state, and Sixel fetches encode to fit that capacity (first fetch is deferred one frame after the modal opens until capacity is known; a cached Sixel encode that no longer fits, e.g. after shrink, is re-fetched at the new capacity). A forced repaint resets terminal image placement state so modal images are re-emitted after clear/resize/drop recovery. Direct terminals get Kitty cleanup commands on enter/leave alt-screen. Alt-screen enter/leave and forced full repaint begin with an ST terminator so a killed session that left iTerm2/Sixel inside an unterminated DCS/OSC image payload can recover before normal clear/repaint bytes. Closing an iTerm2 or Sixel image modal forces a full repaint because those inline images are not tracked/deleted like Kitty placements.

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
- Digits `1..9` while reaction leader is active toggle quick reactions, exit reaction leader mode, and keep the message selected.
- Digit `0` while reaction leader is active opens the icon picker for a custom reaction.
- `Ctrl+P` toggles selected-message pin state; admin only.

Selection deltas are message-based, not row-based. Positive means older, negative means newer.

---

## 10. Reactions, Pins, Ignores

Reactions:
- One reaction per `(message_id, user_id)`.
- Reactions are stored as icon text in `chat_message_reactions.icon`.
- Quick reaction keys `1..9` map to the default emoji set; `0` opens the full icon picker.
- UI appends reaction footer chips under the message body or news card.
- Reaction summaries live in `message_reactions: HashMap<Uuid, Vec<ChatMessageReactionSummary>>`.
- Reaction-owner overlay waits for a matching `ReactionOwnersListed` event keyed by `pending_reaction_owners_message_id`.

Pins:
- `chat_messages.pinned` is global, not scoped to a room or user.
- Only admins can toggle pins.
- Toggling pin does not optimistically update local pinned dashboard state.
- Home pinned stack comes from `load_pinned_messages_task` through a separate watch channel, not from the 10s summary snapshot.

Ignores:
- `users.settings.ignored_user_ids` stores UUIDs, not usernames.
- `users.settings.friend_user_ids` stores private one-way friend marks as UUIDs.
- `/ignore @user` and `/unignore @user` resolve usernames at command time.
- Ignore filtering applies to non-DM rooms only.
- DMs intentionally bypass ignored-user filtering; leaving the DM room is the dismissal path.
- `IgnoreListUpdated` refilters local non-DM messages in place with no DB refetch, then refreshes the Mentions list/unread count.
- `unignore` does not retroactively restore already-filtered local messages until a future tail/snapshot naturally reloads them.

---

## 11. Synthetic Entries

Synthetic entries are selected from the room list but are not normal `ChatRoom`s.

### News

- Backed by persisted `articles`.
- `ArticleService::process_url` extracts title/summary/image, stores an article, and posts a compact `---NEWS---` announcement into `#lounge`.
- Announcement payload format is `NEWS_MARKER title || summary || url || ascii`.
- Rendering/parsing of announcement cards lives in `ui_text.rs`.
- Delete removes the article and deletes matching news announcements by marker/user/url, then broadcasts silent `MessageRemoved` chat events so active #lounge views drop the generated card without showing a second message-delete banner; article deletion can still succeed if chat cleanup only logs a warning.
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
- Public profiles show bio, late.fetch fields, and showcases when the author has data for them. The composer does not expose include toggles. `WorkFeedItem` carries the owner `Profile` projection so the Directory detail panel can preview the same public-page sections without per-row DB calls.
- `i` creates or edits the caller's own profile; `e` edits selected owned/admin entry; `d` deletes owned/admin entry; Enter or `c` copies the selected public work profile link when not composing.
- Snapshot is global and lists recent work profiles by latest update; unread count is per user through `work_feed_reads`.

### Notifications / Mentions

- Backed by `notifications` joined with actor, room, and message preview data.
- Snapshot is user-targeted; consumers must ignore snapshots where `snapshot.user_id != current_user`.
- List and unread queries exclude notifications whose actor is in `users.settings.ignored_user_ids`.
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

Home chat center:
- The room rail is rendered by `draw_room_list_rail` outside the center pane when the terminal is wide enough.
- The center pane renders messages or a synthetic entry, with the composer at the bottom.
- Composer height is dynamic but capped at 8 lines.

Home lounge dashboard chat:
- Uses `DashboardChatView`.
- Composer is capped at 5 visible lines.
- Lounge chrome is controlled by the user's Dashboard Header setting, then by vertical priority: pinned row always renders when present, and the top activity/quest/shop strip drops before chat when space is tight.

Embedded Rooms chat:
- Uses `EmbeddedRoomChatView`.
- Composer is capped at 4 visible lines.
- Game-backed chat rooms are joined through Rooms flow, not the Home room rail.

Message rendering:
- Local message storage is newest-first.
- Rendering reverses to oldest-first rows with newest at the bottom.
- Selected messages replace the leading pad with a selection marker.
- Highlighted reply targets get background styling across the whole row range.
- Message wrapping is word-aware and uses Unicode display width, not codepoint count; hard splits are only valid for a single word longer than width.
- Display author labels are plain usernames without leading `@`; mention syntax still uses `@username`.
- Author labels render as `username [profile awards] [special...] [bonsai] [badge] [flag] [brb]`. Special badges come from a hardcoded per-username allowlist in `chat/special_badges.rs` and must stay in `mod`, `developer`, `artist` order. The bonsai glyph comes from `bonsai_glyphs` keyed by user_id. Profile award badges come from `profile_award_badges` keyed by user_id: top-3 last-completed-UTC-month leaderboard awards plus rankless Lateania boss achievement badges (`LAD`, `LFK`), ordered by rank and then category priority, rendered as one bracketed group. Equipped store badge and flag are split for separate hit targets and rendered badge before flag. The `/brb` moon badge is derived from shared `ActiveSession.afk`, not message metadata, so it is visible to all viewers while the author is away. Hub Shop Bot Username Color sets `bot_username_color_active` for the buyer and brightens `bot`, `graybeard`, and `dealer` author labels while active; chat row fingerprints include that flag.
- Author badge glyphs are separated by `AUTHOR_BADGE_SEPARATOR` (` `). The separator was intentionally returned to a plain space after dot separators failed to prevent terminal-cell drift.
- Investigation note: if a known author glyph is missing on a newly rendered message but appears after terminal resize, first suspect Ratatui/crossterm diff rendering of wide emoji cells, not author metadata. Sent-message events reload author metadata before `push_message`, chat row fingerprints include `bonsai_glyphs`, `chat_badges`, `profile_award_badges`, and AFK state, and resize forces a full terminal clear/redraw. A prior workaround forced full repaint on message-selection scroll, but it was removed because it caused visible flicker; prefer a targeted ratatui/backend fix for wide/VS16 emoji cell drift.
- Ratatui wide/VS16 investigation detail: Ratatui owns the buffer diff model: it renders widgets into a buffer, diffs current vs previous, then writes only changed cells to the backend. Official docs describe that flow at `https://ratatui.rs/concepts/rendering/under-the-hood/`. In this app's failure mode, `ratatui-core` emits extra trailing-cell updates for wide VS16 emoji, while `ratatui-crossterm` prints `cell.symbol()` but tracks the last position as if every printed symbol advances exactly 1 cell. A glyph like `🛡️` is one visible grapheme but 2 terminal cells wide, so the backend's "next update is adjacent, no `MoveTo` needed" optimization can become wrong after wide glyphs. This should be treated first as a Ratatui backend/diff issue, not a `crossterm` crate issue: crossterm is printing what Ratatui asks it to print, while Ratatui's backend decides when cursor moves are needed.
- Proposed upstream path: build a tiny repro outside late.sh that renders rows with `🛡️ 🔨️ 🌼`, then shifts/swaps rows like chat scrolling or room switching; add a Ratatui regression test around wide VS16 glyph diff/backend output; then patch either `ratatui-crossterm` cursor accounting or `ratatui-core`'s VS16 trailing-cell strategy. The naive backend fix is to track printed width instead of cell count, but test it carefully because Ratatui's explicit trailing-cell update may also need adjustment. A failing test/repro first will make the PR easier to get accepted.
- The small Markdown subset supports headings, bold, italic, inline code, blockquotes, and simple `- ` list items.
- `---NEWS---` cards use special boxed rendering.

Cache:
- `ChatRowsCache` stores wrapped rows plus selected/highlighted row ranges.
- Its fingerprint includes width, current user, current minute, message fields, usernames, countries, shop badges/flags, monthly award badges, bonsai glyphs, active `/brb` state, and reactions.
- Composer wrapped rows are cached separately in `ChatState`; invalidate when text or width changes.

---

## 13. Keybindings

### Home Chat Center

| Key | Action |
|-----|--------|
| `h` / `l` / `left` / `right` | Switch room/synthetic selection |
| `Ctrl+N` / `Ctrl+P` | Next/previous room |
| `Space` | Room-jump mode |
| `j` / `k` / arrows | Move message selection or synthetic-list selection |
| `Ctrl+D` / `Ctrl+U` | Approximate half-page message selection |
| `i` | Start composing in selected room, or start News composer when selected |
| `/` | Start command composer in selected room |
| `Enter` | Submit composer; open selected chat news preview; jump reply target; copy URL in News; join Discover; jump Mention |
| `Alt+Enter` / `Ctrl+J` | Insert newline in main chat composer |
| `Alt+S` | Submit main chat composer and keep it open. Dropped (no-op) while the `keep_composer_focused` Tweaks setting is on; Enter then owns send-and-stay. |
| `Esc` | Cancel compose/overlay/autocomplete/room jump |
| `r` | Reply to selected message |
| `e` | Edit selected own/admin message |
| `d` | Delete selected own/admin message or News article |
| `p` | Open selected author's read-only profile |
| `c` | Copy selected message body |
| `f` | Favorite/unfavorite the selected real room |
| `[` / `]` | Move the selected favorite up/down in the room rail |
| `f` then `1..9` | Quick-react to selected message |
| `f` then `0` | Open icon picker for a custom reaction |
| `f` then `f` | Open reaction-owner overlay |
| `Ctrl+P` | Admin toggle selected-message pin |
| `Ctrl+]` | Open icon picker; inserts only into main chat composer |
| Double-click composer bar | Enter compose mode (same as `i`). Dashboard + Rooms only. |
| Click message body | Move message selection to that block (same as `j`/`k` landing on it). |
| Double-click message body | Reply to that message (same as `r`). |
| Click username (or special / friend / bonsai / monthly award / brb badge) | Open that author's profile modal. Debounced ~280 ms so a fast double-click can promote to a mention instead. |
| Double-click username | Insert `@username ` into the composer for the current room. Cancels the debounced profile-open. |
| Click equipped chat-shop badge | Open Hub Shop on the Badges sub-store. |
| Click inline image preview | Select the message and open the image viewer modal. |

The composer rect is captured during `chat::ui` draw into `ChatState::last_composer_rect`
(a `Cell<Option<Rect>>` reset at the top of every frame in `app/render.rs`).
`app::input::handle_chat_composer_click` consumes left-button clicks inside that
rect, stashes the click on `ChatState::last_composer_click`, and on a second
click within 500 ms at the same cell calls `start_composing_in_room` with the
Dashboard's `selected_room_id` or the Rooms screen's `rooms_active_room`
chat-room id.

The chat scroll itself uses the same capture-on-draw pattern: each draw site
that paints messages (Home `#lounge` dashboard card, Home chat center
real-room branch, and embedded Rooms chat) publishes a `ChatHitLayout` into
`ChatState::last_chat_hit_layout` — a single `Cell<Option<ChatHitLayout>>`
reset alongside `last_composer_rect`. The layout pairs the content `Rect`
with one `ChatRowHit` per painted row (including leading viewport
padding rows as `kind: None`), and header rows carry per-segment column
ranges so a click can be resolved to the username, the equipped chat-shop
badge, or the bonsai glyph. `app::input::handle_chat_scroll_click`
consumes left-button clicks against the layout, gated by
`chat_scroll_clicks_blocked` (settings/hub/profile/quit/splash/bonsai/cat
modals and the icon picker). Username profile-opens are debounced via
`App::pending_chat_profile_open` and resolved from `App::tick` once
`PROFILE_CLICK_DEBOUNCE` (~280 ms) elapses with no matching double-click.

### Home Lounge Chat

| Key | Action |
|-----|--------|
| `i` | Compose in `#lounge` |
| `j` / `k` / arrows | Move message selection |
| `r` / `e` / `d` / `p` / `c` / `f` | Same selected-message actions as Home chat center |
| `Enter` | Open selected news preview, or jump selected reply target when loaded |

### Synthetic Entries

| Entry | Keys |
|-------|------|
| News | `j/k` navigate, `i` paste URL, Enter copy/submit URL, `d` delete own/admin article, `/` toggle filter to mine, `Esc` cancel |
| Directory Projects | `j/k` navigate, `i` create, `e` edit own/admin, `d` delete own/admin, Enter copy/submit, Tab cycle fields while composing, `/` toggle filter to mine, `Esc` cancel |
| Directory Profiles | `j/k` navigate, `i` create/edit own, `e` edit own/admin, `d` delete own/admin, Enter/`c` copy public profile link, Tab cycle fields while composing, `/` toggle filter to mine, `Esc` cancel |
| Mentions | `j/k` navigate, Enter jump to referenced room/message |
| Discover | `j/k` navigate, Enter join selected public room |

Directory Projects and Profiles reshuffle their listing on page/tab entry. News keeps its chronological order — only mine-only filtering applies. The slash-command composer in `app/input.rs` skips itself when News is selected so `/` reaches the synthetic-entry handler; Directory page 7 routes `/` directly to Projects/Profiles filtering.

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
7. Delete hard-deletes by author or admin and broadcasts `MessageDeleted`; linked data cleanup such as News announcement removal broadcasts silent `MessageRemoved`.

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
2. `ChatState` also pushes desktop notifications through its `app/notify` `Notifier` handle for friend joins, DMs, direct mentions, and newly started polls.
3. Render drains `App::notify_outbox` through user settings in root `render.rs`; see the notify-domain bullet in root `CONTEXT.md`.

---

## 15. Performance Notes

Landed/scoped-loading state:
- Username autocomplete is one shared directory watch.
- Per-user snapshots contain summaries only.
- Per-room tails are explicit and capped at 500.
- Discover metadata loads only when Discover is selected.
- Events patch local state and tail loads merge with already-applied live events.

Known risks:
- `ChatRowsCache` fingerprint still hashes visible message bodies and metadata. Keep row cache invalidation correct if changing wrapping/reactions/badges/AFK state.
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
- Login `#announcements` modal uses `chat_room_members.last_read_at`; do not add a separate announcement-read table unless the room model itself changes.
- Reaction and pin tasks are async; UI should not assume optimistic success.
- Poll create/vote tasks are async; `ChatEvent::PollUpdated` patches the local active-poll map and `ChatSnapshot.active_polls` refreshes authoritative visibility. Successful poll creation spawns a sleep-until-expiry finalizer that atomically claims the expired poll in Postgres, marks it inactive, and posts compact results into the room as the poll creator. `ChatService::start_poll_finalizer_recovery_task` runs a coarse 10-minute recovery scan for expired active polls so restarts/redeploys do not strand result posts; the DB claim is the cross-replica duplicate guard.
- Poll vote shortcuts use `va/vb/vc` when the selected/visible real room has an active poll, leaving music `v1/v2/v3` selectors available.
- Pinned messages are loaded separately from summary snapshots and chat events.
- Room visual order must stay consistent between state and UI hit-testing/row-building.
- Mouse hit-testing reconstructs a temporary `ChatRenderInput`; room-list layout changes must keep hit tests in sync.
- Chat-scroll mouse hit-testing is driven by `ChatRowsCache` extras (`row_message`, `row_kind`, `header_segments`) and a per-frame `ChatHitLayout` published into `ChatState::last_chat_hit_layout`. If you change how author headers, inline images, or reaction footers contribute rows in `ensure_chat_rows_cache` / `wrap_chat_entry_to_lines`, update both the parallel `row_*` vectors and the segment math in `build_author_prefix_and_segments` so a click still resolves to the right message/segment.
- News payload fields must sanitize the separator and newlines.
- Showcase and Work posts do not create chat messages; News posts do.
- Game rooms must remain opt-in and `auto_join=false`.
