# late-ssh Chat Context

## 1. Scope

Chat in the SSH app: Home chat rooms, DMs, public/private topic rooms, game-backed room chat, the Home/Dashboard chat center and room rail, and the embedded game-chat surfaces (house tables, daily match boards, the door watch view). Also message actions, reactions, ignores, overlays, the service's refresh/tail/event contracts and the DB model constraints.

- Synthetic chat entries: RSS, News, Mentions/Notifications, and Discover. Voice is not a synthetic room slot: the dedicated `#voice` room is a real, permanent, public chat room pinned at the bottom of Core (above Discover). Any voice-enabled chat/game room (including `#voice`) renders an embedded voice strip and exposes `/voice`/`/mute` controls while you are inside it. Showcase/Projects and Work/Profiles keep their feed services and state here, but their UI and their editor (`app/directory/editor`) live on Profiles page 5.
- Companion docs here: `COMMANDS.md` (the composer, every slash command, the keys), `RENDERING.md` (the rows cache, message rendering, images, layout rules), `MARKS.md` (the AFK line and the left-app device mark). Sub-domains with their own context: `cyberspace/`, `gild/`, `news/`, and `../crown/`; the round, translation and `/summary` are `../ai/CONTEXT.md`. A YouTube link in a message opens the watch player, `../video/CONTEXT.md`.
- The session shell, global keys and notifications are in `late-ssh/CONTEXT.md`; repo-wide rules in the root `CONTEXT.md`.

---

## 2. File Map

```text
late-ssh/src/app/chat/
|-- mod.rs                       # Module declarations only
|-- action.rs                    # Shared CTCP-style `/me` action encoding/parsing
|-- commands.rs                  # Slash-command registry (`COMMANDS`), scope rules, and popup ranking
|-- svc.rs                       # ChatService: DB boundary, snapshots, events, room/message tasks
|-- state.rs                     # ChatState: local UI state, receivers, composer, room/message selection
|-- input.rs                     # Home chat input plus shared message actions used by Dashboard and embedded game chat
|-- ui.rs                        # Home room rail/chat center, dashboard-lounge view, embedded room chat, composer, row cache
|-- ui_text.rs                   # Message/news/reaction wrapping into ratatui Lines
|-- list_ui.rs                   # Shared "mine only" filtered-list layout and status line for the synthetic entries
|-- slur.rs                      # Pure drunk-text transform applied to outgoing public-room messages (never touches a spending phrase, `../ai/ROUND.md`, or a memory trigger, `../ai/MEMORY.md`)
|-- special_badges.rs            # Hardcoded per-username badges in author labels (allowlist, edit and redeploy)
|-- cyberspace/                  # Cyberspace rail section: personal client for cyberspace.online, incl. their chat (cIRC)
|-- discover/                    # Synthetic Discover entry: public rooms not yet joined
|-- feeds/                       # Synthetic RSS entry: private per-user RSS/Atom inbox
|-- gild/                        # Gild tier picker: state/input/ui for the `g` message action
|-- history_modal/               # `/history` scroll-back modal: state/input/ui (§14 History Modal)
|-- news/                        # Synthetic News entry: articles, the reader (`reader/`), body fetch (`body.rs`), the #lounge card (`ui.rs::lounge_card_lines`)
|-- notifications/               # Synthetic Mentions entry: mention notifications
|-- polls/                       # /poll modal state/input/UI
|-- showcase/                    # Projects service/state/UI reused by Directory page 5
`-- work/                        # Profiles service/state/UI reused by Directory page 5
```

Message translation itself lives in `../ai/translate.rs`; chat owns only the
cache key, the display state, and the row (§14 Translation; the flow is `../ai/CONTEXT.md`).

Related tests:

Tests live beside the file they exercise as `<file>_test.rs`, wired with
`#[cfg(test)] mod <file>_test;` (the `*_internal_test.rs` twins reach
`pub(crate)` internals). The inventory and what each covers is in §16.

Core models used by chat live in `late-core/src/models/`:
`chat_room.rs`, `chat_room_member.rs`, `chat_message.rs`, `chat_message_reaction.rs`,
`chat_message_gild.rs`, `crown.rs`,
`notification.rs`, `rss_feed.rs`, `rss_entry.rs`, `article.rs`, `article_read.rs`, `cyberspace_account.rs`, `showcase.rs`,
`showcase_feed_read.rs`, `work_profile.rs`, `work_feed_read.rs`, and `chat_poll.rs`.
Chat-owned moderation commands also use `room_ban.rs`,
`chat_slow_mode.rs`, `server_ban.rs`, `artboard_ban.rs`, and `moderation_audit_log.rs`.

---

## 3. Ownership Split

- `svc.rs` is the async boundary between TUI state, DB models, mention notifications, and broadcast/watch channels.
- `state.rs` owns local chat data, room/message selection, composer state, reply/edit/reaction state, overlays, synthetic-entry substates, unread/read tracking, and cache inputs. It reads no palette: anything it builds for the screen carries ink, and `ui.rs` (or `common/overlay.rs`) picks the colour in the draw.
- `input.rs` maps Home chat keys to state/service actions. `handle_message_action_in_room` is shared by Home chat and the embedded game-chat panes.
- `ui.rs` renders Home room rail/chat center surfaces and owns `ChatRowsCache`.
- `ui_text.rs` centralizes wrapping for normal messages, the small Markdown subset, reply quotes, report cards, and reaction footers.

Keep `mod.rs` declaration-only; no `pub use` re-export layer.

---

## 4. Service And Data Flow

`ChatService` channels:
- Per-session `watch<ChatSnapshot>` for low-frequency room summary data.
- `broadcast<ChatEvent>` for live message, reaction, room-command, and error events shared by every session. Single-recipient payload events (`RoomTailLoaded`, `DeltaSynced`, `MessageSearchLoaded`, `MessageContextLoaded`, `DiscoverRoomsLoaded`, and their `*Failed` twins) do NOT ride the broadcast: `ChatService::send_user_event` delivers them point-to-point over a per-session `mpsc<ChatEvent>` registered in `refresh_sessions` (returned by `start_user_refresh_task`), so a 500-message tail is never cloned into every connected session. `ChatState::drain_events` drains the targeted channel ahead of the broadcast; both feed the same event match.
- Shared `watch<Arc<Vec<String>>>` username list for mention autocomplete, rebuilt every 30s from the in-memory `UsernameDirectory`, not the DB. Re-scanning `users` on that timer measures 1.9% of all DB execution time; the directory is written through on login/profile save/rename/delete, so the watch is both free and fresher. Do not point it back at `User::list_all_usernames` (that arm is the no-directory fallback for tests).
- Plain username display is centralized outside Chat in `State.username_directory` (`Uuid -> username`), loaded at startup, refreshed every 30 minutes, and updated on login/profile save/mod rename/account delete. Chat still owns richer author metadata such as bonsai glyphs, countries, badges, reactions, and unread state.
- A service-owned refresh scheduler that refreshes registered sessions every 10s and on explicit signals.
- `read_permits: Semaphore(8)` to cap concurrent snapshot, tail, and discover reads.
- `send_lounge_message_task` is the shared internal producer for custom `#lounge` announcements. It resolves `#lounge`, optionally joins the author first, then sends through the normal `send_message` path. The activity feed uses it (`app/activity/lounge.rs`).

Important constants in `svc.rs`:
- `HISTORY_LIMIT = 500`
- `DELTA_LIMIT = 256`
- `CHAT_REFRESH_INTERVAL = 10s`
- `USERNAME_DIRECTORY_TTL = 30s`
- `SEARCH_RESULTS_LIMIT = 50`, `SEARCH_MIN_CHARS = 3`, `SEARCH_CONTEXT_EACH_SIDE = 4` (message search)

Normal display flow:
1. `ChatState::new` subscribes to chat events/usernames and calls `ChatService::start_user_refresh_task`.
2. The per-user snapshot loads joined rooms, unread counts, latest-message activity timestamps, `#lounge` id, DM/current-user metadata, bonsai glyphs for those users, and ignored user ids.
   - `build_chat_snapshot` runs as **two pipelined rounds, not a serial chain**. Round one is `ChatRoom::list_for_user_with_state` plus `User::friend_and_ignored_user_ids`; round two is voice channels, active polls, author metadata, and room owners, all of which need the room or friend set from round one. Each round is a single `tokio::join!` over one pooled connection, and tokio-postgres pipelines concurrent queries on one connection, so a round costs roughly one round trip rather than one per query (same pattern as `late_core::models::leaderboard::fetch_leaderboard_data`). Postgres still executes them in order, so this buys latency, not server CPU. Latency is what matters here because `refresh_registered_sessions` walks every live session sequentially. **Do not reintroduce a serial `.await` per query.**
   - Rooms, unread counts, and latest-message timestamps come back from **one** query (`list_for_user_with_state`), because all three key off the same `chat_room_members` row. Measured on prod against the heaviest user (157 rooms): 1,452 buffers / 5.2 ms against 1,760 buffers / 8.1 ms for three separate queries, plus two fewer planning cycles. There is deliberately no separate `unread_counts_for_user` or `last_message_at_for_rooms`; a second copy of that SQL is a drift hazard.
   - Unread counts are capped at `ChatRoomMember::UNREAD_COUNT_CAP` (100) and the UI renders anything at the cap as `99+` (`ui.rs::format_unread_badge`). Uncapped, a never-opened `#lounge` counted all 127k messages per user per pass and was 43% of all DB execution time. Room ordering only tests `unread > 0`, so the cap is invisible to it. Do not restore an exact count without re-reading SCALE.md Pain Point 7.
   - The `· ` activity lines are excluded by comparing the author id against the system bot id that `ChatService::set_system_user_id` receives from the #lounge feed task at startup. Do not go back to reading `users.settings->>'system'` per message: that made Postgres hash the whole users table per query.
3. Snapshots intentionally carry empty message vectors. They do not load history; activity timestamps are summary metadata used for stable room ordering.
4. Visible-room changes call `App::sync_visible_chat_room()`, which stores `visible_room_id`, marks the room read, and requests a room tail.
5. `load_room_tail_task` fetches the newest 500 messages, reaction summaries, author usernames, and author bonsai glyphs. A tail that lands before the snapshot naming its room (a game-room join fires both loads at once, and the tail is the lighter query) is held in `ChatState::early_tails` and merged when the room arrives; nothing asks for a tail twice. Render-time display names prefer the app-wide username directory over this per-session chat cache when both know the same UUID.
6. Broadcast `MessageCreated`/`MessageEdited`/`MessageDeleted`/reaction events patch local state; tail/search/discover results arrive on the per-session targeted channel. Broadcast lag triggers a tail reload for the visible room (the targeted mpsc is unbounded and cannot lag).

Room tails deliberately carry **no** read cursor. Render inserts one synthetic `new messages` divider before the first message from someone else past this session's **AFK line** (`ChatState::afk_lines`), and one dim `you left` divider before the first past the **left-app mark** (`ChatState::device_left_at`), never past `chat_room_members.last_read_at`; see `MARKS.md` for why. The divider is render-only state in the chat row cache; do not persist it or count it as a chat message.

### The Two Marks

In `MARKS.md`: the AFK line and the left-app device mark, the two questions they answer, and why neither ever feeds the other.

## 5. DB Contracts

In `ROOMS.md`, DB contracts: room kinds and visibility, derived ownership, game rooms, membership and unread, `SendError`, messages, the translation cache, slow modes, gilds, reactions, mentions. The ones every change must respect:
- Membership (`chat_room_members`) is the authorization check for reading tails, syncing deltas, marking read, sending, reacting, listing members, and inviting.
- `last_read_at` drives unread counts and nothing else (`MARKS.md`).
- Messages are hard-deleted; tails are newest-first (`created DESC, id DESC`), deltas ascending after `(created, id)`.
- `message_translations` rows are written only by `TranslationService` and deleted by edits and the FK cascade.
- A send that did not land is a `SendError` (`Refused(SendRefusal)` or `Failed`), accounted once in `account_send_error`.

---

## 6. Rooms And Selection

In `ROOMS.md`, Rooms and selection: `RoomSlot`, the rail's visual order (favorites, Core, the stream section, unread DMs, channels, DMs), the sticky unread DM, `RoomSection`, Shop room effects on the rail, RSS, room navigation and rail scroll, room jump and message search. `state.rs::visual_order_for_rooms` and the `ui.rs` hit-test mirror must always agree.

---

## 7. Home Shell And Embedded Chat

There is no top-level `Screen::Chat`. `Screen::Dashboard` renders as Home and owns both the room rail and the chat center:
- If `chat.selected_room_id` is `#lounge` and no synthetic entry is selected, the center renders `chat::ui::draw_dashboard_chat_card`: the lounge chat card, full height. It is the one chat surface carrying the live strip (`DashboardChatView.live_strip`, drawn in the topic's place under any stream or voice row and above the poll strip; owned by `app/live/`, see that domain's CONTEXT).
- If any other real room or synthetic entry is selected, the center renders `chat::ui::draw_chat_center`.
- On wide terminals, `chat::ui::draw_room_list_rail` renders a borderless left rail. On narrow terminals, the center owns the available width.

Room favorites:
- Press `f` on a selected real room, or on Mentions, News, RSS or `+ browse rooms`, to toggle it in `ProfileState::toggle_favorite_room`. The four synthetic entries are stored under fixed sentinel ids (`state.rs` `synthetic_favorite_id` / `synthetic_slot_for_favorite_id`, the only code that interprets them; version nibble 0, so no v7 room id can collide). A favorited entry moves from Core into Favorites in both rail builders through the same pushed-id exclusion the rooms use; RSS stays hidden everywhere while feeds are unavailable.
- Press `[` / `]` on a selected favorite to move it up/down via `ProfileState::move_favorite_room`. No-op when the selection isn't a favorite or is already at the edge.
- Favorites are stored in `users.settings.favorite_room_ids` and the vec order drives both the Home room rail and the global picker.
- The rail is the only editor for favorites (`f`, `[`, `]`); there is no Settings tab for them.
- Active Shop room highlights are not favorites; they temporarily render above favorites and expire from `shop_consumable_effects`.

Home presence:
- There is no top activity strip; presence (online count + connected friends) lives in the right sidebar's pinned core block, and the public activity feed ships into #lounge as system messages (`app/activity/lounge.rs`) surfaced in the TUI as the one-row activity ticker above the composer, never as chat rows. The `b1`-`b4` recent-room jump keys died with the Rooms demolition.

`App::sync_visible_chat_room()` is the read/tail-load bridge. It computes the visible chat room from the current screen (Home/Dashboard, house table, daily board, Clubhouse, the Games hub while it has a door-game watch open, a player's own running DCSS, NetHack or Brogue game), stores it in `ChatState`, marks it read, and requests a tail on change. Call it after screen, selected room/synthetic entry, room favorite, or open-surface changes.

There are separate `ChatRowsCache` instances on `App` for:
- Home lounge dashboard chat.
- Home chat center for the selected real room/synthetic entry.
- Embedded game chat (house tables, daily match boards).
- The door watch view's chat (`app/door/spectate`, the watched player's room).

Do not share a row cache across surfaces unless width and visible messages are guaranteed identical.

---

## 8. Composer, Commands, Reply, Edit

In `COMMANDS.md`: the composer (reply, edit, mentions, autocomplete), every slash command, and the keys.

## 9. Message Actions

Shared message actions live in `chat::input::handle_message_action_in_room`.

Keys:
- `j` / `k` and arrows move selected message. When the selected message wraps taller than the pane, they (and the mouse wheel) first scroll by rows inside it, then move selection once its edge is reached: chat scroll is otherwise derived purely from selection (`ui.rs::effective_chat_scroll`), which pins a too-tall message's top and would leave its bottom unreachable. State is `ChatState::selection_scroll` (`rows` offset + renderer-measured `overflow`, both `Cell`s written back during draw, reset on any selection change); the measurement is one frame stale by design, so a held-down key skims across messages instead of crawling through every long one. `Ctrl+D`/`Ctrl+U` deliberately skip the fallback.
- `Ctrl+D` / `Ctrl+U` move by an approximate half-page in message units.
- `r` replies.
- `e` edits.
- `d` deletes (double-press `dd` to confirm; first press arms and banners `Press d again to delete`, any selection change disarms) and moves selection to an adjacent message.
- `p` opens the selected author's read-only profile modal.
- `c` copies the selected message body.
- `t` toggles the message's translation (see Translation below).
- `g` opens the gild tier picker (see Gilds below). Consumed whenever a message is selected: on your own message, or one in a DM, a private room, or a game/stream chat, it banners the refusal instead of opening (`ChatState::gild_target_in_room` answers those room rules locally; the rest stay with the service).
- Enter jumps from a reply to its loaded target.
- `f` enters reaction leader mode.
- `f` again while reaction leader is active opens reaction-owner overlay.
- Digits `1..9` and `w` (👋, wave hi) while reaction leader is active toggle quick reactions, exit reaction leader mode, and keep the message selected.
- Reaction leader shortcuts take priority over global page and Bonsai keys.
- Digit `0` while reaction leader is active opens the icon picker for a custom reaction.

Selection deltas are message-based, not row-based. Positive means older, negative means newer.

---

## 9b. Gilds, the Crown, the Round

- Gilds, the paid name colour for a public message: `gild/CONTEXT.md`.
- The Crown, the one-holder 💎 slot bought with `/crown take`: `../crown/CONTEXT.md`. Chat owns only the command and the glyph.
- The Round, a drink or a whole-bar round from @bartender (the chips, the pour, the drunk points, the trigger phrase): `../ai/ROUND.md`, beside the bartender that runs it.

## 9e. Bartender Memory

- What @bartender remembers about a patron (his kept exchanges, "@bartender remember ...", "forget me"): `../ai/MEMORY.md`, beside the bartender that runs it. Chat lends two things: `ChatService::send_bot_reply`, the awaited bot send that hands back the row he remembers off, and the drunk-text protection of both triggers in `slur.rs` (§14 Drunk Text).

---

## 10. Reactions, Ignores

Reactions:
- One reaction per `(message_id, user_id)`.
- Reactions are stored as icon text in `chat_message_reactions.icon`.
- Quick reaction keys `1..9` map to the default emoji set; `w` waves hi (👋); `0` opens the full icon picker.
- UI appends reaction footer chips under the message body.
- Reaction summaries live in `message_reactions: HashMap<Uuid, Vec<ChatMessageReactionSummary>>`.
- Reaction-owner overlay (`ff`) waits for a matching `ReactionOwnersListed` event keyed by `pending_reaction_owners_message_id`. The event also carries the message's gilds (`ChatMessageGild::list_for_message`, best tier first), which `reaction_owner_lines` lists above the reactions as one block per tier held (`◆◆◆ 1 Gold gild`, buyers under it), sharing the reaction blocks' name capping.

Ignores:
- `users.settings.ignored_user_ids` stores UUIDs, not usernames.
- `users.settings.friend_user_ids` stores private one-way friend marks as UUIDs.
- `/ignore @user` and `/unignore @user` resolve usernames at command time.
- A message is hidden if its author is ignored, OR if `chat_messages.reply_to_user_id` is an ignored user. The latter hides bot/automated replies directed at an ignored user so they cannot be heard by proxy through `@bot`/`@graybeard`/`@bartender`. Only bots set `reply_to_user_id` (via `ChatService::send_bot_reply_task` or its awaited `send_bot_reply`); human replies use `reply_to_message_id`. The shared filter helper is `state::message_is_ignored_in`.
- Ignore filtering applies to DMs too. An ignored peer's DM messages are filtered, and the DM room is hidden from the room rail/navigation while the peer is ignored (both mirrors skip DMs whose `dm_peer_id` is ignored: `visual_order_for_rooms` and the rail's own DM filter, via the shared `dm_peer_is_ignored`), so a new DM from the ignored user can't resurface the room or its unread badge. Unignoring restores the DM on the next render/snapshot.
- `IgnoreListUpdated` refilters local messages in place (all rooms, including DMs and `reply_to_user_id` matches) with no DB refetch, then refreshes the Mentions list/unread count.
- `unignore` does not retroactively restore already-filtered local messages until a future tail/snapshot naturally reloads them.
- Ignores cover messages only. A News article is not a message: the News room shows every share, whoever shared it. Its #lounge card is the sharer's own line, so ignoring the sharer hides the card like the rest of their lines (§11 News).

---

## 11. Synthetic Entries

Synthetic entries are selected from the room list but are not normal `ChatRoom`s.

### News

Shared links, their #lounge cards, the News room and the reader: `news/CONTEXT.md`.

### Showcase

- Backed by persisted `showcases`.
- It is a separate feed and does not mirror posts into chat messages.
- Edited in the profile editor's projects page (`app/directory/editor`); this state only lists, saves (`save(params, editing)`), and deletes (`delete_project(id)`).
- Validation (in the editor) requires title, `http://` or `https://` URL, and description.
- Title max is 120 chars; description max is 800 chars.
- Tags normalize lowercase, split on comma/whitespace, strip leading `#`, allow ASCII alnum plus `-_.`, cap each tag at 24 chars and total tags at 8.
- Snapshot is global and lists recent showcases; unread count is per user through `showcase_feed_reads`, loaded once at session start (`refresh_unread_count`) and zeroed on visiting Profiles page 5. It does not move live when someone else posts: that needs a fresh session. Writes publish only the writer's own `Created`/`Updated`/`Deleted`/`Failed` event.
- The profile modal does not read this snapshot: `ProfileSnapshot::showcases` carries every showcase of the viewed user, newest first (`Showcase::list_by_user_id`), loaded with the rest of the profile on open.

### Work

- Backed by persisted `work_profiles` and `work_feed_reads`.
- It is a separate feed and does not mirror posts into chat messages.
- Each user has at most one work profile; creating again updates the existing profile and preserves its public random slug (`w_` plus 12 lowercase alphanumeric chars).
- Edited in the profile editor's card page (`app/directory/editor`); this state only lists, saves (`save(params, editing)`), and deletes (`delete_card(id)`).
- `status` is `WorkStatus` (open, casual, not-looking) and `work_type` is `WorkType` (full-time, contract, freelance, part-time, any), both text columns parsed at the row boundary in `late_core::models::work_profile`.
- Links require `http://` or `https://`, cap at 6, and are stored for later web rendering.
- Skills are canonical tags from the vocabulary (`late_core::vocab`), picked in the tag picker, twelve at most; `skills_tags` holds the same list, the array the job matcher joins on.
- Public profiles show bio, late.fetch fields, and showcases when the author has data for them. The composer does not expose include toggles. `WorkFeedItem` carries the owner `Profile` projection so the Directory detail panel can preview the same public-page sections without per-row DB calls.
- Snapshot is global and lists recent work profiles by latest update; unread count is per user through `work_feed_reads`, loaded once at session start and zeroed on visiting Profiles page 5, the same as Showcase: no live bump when someone else posts.

### Cyberspace

- late.sh as a personal client for cyberspace.online. The slice (`chat/cyberspace/`), its API-terms contract, token model, views/modals, their chat (cIRC), and tests live in `cyberspace/CONTEXT.md`: read that before touching anything in the directory.
- What stays in this file: `/cs` (alias `/cyberspace`) parsing/dispatch on `ChatState` (see Commands), and the rail contract. A linked account gets its **own** `RoomSection::Cyberspace` (shortcut `y`, folds like any other section) holding the pane (`feeds`), a `notifications` row, one row per pinned cIRC chat room, and one per pinned C-Mail conversation; `cyberspace_linked` gates the section in **both** `visual_order_for_rooms` and the two rail builders, since gating one and not the other leaves a slot the user can arrow onto but never see.
- `RoomSlot::CyberspaceRoom(index)` and `RoomSlot::CyberspaceMail(index)` are the **dynamic** slots: they index `ChatState::cyberspace.pinned_rooms()` / `pinned_cmail()`, with `cyberspace_room_selected` / `cyberspace_mail_selected` as the matching selections. Because a room or conversation streams only while it is open, **every** selection change must close it: that is why `clear_synthetic_selection` (the one place the synthetic-entry bools are reset) calls `cyberspace.leave_room()`. A future entry that clears selection by hand instead of through that helper would leave a room streaming behind the user's back.
- The one rule that crosses domains: **their API terms are load-bearing** (no bots, no scraping/caching for redistribution, no feeding their content to AI). Fetched content renders only for the user who fetched it, and `news/svc.rs::is_ai_blocklisted_url` hard-stops cyberspace.online URLs at the summarizer.

### Notifications / Mentions

- Backed by `notifications` joined with actor, room, and message preview data.
- Snapshot is user-targeted; consumers must ignore snapshots where `snapshot.user_id != current_user`.
- List and unread queries exclude notifications whose actor is in `users.settings.ignored_user_ids`.
- Selecting Mentions lists notifications and marks all read optimistically; re-selecting Mentions through room-jump or mouse does the same.
- Enter always opens the Ctrl+/ modal as a single-message preview (the mention with its 4-either-side context window), whatever the mention's age; Enter inside the modal performs the actual jump, so going to the room is Enter-Enter. The preview path: `ChatState::start_message_preview` fires `ChatService::load_message_preview_task` (`ChatMessage::get_for_viewer` (members always; public non-game rooms readable by anyone, since public-room mentions can target non-members — Enter-jump from such a preview banners toward Discover instead of selecting an unjoined room)), which reuses the `MessageSearchLoaded` pipeline to show the full message in the modal's detail pane; from there Enter jumps (selecting the message when it is in the loaded tail, else via `pending_search_jump`) and Ctrl+Y copies.

### Discover

- Lists public topic rooms the current user has not joined.
- Uses `ChatService` events, not a separate service.
- `DiscoverRoomsLoaded { user_id, rooms }` and `DiscoverRoomsFailed { user_id, message }` are user-targeted.
- `start_loading()` clears stale rows until results arrive; empty loaded state is distinct from loading.
- Enter joins the selected public room.
- `s` cycles the sort between `activity` (the query's own order: most recently active first) and `members` (largest first, slug breaking ties). The sort lives in `discover::state::SortMode` and is applied in `visible_items()` after filtering, so it reorders search results too; cycling resets the selection to the top because the old index points at a different room afterwards. Activity is the default and adds no sort work.
- Rooms render two rows each (`ITEM_HEIGHT = 2` in `discover/ui.rs`): `#slug` plus the room's topic when a mod has set one (clipped to the row), then member/message counts and last activity. The preview pane repeats the topic in full under the room's stats, since that is where someone decides whether to join.

### Room Header

`ui.rs::draw_room_header` owns everything between the room rail and the messages, and returns the area left for messages. Each content row pairs live state on the left with the keys or commands that act on it flushed right (`primitives::row_with_hint`): the voice row (`voice::ui::voice_strip_line`, present only for voice-enabled rooms), a dim full-width rule when voice and a topic are both present, the topic row (`TopicRow::Topic`, with a `/rules` hint when the room has rules), and a closing rule that separates the block from the conversation (`RoomHeader.closing_rule`). On the #lounge card, on a day the board has an all-day event, the topic row reads the board's day instead (`TopicRow::BoardDay`, amber, `7 for the board` where the hint sits, the key in the strip's hint style; the text comes from `calendar/live.rs::day_text` through `ChatRenderInput.board_day`). While the live strip is up, the #lounge card drops that row and the closing rule and keeps only the stream and voice rows: the strip stands in for it, and the strip ends in its own rule. A room with neither voice nor a topic keeps the full height for messages, and the whole header yields if it would leave fewer than two rows for them.
- The stream row (`stream_header_line`) fits everything around its watch link, not the other way round: `row_with_hint` drops a hint it cannot fit rather than wrapping it, and here the hint *is* the URL. So the hint is measured first, the title clips to whatever is left, and the ` · N watching` count drops when even that is not enough. A `watch: https://…/live/<id>` plus its load-bearing trailing cell runs 51 columns, which is why stream capability ids are 22-char base64url (`stream/CONTEXT.md` §2), and why the title budget is measured after the hint so the title, not the link, is what gets dropped. The watcher count survives down to 73 columns.
- `/` opens an inline substring filter over room slugs (footer shows the live query); typing edits it, `selected`/`visible_items` track the filtered subset, and `Esc` clears+closes it. While `discover.is_filtering()`, `app::input::handle_byte_event` and `chat::input::handle_byte` route every byte (digits, `space`, `h`/`l`) into the filter so it captures an unrestricted query; arrows still navigate. `start_slash_command_composer` excludes Discover so `/` never starts a slash command there.

---

## 12. Rendering Constraints

In `RENDERING.md`: the rows cache, message and author rendering, images, and the shared layout rules.

## 13. Keybindings

In `COMMANDS.md`, Keybindings: Home chat center, the dashboard lounge, and the synthetic entries.

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

### Drunk Text

A patron deep enough into the tavern's drinks types like it. `ChatService::slurred_body` runs `chat/slur.rs` over the body as the last step of `send_message`, immediately before the row is written.

- **Stored, not rendered.** The slurred text *is* the message body, so IRC, search, replies, and every viewer agree on one version. This is deliberate: the drunk level is decay-based, so a render-time transform would re-evaluate against the reader's *current* level and quietly sober up an old message hours later. The level at the moment of typing is the only one that ever made sense. The cost is that the original is unrecoverable, including for moderation.
- **The patron's own words only.** A line sent as `Wording::Quoted` (a News card, whose title is the page's, not the sharer's) is stored as built and creates no mention notifications; everything a patron types is `Wording::Typed`.
- **Public rooms only** (`room.visibility == "public"`). DMs and private rooms can carry something that genuinely needs reading. The `UserDrinks::find` level lookup only runs where it can matter, and sober users and the ghost bots (who never drink) short-circuit to an unchanged body.
- **Runs last**, so report markers, `contains_link` cooldown, and slow mode all judged the sober text. `create_mentions_task` gets the slurred body so the notification preview matches the room.
- **Readability rests on one rule:** a word's first and last character never move. Only interior letters are reordered (never added or dropped), which is the typoglycemia effect and is why level 4 stays legible at all. Two dials climb per level: what share of words get scrambled (6/32/60/85%) and how far each goes (one swap, one swap, one-or-two swaps, full interior shuffle). Tipsy and buzzed deliberately share a depth: the same fumble, just far more often. The change in *kind* lands at sloshed. Measured over ordinary prose that is roughly 3/21/34/54% of *all* words visibly changed, since short words are ineligible; `each_drink_reads_harder_than_the_last` pins those bands.
- **The hiccup belongs to the top of the ladder.** A single `*hic*` is dropped into an existing gap in 33% of a wasted patron's messages and 10% of a sloshed one's; tipsy and buzzed never hiccup, so the stammer marks the top of the ladder rather than drinking as such. One roll per message at every level: two hiccups in one line is the joke repeating itself. `only_the_top_of_the_ladder_hiccups` pins the bands.
- **Protected tokens are never touched:** `@mentions` (they drive notifications and the mention wash), `#slugs`, URLs, backtick code spans, `---BUG---`-family markers, the leading `> ` reply quote line (someone else's words), the phrases @bartender reads literally (the spending phrases of `../ai/ROUND.md`, and the remember request and "forget me" of `../ai/MEMORY.md`), and anything non-ASCII (so CJK and emoji pass through whole). The `*hic*` only widens an existing gap and respects the same exclusions.
- `slur(body, level, seed)` is pure with a caller-supplied seed; `svc.rs::slur_seed` supplies a fresh one per message. Tests live in `slur_test.rs`.

### Translation

`t` (or auto mode) translates a message; the model call, the shared cache, the render rule and the guardrails are `../ai/CONTEXT.md`, Translation. Chat owns the per-session display state (`ChatState`), the `↳` row, and the invalidation on edit (`ChatService::edit_message` deletes the cached rows inside the edit transaction; `ChatState::forget_translation` drops the session copy on edit and delete).

### History Modal

Scroll-driven room history (`history_modal/`), opened by `/history` or by a search hit/mention older than the loaded tail. Three open modes on `ChatHistoryModalState`: `open_at_tail`, `open_at_message` (anchored, highlighted), and `open_at_unread`. `/history` picks between tail and unread by this session's AFK line for the room (`ChatState::afk_lines`); the cutoff is always that, never `chat_room_members.last_read_at`, which records presence rather than reading. `/history` only *reads* the line and never clears it: opening scrollback is how you go and look at the backlog, not proof you got through it. `ChatService::load_history_unread_task` resolves the exact first unread server-side (`ChatMessage::first_unread_after`, same read-scope/ignore/system filters as history pages, so it can sit further back than the tail's 500) and answers through the anchor pipeline; when nothing qualifies it degrades to a plain tail page under the same request id, which the modal accepts as a tail open. The modal draws the same `new messages` divider as the live tail, before the first loaded message from someone else past the cutoff (`unread_divider_target`); a room with no AFK line gets no divider, mirroring the live tail.

### Summary

`/summary` is the AI room catch-up; the window rules, bounds and guardrails are `../ai/CONTEXT.md`, Summary. Chat parses the command (`parse_summary_arg` + `catch_up_window`), hands over the device mark, and drains `SummaryEvent`s.

### Tail And Delta Recovery

1. Visible-room changes request a tail.
2. Tail checks membership and loads newest 500 messages plus reactions and author metadata. It carries **no** read cursor: `RoomTailLoaded` fans out to every session of the account, so a cursor on it let one device rewrite another's divider.
3. Tail merge dedupes by id, sorts newest-first, truncates to 500, and preserves ignored-user filtering.
4. Broadcast lag requests a visible-room tail reload.
5. Delta sync checks membership and loads up to 256 messages after `(created, id)`.

### Room Membership Commands

1. `/public #room` gets or creates a public topic room and joins only the caller. A new room starts with `auto_join=false`; an existing room keeps its flag, so `/join` on a room a mod filled never unfills it. `/join #room` is an alias for it, parsed in the same `parse_public_room_command` so the two can never drift; it exists because IRC users type `/join` first. Public rooms are hosted, not owned: opening one grants nothing, and a brand-new one posts two plain system-bot lines (deliberately without the system-line prefix, so they render as messages), one in the room asking the creator to describe it and one in #moderators reporting it.
2. `/private #room` opens the room-info form (`app/room_info_modal`) and creates the room with its topic/rules and `created_by` in one go.
3. `/roominfo` opens the same form for the selected room. Authority is decided once in `ChatService::set_room_info`: mods for any room, otherwise the derived owner of a private topic room. `ChatState::room_info_authority` mirrors the rule for what the UI offers (and what the refusal banner says); DMs and game rooms have no info at all. A successful write broadcasts `RoomInfoUpdated`, which banners for the editor and refreshes the room list of every session sitting in that room, so no header waits on the 10s snapshot.
4. `/rules` shows the selected room's rules in the shared overlay (`Overlay`, the same surface `/active` uses), titled `#slug rules`, one entry per stored line: rules are multi-line and a banner is one line. A room with no rules answers with a banner instead of an empty overlay.
5. `/kick @user` runs the moderation service's room action (`ModerationService::room_command` then `room_action`), so membership removal, voice removal, audit log and the target's live session behave exactly as from the mod surface. A private room's owner is granted `Caps::KICK_FROM_ROOM` for that one room via `Permissions::as_room_owner`, which leaves the tier alone so staff stay out of reach. Chat-originated room actions name the room by id (`RoomRef::Id`), never by slug: slugs are only unique per namespace (a topic room and a stream room can share one), so the id is the only exact name of the room the actor is sitting in. The mod surface still resolves its typed slug.
6. `/ban @user [duration] [reason]` and `/unban @user` run the same room action with `RoomModAction::Ban`/`Unban`. Staff act by rank anywhere; a streamer additionally holds the `STREAM_OWNER` grant (kick + ban + unban) inside their own stream room only — the full story, including why ban rather than kick and the voice-ticket refusal, lives in `stream/MODERATION.md`. When the grant comes from ownership rather than rank, an *active* ban placed by another actor refuses both the unban and a re-ban, so a streamer can never lift or soften a staff decision on their room; expired bans are history and do not block.
7. `/invite @user` requires caller membership and rejects DMs.
8. `/leave` rejects permanent rooms.
9. Admin `/fill-room #room` works only for public rooms, bulk-adds all users, and sets `auto_join=true`.
10. DMs always preserve canonical endpoints; sending repairs membership for both endpoints.

### Notifications

1. `send_message` calls `notification_svc.create_mentions_task`.
2. `ChatState` also pushes desktop notifications through its `app/notify` `Notifier` handle for friend joins, DMs, direct mentions, and newly started polls.
3. Render drains `App::notify_outbox` through user settings in root `render.rs`; see Notify in `late-ssh/CONTEXT.md`.

---

## 15. Performance Notes

Scoped loading:
- Username autocomplete is one shared directory watch.
- Per-user snapshots contain summaries only.
- Per-room tails are explicit and capped at 500.
- Discover metadata loads only when Discover is selected.
- Events patch local state and tail loads merge with already-applied live events.

Message search stays cheap by construction: debounce + 3-char minimum + one in-flight latest-wins request per session + `read_permits` + `LIMIT 50` + the migration-114 trigram index. Do not add unbounded result paging; refining the query is the pagination.

Known risks:
- `ChatRowsCache` validity is counter-based (`ChatRowsVersions`); any new mutation path that changes rendered rows must bump the room version or a context epoch, or rows go stale.
- Summary snapshot merge clones preserved message vectors for rooms with empty incoming message lists.
- Unread count SQL counts rows newer than `last_read_at`; if message volume grows, run `EXPLAIN ANALYZE`.
- Tail reload is the recovery path for lagged broadcasts, so keep it bounded and membership-protected.

Never load every room's history per session on a timer.

---

## 16. Tests

Repo-wide rule from the root context still applies:
- Every test, pure or DB-backed, lives beside the file it exercises as `<file>_test.rs`, wired with `#[cfg(test)] mod <file>_test;`. No inline test modules, no crate `tests/` folder.
- Targeted runs go through `make test-llm ARGS="-p late-ssh --lib -E 'test(chat)'"` (memory-capped `cargo nextest` with the check DB). The full sweep is `make check` and stays with the human owner.

Where the coverage sits:
- `svc_test.rs` is the DB-backed service suite (send, reactions, tails, ignores, discover, search scoping, gilds and each gild refusal); `svc_internal_test.rs` covers the pure service helpers (link detection and cooldown tiers, report kinds, poll copy).
- `state_internal_test.rs` reaches `pub(crate)` `ChatState` internals (command parsing, autocomplete ranking, visual order, reply helpers, translation display, inline preview links and YouTube link shapes); `state_test.rs` is a placeholder.
- `commands_test.rs` pins the command registry and popup ranking; `ui_test.rs`/`ui_text_test.rs` the layout, wrapping and hit testing; `slur_test.rs` the drunk-text contract (§14 Drunk Text).
- Each synthetic entry (`news/`, `showcase/`, `work/`, ...) and `../ai/` carries its own `*_test.rs`, and the chat models' tests sit beside them in `late-core/src/models/`.

Test gaps:
- Dedicated notification-service DB-backed tests for mention creation/list/mark-read.
- Direct input-handler tests for News/Showcase/Work/Notifications/Discover.
- Direct `ChatState` synthetic-panel tests.
- Full News process success path is hard to cover because extraction depends on AI/search/network behavior; the body step is tested on its own (`ArticleService::store_body`) for that reason. The X post path is the exception: its parsing is pure, and `news/svc_internal_test.rs` drives it from `OEMBED_HTML`, a byte-for-byte copy of a real `publish.x.com/oembed` response, so the card's text, date, handle, and title are pinned against the actual payload shape rather than a hand-written approximation of it.

---

## 17. Gotchas

- `selected_room_id` is not always the send target. Use `composer_room_id` for active composer submissions.
- `visible_room_id` drives read markers and tail loading.
- Snapshots may contain empty message vectors; empty means preserve existing local tail, not clear history.
- Message storage, recent queries, and tails are newest-first. Delta queries are ascending.
- `(created, id)` is the catch-up cursor.
- Any operation exposing room contents must check membership first.
- DM/private message bodies must not leak to non-members through broadcast handling.
- Ignore filtering covers all rooms including DMs, and also hides bot replies whose `reply_to_user_id` is ignored. DMs with an ignored peer are hidden from the room rail entirely.
- `#announcements` admin-only currently depends on the provided `room_slug`; stale/missing slug is a fragile path.
- Reaction tasks are async; UI should not assume optimistic success.
- A gild marker repaints off the Postgres notify, not off `evt_tx`. If `start_notify_worker` and the process listener are not running (tests, or a process wired without it) the marker only appears on the next room tail load. Do not "fix" that by broadcasting locally as well: two paths would mean two repaints and a marker that behaves differently on the replica that sold it.
- Poll create/vote tasks are async; `ChatEvent::PollUpdated` patches the local active-poll map and `ChatSnapshot.active_polls` refreshes authoritative visibility. Successful poll creation spawns a sleep-until-expiry finalizer that atomically claims the expired poll in Postgres, marks it inactive, and posts compact results into the room as the poll creator. `ChatService::start_poll_finalizer_recovery_task` runs a coarse 10-minute recovery scan for expired active polls so restarts/redeploys do not strand result posts; the DB claim is the cross-replica duplicate guard.
- Poll vote shortcuts use `va/vb/vc` when the selected/visible real room has an active poll, leaving music `v1`..`v5` selectors available.
- Room visual order must stay consistent between state and UI hit-testing/row-building.
- Room-list mouse hit-testing reconstructs a temporary `ChatRoomListView` (`app/input.rs`); room-list layout changes must keep hit tests in sync.
- Chat-scroll mouse hit-testing is driven by `ChatRowsCache` extras (`row_message`, `row_kind`, `header_segments`) and a per-frame `ChatHitLayout` published into `ChatState::last_chat_hit_layout`. If you change how author headers, inline images, or reaction footers contribute rows in `ensure_chat_rows_cache` / `wrap_chat_entry_to_lines`, update both the parallel `row_*` vectors and the segment math in `build_author_prefix_and_segments` so a click still resolves to the right message/segment.
- Showcase and Work posts do not create chat messages. A News share posts one #lounge line as the sharer, which chat draws as the story's card (§11 News).
- Game rooms must remain opt-in and `auto_join=false`.
- Private `kind='game'` rooms (daily match chat) are membership-fixed at creation; no join path may admit a third user, and they stay hidden from the rail/Mentions/IRC like all game rooms. The daily sweeper hard-deletes them 30 days after the match ends.
