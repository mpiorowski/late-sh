# late-ssh Chat Context

## Metadata
- Domain: late.sh SSH chat, synthetic chat entries, and dashboard/room chat surfaces
- Primary audience: LLM agents working in `late-ssh/src/app/chat`
- Last updated: 2026-08-27 (the round: telling @bartender "round for everyone" buys a drink for everyone online but you, 100 chips a head, burned whole. The trigger is a literal phrase from `ROUND_PHRASES` (`late-core/src/models/drink_round.rs`), never a model decision, and `slur.rs` is the other half of it: drunk text is stored rather than rendered, so the phrase is passed through unscrambled (and the `*hic*` kept out of it) or the feature would break for exactly the patrons most likely to use it. Only the buyer is poured into; everyone else gets a `drink_credits` row cashed by ordering, one open per patron, 24h, worth a flat 300 points. §9d The Round.) Previously 2026-08-26 (burn milestones: a permanent Shop glyph that renders after the rented badge and flag in the author label and can never be hidden by either, resolved off `ResolvedName.milestone` like the crown; see `hub/CONTEXT.md` for the catalog side.) Previously 2026-08-26 (the crown: one slot, one holder, one 👑 after their name in every chat author header and on the Clubhouse floor. `/crown` prints who wears it and what taking it costs; `/crown take` buys it at `max(500, ceil(paid x 1.5))`, burned whole, with no hold or cooldown and no self-take. It empties at the UTC month rollover, and the month's last holder keeps the `CRWN` profile award. The glyph rides the `name_flair` map (resolved on the same once-a-second edge as titles and effects) off a process-shared holder that the `crown_changed` Postgres notify keeps in step; the domain is `late-ssh/src/app/crown/`. §9c The Crown.)
- Status: Active
- Parent context: `../../../../CONTEXT.md`

---

## 1. Scope

This file owns chat-specific context that used to make the root `CONTEXT.md` too large.

Included here:
- Home chat rooms, DMs, public/private topic rooms, synthetic entries, and game-backed room chat.
- Home/Dashboard chat center, room rail, and the embedded game-chat surfaces (house tables, daily match boards).
- Message composer, replies, edits, deletes, reactions, ignores, overlays, and autocomplete.
- Synthetic chat entries: RSS, News, Mentions/Notifications, and Discover. Voice is not a synthetic room slot: the dedicated `#voice` room is a real, permanent, public chat room pinned at the bottom of Core (above Discover). Any voice-enabled chat/game room (including `#voice`) renders an embedded voice strip and exposes `/voice`/`/mute` controls while you are inside it. Showcase/Projects and Work/Profiles still use chat-adjacent services/state, but their UI is hosted on Directory page 5.
- Chat service refresh/tail/event contracts, DB model constraints, keybindings, tests, and gotchas.

Global SSH, audio, games, profile, rooms/blackjack, observability, and repo-wide test policy stay in the root context.

---

## 2. File Map

```text
late-ssh/src/app/chat/
|-- mod.rs                       # Module declarations only
|-- action.rs                    # Shared CTCP-style `/me` action encoding/parsing
|-- svc.rs                       # ChatService: DB boundary, snapshots, events, room/message tasks
|-- state.rs                     # ChatState: local UI state, receivers, composer, room/message selection
|-- input.rs                     # Home chat input plus shared message actions used by Dashboard and embedded game chat
|-- ui.rs                        # Home room rail/chat center, dashboard-lounge view, embedded room chat, composer, row cache
|-- ui_text.rs                   # Message/news/reaction wrapping into ratatui Lines
|-- slur.rs                      # Pure drunk-text transform applied to outgoing public-room messages (never touches a round phrase, §9d)
|                                # (translation itself lives in ../ai/translate.rs; chat owns only the key, the display state, and the row)
|-- cyberspace/                  # Cyberspace rail section: personal client for cyberspace.online, incl. their chat (cIRC)
|-- discover/                    # Synthetic Discover entry: public rooms not yet joined
|-- feeds/                       # Synthetic RSS entry: private per-user RSS/Atom inbox
|-- gild/                        # Gild tier picker: state/input/ui for the `g` message action
|-- news/                        # Synthetic News entry: articles + #lounge announcement
|-- notifications/               # Synthetic Mentions entry: mention notifications
|-- polls/                       # /poll modal state/input/UI
|-- showcase/                    # Projects service/state/UI reused by Directory page 5
`-- work/                        # Profiles service/state/UI reused by Directory page 5
```

Related tests:

```text
late-ssh/src/app/chat/           # adjacent _test.rs files, wired with #[cfg(test)] mod
|-- svc_test.rs                  # Broad ChatService DB-backed coverage
|-- sheet_test.rs                # Character-sheet model/service coverage
|-- state_test.rs                # Placeholder; direct ChatState tests need more accessors
|-- cyberspace/svc_test.rs       # CyberspaceService DB-backed coverage (dead base URL, no network)
|-- news/svc_test.rs             # ArticleService DB-backed coverage
|-- showcase/svc_test.rs         # ShowcaseService DB-backed coverage
`-- work/svc_test.rs             # WorkService DB-backed coverage
late-ssh/src/app/announcements_test.rs   # Login #announcements loading/read-cursor behavior
```

Core models used by chat live in `late-core/src/models/`:
`chat_room.rs`, `chat_room_member.rs`, `chat_message.rs`, `chat_message_reaction.rs`,
`chat_message_gild.rs`, `crown.rs`,
`notification.rs`, `rss_feed.rs`, `rss_entry.rs`, `article.rs`, `article_feed_read.rs`, `cyberspace_account.rs`, `showcase.rs`,
`showcase_feed_read.rs`, `work_profile.rs`, `work_feed_read.rs`, and `chat_poll.rs`.
Chat-owned moderation commands also use `room_ban.rs`,
`chat_slow_mode.rs`, `server_ban.rs`, `artboard_ban.rs`, and `moderation_audit_log.rs`.

---

## 3. Ownership Split

- `svc.rs` is the async boundary between TUI state, DB models, mention notifications, and broadcast/watch channels.
- `state.rs` owns local chat data, room/message selection, composer state, reply/edit/reaction state, overlays, synthetic-entry substates, unread/read tracking, and cache inputs.
- `input.rs` maps Home chat keys to state/service actions. `handle_message_action_in_room` is shared by Home chat and the embedded game-chat panes.
- `ui.rs` renders Home room rail/chat center surfaces and owns `ChatRowsCache`.
- `ui_text.rs` centralizes wrapping for normal messages, the small Markdown subset, reply quotes, `---NEWS---` cards, and reaction footers.

Keep `mod.rs` declaration-only; no `pub use` re-export layer.

---

## 4. Service And Data Flow

`ChatService` channels:
- Per-session `watch<ChatSnapshot>` for low-frequency room summary data.
- `broadcast<ChatEvent>` for live message, reaction, room-command, and error events shared by every session. Single-recipient payload events (`RoomTailLoaded`, `DeltaSynced`, `MessageSearchLoaded`, `MessageContextLoaded`, `DiscoverRoomsLoaded`, and their `*Failed` twins) do NOT ride the broadcast: `ChatService::send_user_event` delivers them point-to-point over a per-session `mpsc<ChatEvent>` registered in `refresh_sessions` (returned by `start_user_refresh_task`), so a 500-message tail is never cloned into every connected session. `ChatState::drain_events` drains the targeted channel ahead of the broadcast; both feed the same event match.
- Shared `watch<Arc<Vec<String>>>` username list for mention autocomplete, rebuilt every 30s from the in-memory `UsernameDirectory`, not the DB. It used to re-scan all of `users` on that timer, which was 1.9% of all DB execution time; the directory is written through on login/profile save/rename/delete, so this is both free and fresher. Do not point it back at `User::list_all_usernames` (that arm is the no-directory fallback for tests).
- Plain username display is centralized outside Chat in `State.username_directory` (`Uuid -> username`), loaded at startup, refreshed every 30 minutes, and updated on login/profile save/mod rename/account delete. Chat still owns richer author metadata such as bonsai glyphs, countries, badges, reactions, and unread state.
- A service-owned refresh scheduler that refreshes registered sessions every 10s and on explicit signals.
- `read_permits: Semaphore(8)` to cap concurrent snapshot, tail, and discover reads.
- `send_lounge_message_task` is the shared internal producer for custom `#lounge` announcements. It resolves `#lounge`, optionally joins the author first, then sends through the normal `send_message` path. News uses it with a request id so normal composer-style send success/failure events are preserved.

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
   - Rooms, unread counts, and latest-message timestamps come back from **one** query (`list_for_user_with_state`), because all three key off the same `chat_room_members` row. Measured on prod against the heaviest user (157 rooms): 1,452 buffers / 5.2 ms against 1,760 buffers / 8.1 ms for the three separate queries it replaced, plus two fewer planning cycles. There is deliberately no separate `unread_counts_for_user` or `last_message_at_for_rooms` any more; a second copy of that SQL is a drift hazard.
   - Unread counts are capped at `ChatRoomMember::UNREAD_COUNT_CAP` (100) and the UI renders anything at the cap as `99+` (`ui.rs::format_unread_badge`). Uncapped, a never-opened `#lounge` counted all 127k messages per user per pass and was 43% of all DB execution time. Room ordering only tests `unread > 0`, so the cap is invisible to it. Do not restore an exact count without re-reading SCALE.md Pain Point 7.
   - The `· ` activity lines are excluded by comparing the author id against the system bot id that `ChatService::set_system_user_id` receives from the #lounge feed task at startup. Do not go back to reading `users.settings->>'system'` per message: that made Postgres hash the whole users table per query.
3. Snapshots intentionally carry empty message vectors. They do not load history; activity timestamps are summary metadata used for stable room ordering.
4. Visible-room changes call `App::sync_visible_chat_room()`, which stores `visible_room_id`, marks the room read, and requests a room tail.
5. `load_room_tail_task` fetches the newest 500 messages, reaction summaries, author usernames, and author bonsai glyphs. Render-time display names prefer the app-wide username directory over this per-session chat cache when both know the same UUID.
6. Broadcast `MessageCreated`/`MessageEdited`/`MessageDeleted`/reaction events patch local state; tail/search/discover results arrive on the per-session targeted channel. Broadcast lag triggers a tail reload for the visible room (the targeted mpsc is unbounded and cannot lag).

Room tails deliberately carry **no** read cursor. Render inserts one synthetic `new messages` divider before the first message from someone else past this session's **AFK line** (`ChatState::afk_lines`), never past `chat_room_members.last_read_at`; see `The Two Marks` below for why. The divider is render-only state in the chat row cache; do not persist it or count it as a chat message.

### The Two Marks

Two marks, two questions, and they never feed each other. Keeping them apart is the whole design.

| | AFK line | left-app mark |
|---|---|---|
| Question | where did I stop reading this room, this session | when was I last here, on this device |
| Drives | the `new messages` divider, `/history`'s open position | the bare `/summary` window |
| Scope | one SSH session, one room, in memory | one SSH key, in `user_ssh_keys.left_at` |
| Set by | `AFK_LINE_IDLE` (10 min) of keyboard silence on the room on screen | session end, stamped at the moment the keyboard went quiet |
| Cleared by | your own message landing in the room | taken (nulled) by the next session on that key |

**Neither is a read cursor, because reading cannot be observed.** Reading a room produces no input at all, so no signal the server can see distinguishes a person reading in silence from a person who walked away. Both marks are about something the reader *did* (went quiet, left, spoke), not about what they took in.

**The AFK line** (`ChatState::afk_lines`, `sync_afk_line`, `clear_afk_line`). `App::tick` mirrors `last_input_at.elapsed()` into chat each tick, the same way `viewer_tz` is mirrored, because chat is what knows which room is on screen. Past `AFK_LINE_IDLE` the visible room gets a line stamped at `now - idle`, the moment the keyboard went quiet rather than the moment we noticed. One sentence governs it: **the line marks what you missed, from when you went quiet until you speak again.** Everything else follows from that sentence, and anything the sentence does not predict is a bug.

- **Only the visible room collects one.** A room you are not looking at was never being attended, and its rail badge already says what is waiting.
- **A room that has a line keeps the one it has.** The line says when you left; staying away longer does not move when you left.
- **Placement is keystroke silence, clearing is a post.** The asymmetry is deliberate: most people read for hours without posting, so "hasn't posted" would carpet the room in lines, while a post is unambiguous *and* room-scoped in a way a keystroke is not. A silent reader does collect a line they did not need; that costs one glance, and it is the direction to be wrong in, since the failure it replaced silently swallowed whole days.
- **Returning does not clear it.** Clearing on the keystroke that brings you back destroys the line at the exact moment it is wanted. The one clear is `push_message` seeing a message you authored (one place to look, and the authoritative one: several submit paths send, `/me`, `/roll`, `/cup`, a plain line, and they all land there).
- **`/history` reads the line and never spends it.** Opening scrollback is how you go and look at the backlog, not proof you got through it, and the anchor has to still be there when the modal closes.
- **`/summary` does not read it at all.** The catch-up is the other mark's job.
- **Never leaves the process.** "Did the person at this terminal step away" is a fact about this terminal; your phone being idle says nothing about your desktop. Keyboard-only is an approximation: `?1003h` any-event mouse tracking is on, so pointer motion over the terminal also counts as input.

**The left-app mark** (`user_ssh_keys.left_at`, migration 166; `UserSshKey::set_left_at` / `take_left_at`; `ChatState::device_left_at`). `Drop for App` writes it through `ProfileService::set_key_left_at`, stamped at `now - last_input_at.elapsed()`: the moment the keyboard went quiet, not the moment the connection closed, so a terminal parked open overnight and shut in the morning left last night. `session_bootstrap::load_device_state` **takes** it at the next boot on that key (an `UPDATE ... RETURNING` that nulls the column), `SessionConfig::key_left_at` carries it into `ChatSession::device_left_at`, and it stays fixed for the session.

- **It is the only input to a bare `/summary`.** "What happened since I was last here" is a question about this device, not about which room happened to be on screen or whether the keyboard went quiet ten minutes ago. The room's AFK line is never consulted (`catch_up_window`), and asking twice in one session reads the same stretch twice, plus whatever landed since; the cooldown bounds that.
- **Per device, never per account.** The cost is that a room you read on the phone all evening is summarized again on the desktop next morning. That is what "since I was last here" means, and it is accepted over an account-wide cursor that could not tell reading from presence. One key on two machines reads as one device; last write wins.
- **Taken, not read.** The drop write is fire-and-forget, and a session whose write never landed (process killed mid-drain, DB blip, key re-pointed by account linking) must not hand the next session a leave from *before* itself, which would summarize messages already read. A lost write degrades to the default window, never to a wrong one.
- **Never seeds a divider.** Reconnecting draws no `new messages` line; the divider is about this session, and this session has not stepped away yet. Keyless sessions (ghost bots, tests) have no device to remember anything on.

**What this replaced, and why not to put it back.** `ChatState::room_unread_markers` used to hold the `last_read_at` that came back on each `RoomTailLoaded`, and the divider drew against it. Three defects came from that one arrangement, and all three are gone with it:

1. **The divider froze.** The marker was only ever recomputed on a tail load, so once placed it never moved. Messages piled below it for hours, your own replies included, and it cleared only by leaving the room and coming back.
2. **Sessions rewrote each other's dividers.** `send_user_event` delivers `RoomTailLoaded` to *every* session of the account, so opening the room on a phone planted that phone's pre-mark cursor as a divider on an idle desktop that was fully caught up.
3. **A debounced race.** `request_list` fires `flush_pending_read_cursors()` and `request_room_tail()` as independent `tokio::spawn`s, so a tail could read `last_read_at` before the mark that preceded it committed, and a message from the last two seconds would read as unread.

Two more arrangements were tried and removed in the same pass. A per-account `/summary` watermark (`chat_summary_reads`, advanced on delivery) answered "what have you been told" correctly but repeated nothing across devices at the price of a table, a second set of rules, and re-summarizing nothing you had read on another device even when you wanted it. Then a single unified mark, where `left_at` seeded the AFK line on reconnect and `/summary` read the AFK line: it collapsed two questions into one cursor, so posting in a room (which clears the divider) also silently reset the catch-up window to the 24h default, and a room never opened here inherited a divider it had no business drawing. The two marks are separate because their clears are different acts.

`chat_room_members.last_read_at` still exists and is still written on presence (`mark_room_read`). It drives rail unread badges and nothing else. Do not make it load-bearing for a divider or a summary window again: it records that a session had a room on screen, which is not reading.

System-feed lines: the `#lounge` activity feed (`app/activity/lounge.rs`) posts persisted messages authored by the `system` bot user (`SYSTEM_USERNAME`) with the `· ` body prefix. A message counts as a system line only when BOTH hold — author is the feed bot and the body parses via `ui_text.rs::parse_system_line` — so neither a human squatting a nick nor a pasted `· ` can spoof it (`state.rs::system_line_text_in`, `ui.rs::is_system_author`, both via `activity/lounge.rs::is_system_username`). The TUI never stores system lines as chat rows: every ingestion point (`push_message`, `merge_room_tail`, snapshot `filter_messages`, with a `note_activity_ticker_from` scan on tails/snapshots) diverts them into `ChatState::activity_ticker`, a newest-first queue of `ActivityTickerEntry` (id/text/at) deduped by message id and capped at `ACTIVITY_TICKER_CAP` (10). The queue renders as the one-row activity ticker (`ui.rs::draw_activity_ticker`) in the composer-gap row of both Home chat surfaces (`draw_dashboard_chat_card` and `draw_chat_center`): events pack left to right, newest first, each as dim italic text plus a faint compact stamp (`format_relative_time_short`: `now`/`5m`/`3h`), `·`-separated, until the row is full; events that don't fit are simply not shown (the cap exists to outfill any sane width), and the newest event always shows (truncated if it must). The gap row always exists, so chrome never moves. Because they never enter room message lists, system lines are not selectable/reactable/replyable in the TUI, cannot trip the unread divider, and scrollback skips them; they remain excluded from unread counts at the SQL layer (`ChatRoom::list_for_user_with_state` skips a `settings.system` author *whose body carries the prefix*, so ordinary system messages such as the new-public-room report to #moderators still light a badge), and their bodies never contain `@` so no mentions fire. IRC still projects every line as an ordinary PRIVMSG from the `system` nick, and #lounge history keeps them all. (The legacy authorless-row renderer — `wrap_system_to_lines`, `prev_was_system` stacking in `ensure_chat_rows_cache` — is now unreachable in practice since ingestion diverts every system line before it can enter a room list; it is kept deliberately as the fallback that makes the ticker experiment a one-site revert. Remove it if the ticker sticks.)

`ChatSnapshot` is summary data. `RoomTailLoaded` is history data. Do not merge those responsibilities back together.

Login announcements:
- `app::announcements::load_login_announcements` runs during SSH session bootstrap, outside `ChatState`.
- If public `#announcements` exists, the user is idempotently joined and up to 10 oldest unread messages from other users are loaded from `chat_messages` without marking them read. Dismissing the modal advances `chat_room_members.last_read_at` to `latest_displayed_at()`.
- The resulting modal is stored on `App`, appears only after splash/settings are gone, consumes input while visible, scrolls with j/k, and closes on Enter/Esc/q.

---

## 5. DB Contracts

Room model:
- `chat_rooms.kind`: `lounge`, `language`, `dm`, `topic`, `game`.
- `chat_rooms.visibility`: `public`, `private`, `dm`.
- `lounge` must have slug `lounge`, is public, auto-join, and permanent.
- `language` rooms are public, opt-in, unique by `language_code`, with slug `lang-{code}`.
- `topic` rooms are unique by `(visibility, slug)`.
- `chat_rooms.topic` / `rules` are the room's "about" info, both nullable (NULL = unset; blanks are stored as NULL by `ChatRoom::set_topic_and_rules`). `topic` is projected as the IRC topic, shown in the Discover list, and shown in the room header; `rules` are read on request with `/rules`.
- `chat_rooms.created_by` is who opened the room, written by the create paths only and never back-filled (NULL for system rooms, DMs, and everything predating migration 125). Ownership in force is **derived**, not stored: `ChatRoom::owner_id` / `owner_ids_for_rooms` return the creator while they are still a member, else the earliest remaining member (ties by user id), so a creator who leaves hands the room on with no write. Only private `topic` rooms are owned; the chat snapshot carries `room_owner_ids` for them alone.
- `game` rooms require `game_kind + slug`, are unique by `(game_kind, slug)`, and DB constraints require `auto_join = false`. Two flavors: permanent public house-table rooms (slugs `poker`/`blackjack`/`maze`/`tron`, seeded at startup by `HouseTableRegistry::ensure_chat_rooms`), and private two-player daily match chats (slug `daily-{match_id}`, created in the daily claim transaction with both memberships — see `late-ssh/src/app/lobby/daily/CONTEXT.md`). `ChatService::join_game_room` joins public game rooms freely but rejects non-members for private ones (`this match chat is players only`); daily players are already members, so their "join" is only the idempotent re-join that triggers the list/tail refresh chain.
- DMs canonicalize endpoint UUIDs by text order and are unique by `(dm_user_a, dm_user_b)`.

Membership:
- `chat_room_members` primary key is `(room_id, user_id)`.
- `last_read_at` drives unread counts **and nothing else**. It is a presence cursor (`mark_room_read` fires whenever a message lands in a visible room); it is not what the `new messages` divider or the `/summary` window rest on. See `The Two Marks`.
- Unread counts exclude messages authored by the current user.
- `join` is idempotent and preserves original `joined_at` on conflict.
- Membership is the authorization check for reading tails, syncing deltas, marking read, sending, reacting, listing members, and inviting.

Messages:
- `chat_messages.body` must be trimmed non-empty and length <= 2000.
- Messages are hard-deleted. There are no tombstones.
- Recent/tail queries return newest-first: `ORDER BY created DESC, id DESC`.
- Delta queries return ascending after `(created, id)` and are inserted into newest-first local state.
- `reply_to_message_id` is nullable and uses `ON DELETE SET NULL`.
- `reply_to_user_id` is nullable and uses `ON DELETE SET NULL`. It records the user a bot/automated reply is responding to, used to filter such replies for viewers who ignore that user. Set only by bot sends.

Translations:
- `message_translations` is the shared translation cache, primary key `(message_id, target_lang)`, `ON DELETE CASCADE` from the message. Migration 136; migration 137 adds `same_language` (a cached "already in this language" verdict whose `body` keeps the judged text, rendered as nothing); migration 138 adds `author_shared` (the author's opt-in wrote this row, so every session reading the target displays it; the upsert ORs the flag so a reader's private rewrite never un-shares a row).
- Rows are written only by `TranslationService` after a successful model call, and deleted by `ChatMessage` edits (inside the edit transaction) and the FK cascade. Nothing else may write them: a stale row is a translation of text that no longer exists.
- `target_lang` is the `TranslateLang` key (`en`, `zh-hans`, `ko`, `ja`, `es`, `fr`, `pt`, `de`, `it`, `pl`, `ru`, `uk`, `tr`, `vi`, `id`, `th`, `hi`); adding a language is a new enum variant, and its cache rows are independent of every other language's.

Slow modes:
- `chat_slow_modes` is a per-user throttle, not a ban. `room_id` set means room-scoped; `room_id NULL` means server-scoped. Unique indexes enforce one row per `(room_id, target_user_id)` for room scope and one server row per target. Rows store `interval_secs`, nullable `expires_at` (`NULL` = permanent), actor, and reason.
- Enforcement happens in `ChatService::send_message` after membership/room-ban checks and before insert. Room-slow is checked first; server-slow applies to non-DM chat rooms only, so DMs are not throttled. Admin sends bypass the throttle; moderators are not inherently exempt unless they are admins.
- A slowed user keeps room membership. Early sends are rejected privately with a `Slow mode in #room: wait ...` banner; messages are not queued.
- `/mod slow <server|#room> @user <interval> <duration|permanent> [reason...]` applies it, `/mod unslow <server|#room> @user [reason...]` removes it, and `/mod view slows [server|#room] [page]` lists active slow modes. Applying/removing slow mode uses targeted session toasts and writes moderation audit actions `room_slow` / `room_unslow` or `server_slow` / `server_unslow`.

Gilds:
- `chat_message_gilds` is append-only: no update path, no un-gild, `created` and no `updated`.
- Unique on `(message_id, user_id)`: one slot per buyer per message. A higher tier from the same buyer raises the row in place (`ChatMessageGild::place_in_tx`, a closed `GildPlacement`: `Placed` / `Upgraded` / `SameTier` / `HeldHigher`), at the new tier's full price; the same or a lower tier is refused. So `count` is distinct buyers by construction.
- `author_user_id` is denormalized off `chat_messages` so `chat_message_gilds_no_self_gild CHECK (user_id <> author_user_id)` can exist at all, and so the profile count reads one owner-scoped query.
- `chips` records what was paid at purchase time; a later reprice never rewrites history.
- `ON DELETE CASCADE` from both the message and the users.

Reactions:
- `chat_message_reactions` primary key is `(message_id, user_id)`.
- Each user has at most one icon-picker reaction per message.
- Message/user deletion cascades remove reactions.

Notifications:
- Mentions are stored in `notifications`.
- Mention unread state clears two ways, and **either** is enough: the global `mention_feed_reads` watermark (opening the Mentions entry) or the mention row's own `read_at`, stamped when the message it rides on is rendered in its room. `ChatState::flush_rendered_mention_reads` collects mentions of the current user among the *loaded* messages of each room being marked read (on the same debounced flush as the read cursors) and sends them through `NotificationService::mark_read_for_messages_task`, which stamps `notifications.read_at` and republishes the unread count in the same task after the update commits, so the rail badge clears live. The room's coarse `chat_room_members.last_read_at` cursor deliberately plays no part: it moves whenever a room is merely opened (including the auto-selected room at connect), which would clear mentions above the loaded tail that were never on screen; those stay unread. `list_for_user` returns `read_at` so the list's unread dot applies the same rule (still against the marker frozen on entry, so the dots survive the visit that read them). (`MentionFeedRead::unread_count_for_user` is an older single-cursor copy of the count with no production callers.)
- Mention resolution excludes the actor and recipients who ignore the actor; DMs only notify DM participants, private rooms only members, and non-game public rooms may mention any user. Game-room chat does not create Mentions feed notifications.

---

## 6. Rooms And Selection

`RoomSlot` represents either a real room or one of the Home synthetic entries: RSS (`RoomSlot::Feeds`), News, Notifications/Mentions, or Discover. `RoomSlot::Showcase` and `RoomSlot::Work` remain in code for state compatibility and focused helpers, but they are no longer emitted by Home visual order, room rail, or room jump. The `#voice` room is a normal `RoomSlot::Room` (a permanent public room, pinned at the bottom of Core directly above Discover in both `state.rs::visual_order_for_rooms` and the `ui.rs` hit-test mirror), matched by slug `voice`; being permanent is what keeps it in Core rather than sorting into Channels (which excludes slug `voice`). Voice-enabled rooms additionally render an embedded voice strip when open.

Visual order is defined in `state.rs::visual_order_for_rooms` and mirrored by cozy room-rail rendering in `ui.rs`. The base navigation order is:
1. Favorite real rooms in `users.settings.favorite_room_ids` order.
2. Core permanent rooms plus synthetic updates: `lounge`, `announcements`, `suggestions`, `bugs`, Notifications/Mentions, News, RSS when available, the permanent `#voice` room (matched by slug, directly above Discover), and Discover / `+ browse rooms` last. Collapsing Core hides these synthetic update entries too (Discover included). A `#voice` room that is not permanent shows nowhere: Core requires `permanent` and Channels excludes slug `voice`, so promote it with `/create-room voice`.
2b. The `stream` section (`RoomSection::Stream`, shortcut `s`), directly under Core and above Cyberspace/Channels: one `▶ {user}-live · title · N watching` row per registered "watch me" stream, fed by `ChatState::live_streams` (copied from the stream registry watch in `App::tick_stream`). The section exists only while somebody is streaming. Stream rooms are `kind='game'` so they can never leak into Channels; opening one the user never joined triggers the lazy public game-room join from `select_room_slot`. The stream header block (title, watcher count, watch-URL nudge), the `▶LIVE` author presence badge, and the ON AIR voice-strip state ride the same copy; the domain contract is `late-ssh/src/app/stream/CONTEXT.md`.
3. Unread DMs, under an `unread dms` header. At the bottom of the rail DMs were going unnoticed, so any DM with unread messages is promoted here, sorted the same way as the DMs section below. Three rules keep it stable: favorited DMs stay in Favorites (they are already in `pushed_rooms` when the group is built), an ignored peer's DM is promoted nowhere, and the group ignores the DMs collapse toggle, which makes collapsing DMs a way to fold the read ones away without losing the ones waiting on a reply. The header is plain text like the `bumped` strip: no collapse toggle, no `RoomSection` variant, no section shortcut.
4. Other non-DM chat-list rooms/channels, excluding favorites.
5. DMs, sorted by unread status, then snapshot latest-message activity, then peer display name. Do not derive this order from lazily loaded room tails.

Reading a DM zeroes its unread count on the same frame (`mark_room_read`), which would drop it out of the promoted group with the cursor still on it. `ChatState::sticky_unread_dm` holds exactly one DM in the group while it is being read: `note_sticky_unread_dm` claims it when an unread DM is marked read, keeps it while that same room is re-marked (every message landing in a visible room does that), and releases it when any other room is read, so it falls back into the DMs section as soon as you open something else. Reading is the only release: `sync_visible_chat_room` marks a room read only when one is visible, so leaving Home for a screen with no chat room (the Arcade, Games) parks the DM in the promoted group, badge-less, until the next room is opened. The decision is the pure `next_sticky_unread_dm`; the promotion predicate `dm_is_promoted_unread` is shared by `visual_order_for_rooms` and the rail so the two mirrors cannot disagree.

`RoomSection` is the closed roster of collapsible rail sections: Favorites, Core, Stream, Cyberspace, Channels, Dms. Each one is rendered, foldable from its header, and reachable by its `shortcut` key; a variant that no section header draws is dead weight, so add one only alongside the header that renders it. The Showcase/Work feeds moved to Directory page 5 and took the old `Updates` section with them.

Hub Shop room effects add render-time top sections in the cozy room rail. Active `room_bump` effects on non-permanent public topic rooms render first under a dedicated `bumped` section as plain synthetic `join #slug` text rows; the synthetic row never shows glow/spark/pulse/bump suffixes. The real room stays in its normal navigation section if the viewer has it, and pressing Enter on the synthetic row joins/moves through the existing public-room join path. `room_spark`, `room_glow`, and `room_pulse` are one-minute page-level visuals over the selected room content; they must not add top text, promote rooms, or restyle room-list rows. No effect changes real room-list text or color: Hack Room (`pinned_vibe`, the one that did) was retired by migration 152 and its rail styling removed. Active effects flow through `ChatRoomListView.active_room_effects`. Hit testing uses the same visual slot list, so bumped room clicks stay aligned with rendering.

RSS:
- RSS subscriptions are per-user and managed in `Settings -> RSS`.
- `rss_feeds` stores connected RSS/Atom URLs; `rss_entries` stores private pending entries.
- The background `FeedService` polls active feeds, parses a conservative RSS/Atom subset, stores unseen entries, and publishes per-user events.
- Feed URLs are user-supplied, so fetches go through the SSRF-guarded downloader (`files::image_upload::download_url_bytes_following_redirects`): private/link-local/reserved resolved IPs rejected, DNS pinned, every redirect hop re-validated (up to 5 hops; feeds legitimately redirect), 1 MB body cap. Do not swap in a plain `reqwest::Client`.
- The visible entry list is capped per feed (`PER_FEED_ENTRY_LIMIT`, 20) inside the flat `ENTRY_LIMIT` (100) window via `RssEntry::list_visible_for_user`, so a high-volume feed (news site, ~20 posts/day) cannot evict weekly/monthly feeds from the inbox.
- The RSS synthetic room (`RoomSlot::Feeds`) is private. Press `s` on an entry to share it through `ArticleService::process_url`; only then does it become a public News article and `#lounge` announcement, and only then does it pay the share reward (see §11 News).
- Enter copies the selected RSS entry URL, `d` dismisses it, and `r` asks the RSS poller to refresh.

Game rooms stay in `ChatState.rooms` for the embedded game-chat panes, but `is_chat_list_room` hides them from the Home room rail/navigation and favorite-room picker.

Room navigation:
- `h`/`l`, left/right arrows, `Ctrl+P`/`Ctrl+N` switch room selection.
- `Space` activates room-jump mode, assigning keys from `ROOM_JUMP_KEYS`. Jumping to the already selected room/synthetic entry still re-runs the entry's read/list side effects so stale unread badges clear.
- Global `Ctrl+/` opens the room jump modal. Rows include unread counts and synthetic entries for RSS, News, Mentions, and custom room browse. Showcase/Projects and Work/Profiles live on Directory page 5 instead. Results are ordered favorites first, then unread entries, then latest message/activity; typed `@` and `#` prefixes filter to DMs or rooms while keeping that ordering.
- A leading `?` flips the same modal into message search (`app/room_search_modal`): `?query` searches every joined room, `?#room query` scopes to one room, `?@user query` to one DM (scope tokens resolve against joined rooms only; an unresolved scope never fires). Searches are debounced 300ms, need `SEARCH_MIN_CHARS` (3) chars, run one-at-a-time latest-wins by request id through `ChatService::search_messages_task` (`read_permits`-gated, `SEARCH_RESULTS_LIMIT` 50 newest hits). The SQL (`ChatMessage::search_for_user`) is a LIKE-escaped ILIKE join on `chat_room_members` (membership is the auth boundary), excluding game rooms, system-feed authors, and the caller's ignored users; migration 114 adds the `pg_trgm` GIN index that makes it indexed. Results live in `ChatState.message_search` (snippets precomputed around the first match at drain time); the modal renders hits plus a fixed-height detail pane showing the full body with a context window of up to 4 messages either side (dim `author: body` rows around the highlighted hit). Context loads lazily per selected hit through `ChatService::load_message_context_task` (`ChatMessage::list_around`: `(created, id)` cursors both directions, membership checked, system-feed and ignored authors excluded), cached by message id with a single in-flight slot so fast scrolling converges instead of fanning out; a failed fetch caches an empty window rather than refiring every tick. The pane uses fixed slots, so the hit row never moves while context fills in. Enter lands in the hit's room and selects the message if it is in the loaded tail, else registers `pending_search_jump`, resolved (or dropped with a banner) when the tail lands. `Ctrl+Y` copies the selected hit. `/search [query]` opens the modal pre-filled with `?query`.
- While composing on Home, `Ctrl+N`/`Ctrl+P` switch real rooms while preserving draft text and dropping reply/edit state.
- Synthetic entries are selected with booleans (`news_selected`, `notifications_selected`, `discover_selected`, `showcase_selected`, `work_selected`), not `selected_room_id`.

---

## 7. Home Shell And Embedded Chat

There is no top-level `Screen::Chat`. `Screen::Dashboard` renders as Home and owns both the room rail and the chat center:
- If `chat.selected_room_id` is `#lounge` and no synthetic entry is selected, the center renders `chat::ui::draw_dashboard_chat_card`: the lounge chat card, full height.
- If any other real room or synthetic entry is selected, the center renders `chat::ui::draw_chat_center`.
- On wide terminals, `chat::ui::draw_room_list_rail` renders a borderless left rail. On narrow terminals, the center owns the available width.

Room favorites:
- Press `f` on a selected real room to toggle it in `ProfileState::toggle_favorite_room`.
- Press `[` / `]` on a selected favorite to move it up/down via `ProfileState::move_favorite_room`. No-op when the selection isn't a favorite or is already at the edge.
- Favorites are stored in `users.settings.favorite_room_ids` and the vec order drives both the Home room rail and the global picker.
- Favorites are no longer edited through a Settings tab.
- Active Shop room highlights are not favorites; they temporarily render above favorites and expire from `shop_consumable_effects`.

Home presence:
- The top activity/multiplayer/quest strip was removed; presence (online count + connected friends) lives in the right sidebar's pinned core block, and the public activity feed ships into #lounge as system messages (`app/activity/lounge.rs`; the sidebar Activity panel is retired) surfaced in the TUI as the one-row activity ticker above the composer, never as chat rows. The `b1`-`b4` recent-room jump keys died with the Rooms demolition.

`App::sync_visible_chat_room()` is the read/tail-load bridge. It computes the visible chat room from the current screen (Home/Dashboard, house table, daily board, Clubhouse), stores it in `ChatState`, marks it read, and requests a tail on change. Call it after screen, selected room/synthetic entry, room favorite, or open-surface changes.

There are separate `ChatRowsCache` instances on `App` for:
- Home lounge dashboard chat.
- Home chat center for the selected real room/synthetic entry.
- Embedded game chat (house tables, daily match boards).

Do not share a row cache across surfaces unless width and visible messages are guaranteed identical.

---

## 8. Composer, Commands, Reply, Edit

The main composer is a `ratatui_textarea::TextArea<'static>`.

`composer_room_id` is the authoritative send target while composing. This matters because Home and the embedded game surfaces do not necessarily drive `selected_room_id` in the same way.

`/me <action>` stores a CTCP-style action body through `chat/action.rs` and renders locally as italic `* name action`; IRC delivery unwraps it into the same readable action text. Keep new action handling on the shared helpers so TUI and IRC stay aligned.

`/gift @user <chips>` transfers chips through `ChipService` and `late-core::models::chips::UserChips::transfer_gift`. The transfer is one transaction: sender debit, recipient credit, two ledger rows, and chip notifications. It enforces the chip floor, rejects self-gifts, caps gift size, and applies a short per-sender cooldown in `ChatService`.

`/members` renders a styled overlay with online members first, offline members second, each group sorted alphabetically. Preserve the fixed status-cell shape so overlay rows do not jump as online state changes.

Directory page 5 uses the Work/Profiles and Showcase/Projects substates from chat. Its local `directory::state` search mode is independent of Home room search: `s` opens a case-insensitive substring search on Profiles or Projects, arrows move the filtered selection, `Enter` selects the underlying Work/Showcase item, and `Esc` exits search.

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
- Edit calls `edit_message_task`. There is no edited flag: a message counts as edited when `updated > created`, which appends `(edited)` to the author header's stamp. Because of that, an edited message never groups as a continuation under the message above it (`ensure_chat_rows_cache`): it takes its own author header back so the marker has somewhere to live. Editing the second message in a run visibly breaks the run; that is the intent.
- An image upload started from the composer carries its reply target with it (`PendingUrlUpload`/`PendingClipboardImageUpload`, then `image_upload_reply_target`), because both `/upload` and `/paste-image` clear the composer at submit and the finished URL reopens it through `start_composing_in_room`. `tick.rs` restores the target after reopening, and drops it on a failed upload.
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
- `/cs` (alias `/cyberspace`) opens the Cyberspace `feeds` entry; `/cs post` opens its compose modal, `/cs chat` (alias `/cs rooms`) the chat-room picker that adds rooms as rail entries, `/cs mail` the C-Mail picker that pins conversations the same way, `/cs mail @user` starts (or finds) a conversation, pins it, and walks into it, `/cs link` the account-link modal, `/cs unlink` forgets the link. Parsed in `submit_composer` (`parse_cyberspace_command`), handled inline on `ChatState` (no `take_requested_*` plumbing; `pending_chat_screen_switch` pulls the user to Home).
- `/aquarium` (alias `/aq`) toggles the Shop-unlocked aquarium tray shown only in the Home Lounge view (carved from the top of the lounge chat column); `/aquarium feed` feeds it. Parsed in `submit_composer`, drained via `take_requested_aquarium_command` in `handle_post_submit_requests`.
- `/pet` toggles the pet strip (same `show_pet_strip` setting as the settings tweak); `/pet feed` and `/pet water` care for the Pet Companion (same strip actions as clicking the bowls/pet; the pet and the food bowl are both feed targets). The strip renders only in the Home Lounge view. Parsed in `submit_composer`, drained via `take_requested_pet_command`.
- `/dm @user` opens/creates a DM.
- `/exit` opens quit confirm.
- `/golive [title]` registers this user's "watch me" stream (`/golive stop` ends it) and `/watch @user` opens a live stream. Both are parsed in `submit_composer` (`parse_golive_command` / `parse_user_command`) and drained by `App::tick_stream`, which owns the stream service, the publisher URL modal, and the paired-CLI `open_url` control; the domain contract is `late-ssh/src/app/stream/CONTEXT.md`.
- `/crown` prints who wears the crown, how long they have, and what taking it costs; `/crown take` buys it. Parsed in `submit_composer` (`parse_crown_command`) and drained by `App::tick_crown`, which owns the crown service and both banners. §9c.
- `/pot` prints the weekly pot's size, the tickets in it, what you hold and what it cost, how many more you may buy today, and the time to the draw; `/pot buy N` buys N tickets. Parsed in `submit_composer` (`parse_pot_command`, which is also the boundary that rejects any count outside `1..=10`, the daily cap) and drained by `App::tick_pot`, which owns the pot service and the banners. The status line is answered straight from the process-shared snapshot, so `/pot` costs no query; the domain contract is `late-ssh/src/app/pot/CONTEXT.md`.
- `/icons` opens the icon picker (same as `Ctrl+]`).
- `/poll` opens a modal for the currently visible real room. Polls are room-scoped, support two or three options, can run for 10, 20, or 30 minutes, and are limited to one active poll per room. Active polls render at the top of the room message pane; while one is visible, `va`, `vb`, and `vc` vote for poll options. `v1`, `v2`, and `v3` remain music stream/station selectors. Failed starts show the remaining active wait in the banner.
- `/pomodoro [minutes] [label...]` starts a session-local focus countdown (default 25 minutes, cap 180, label control-stripped and capped at 24 display cells); a leading integer is the duration, so `/pomodoro deep work` is a default-length block named `deep work`. `/pomodoro stop` cancels it, a second start replaces the running one, and the label is echoed in the banner. Parsed in `submit_composer`, drained via `take_requested_pomodoro` in `handle_post_submit_requests`; the timer itself is `App::pomodoro` (in-memory, no DB, dropped on disconnect) because `tick.rs` fires it and the status HUD draws it on every screen. Expiry rides the shared 1Hz edge: it banners and pushes a `GameEvents` desktop notification, and a running timer dirties that edge so the `MM:SS` badge in the top border counts down. The badge is one of the two width-degrading segments in `status_hud_title` (the pot badge is the other, and it sheds first): the right-aligned HUD paints over the left title, so the newcomer sheds its label and then itself when the border is tight (an 80-col terminal with unread mentions + voice + chips has no room for it) rather than eating the page tabs. Expiry still banners and notifies with the badge hidden. Peers see a presence badge instead: every session that changes its timer (start, stop, tick expiry, disconnect teardown in `ssh.rs`) publishes through `App::publish_pomodoro` into the process-shared `common/pomodoro.rs` snapshot-swap directory (same shape as the flair directory), which stores only `ends_at`, never the label; `tick.rs` resolves it once a second into `App::peer_pomodoros` and chat author labels paint the rounded-up whole-minute countdown as a presence badge after AFK. The badge string only changes on minute rollovers, so the chat-row epoch bump is 1/60th the resolve cadence.
- `/roll [NdM ...]` rolls dice into the current room; bare `/roll` defaults to `d20`, caps are 100 dice per group and 1000 sides.
- `/search [query]` opens the Ctrl+/ modal in message-search mode, pre-filled with `?query`. Parsed in `submit_composer`, drained via `take_requested_message_search` in `handle_post_submit_requests` (the modal is App-owned).
- `/summary` asks the AI for a catch-up of the visible public room, from when you last left the app on this device (24h when the device has no mark), or exactly the window you type (`/summary 6h`, `/summary 90m`, up to 48h); see §14 Summary. `/history` opens the scroll-back modal, at the first message you missed when this session has an AFK line for the room; see §14 History Modal.
- `/voice` joins the enabled voice channel for the active room; `/mute` toggles paired-CLI mic mute.
- `/ultimate` opens owned Ultimate Spells.
- Staff-only `/audio`, `/audio fallback`, and `/audio skip` route trusted music controls.
- `/ignore [@user]` mutes a user or lists muted users.
- `/invite @user` adds a user to the selected non-DM room.
- `/leave` leaves the selected non-permanent room.
- `/list` lists public rooms.
- `/members` lists selected-room members.
- `/mod` opens the moderation command modal; `/mod ...` in chat is rejected because commands run only in the modal.
- `/paste-image` asks a paired `late` CLI with `clipboard_image` capability to read the local system clipboard image, sends it back over `/api/ws/pair`, uploads the PNG bytes through the normal image upload path, and inserts the resulting public URL into the composer. Pending clipboard requests time out after 15s so a dead paired client cannot wedge the command.
- `/petname [name]` shows or sets the user's cat name; `/petname clear` removes it.
- `/brb [message]` posts a short away message to the active composer room, marks the session away in the sidebar, publishes a moon badge next to that user's chat name for everyone while any active session is away, and mutes paired audio if it was not already muted. Sending a normal chat message clears away state for that session and only unmutes paired audio when `/brb` performed the mute.
- `/bug <text>` and `/suggest <text>` post a report card into `#bugs` / `#suggestions` regardless of the composer's current room (`ChatService::send_report_task` resolves the room by slug and joins the caller first). A report is a normal chat message whose body starts with `ReportKind::marker()` (`---BUG---` / `---SUGGESTION---`, same trick as `---NEWS---` cards), so reactions, replies, pins, and deletes work unchanged; `ui_text::wrap_report_to_lines` renders the card. Text under 10 chars (`REPORT_MIN_CHARS`) banners usage instead of posting. Those two rooms are report-only: `send_message` rejects free-text sends from non-staff (`report-only:<slug>` error, covers IRC too since it checks the DB slug), while admins/moderators keep plain text so they can reply under a report; everyone keeps reactions ("+1"). The staff-flag DB lookup runs only on that rare gated path.
- `/coffee` and `/tea` post a small ASCII-cup chat message to the current room as a coffee/tea-break ritual. No arguments. Steam pattern rotates per invocation through `CUP_VARIANT_COUNT` variants tracked on `ChatState::next_cup_variant` (session-local, not persisted). Routes through the normal `send_message_with_reply_task` send path — the body is a regular chat message subject to the same length/visibility rules.
- `/private #room` creates a private topic room and joins the caller.
- `/profile [@user]` opens a user's read-only profile modal. Bare `/profile` opens the caller's own profile as others see it. `@username` autocompletion is available after `/profile `.
- `/public #room` (alias `/join #room`) opens or creates an opt-in public room for the caller only (`auto_join=false`).
- `/sheet [@user]` (room-scoped to `#dnd`) opens the character sheet modal: bare form opens your own sheet editable (name + freeform body, saved per user per room on field submit via `ChatService::save_sheet_task`); targeted form opens another user's sheet read-only, or banners if they have none. Resolution and fetch happen in `ChatService::open_sheet_task`; saves and reads validate the shared `RoomScopedCommand` metadata plus room membership in `ChatService::ensure_room_scoped_command_access`; the modal lives in `app/sheet_modal`.
- `/settings` opens settings.
- `/shop` opens the Shop modal (the Shop has no global chord; this and the locked-feature nudges are its only entry points).
- `/unignore [@user]` removes an ignored user.
- `/upload <url>` downloads a public image URL server-side, reuploads it to configured public file storage, and inserts the resulting URL into the composer for the user to send.

Admin commands:
- `/create-room #room` creates a permanent auto-join room and bulk-adds existing users. It is idempotent on rooms that are already permanent, and it promotes an existing non-permanent public room to permanent + auto-join (`ChatRoom::ensure_permanent` UPDATEs the row, then the caller bulk-adds users) — this is how a user-created `/public #voice` room becomes the permanent `#voice` core room. Because promotion bulk-adds every user to a room nobody can leave, `/create-room` is admin-only and a mistyped slug will promote whatever public room matches it.
- `/delete-room #room` deletes a permanent room.
- `/fill-room #room` bulk-adds all users to an existing public room and flips `auto_join=true`; private rooms cannot be filled.

Moderation modal commands:
- `rename-room <#oldname> <#newname>`
- `rename-user <@oldname> <@newname>`
- `view <@user|#room|bans|slows|audit|artboard|help> [pagenumber]`
- `artboard curate <live|YYYY-MM-DD> [reason...]`
- `artboard restore [YYYY-MM-DD] [reason...]`
- `room-voice <#room> <on|off>`
- `kick <server|voice|#room> @name [reason...]`
- `ban <server|#room|artboard|audio> @name [duration] [reason...]`
- `unban <server|#room|artboard|audio|voice> @name [reason...]`
- `slow <server|#room> @name <interval> <duration|permanent> [reason...]`
- `unslow <server|#room> @name [reason...]`
- `admin`
- `admin grant mod @name`
- `admin revoke mod @name`

Moderation list pages show 15 rows. Durations use positive `s/m/h/d` suffixes.

Reply mode:
- Captures `ReplyTarget { message_id, author, preview }`.
- Enters compose mode and clears edit.
- On submit, stores `reply_to_message_id` and prefixes the stored body with a visible quote line for backward-compatible rendering.
- Enter on a selected reply jumps only if the target is already loaded in the current room tail.
- `G` on a selected reply also jumps to the loaded target. Enter is overloaded (image/News modals take precedence), so a reply that contains an inline image can only be followed with `G`, not Enter. Lowercase `g` is the gild key.

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
- File-upload storage is optional per profile: `Config.files` is `Some(FilesConfig)` in prod (endpoint/bucket/URL literals plus `LATE_FILES_S3_ACCESS_KEY_ID`/`LATE_FILES_S3_SECRET_ACCESS_KEY` env secrets). Dev is `None` unless both R2 credentials are set in `.env.local`, which opts uploads into the prod bucket; a half-set pair is a startup error.
- Pasting raw PNG/JPEG/GIF/WebP bytes into the chat composer starts an upload because there is no stable URL to preview until the bytes are hosted.
- Pasting an image URL does not upload or rehost it. It is inserted as normal composer text; after send, inline rendering previews that URL best-effort.
- `/upload <url>` is the explicit URL upload path: it downloads a public image URL server-side, reuploads it to configured public file storage, and inserts the resulting URL into the composer for the user to send and preview.
- `/paste-image` is the explicit paired-CLI clipboard path. It requires an updated `late` paired client, not plain `ssh`.
- Non-admin uploads use a per-session `ChatState` cooldown. This is intentionally lightweight, not a server-side quota.
- URL downloads for upload and inline rendering must go through `files::image_upload::download_url_bytes`: validate `http(s)`, reject localhost/private/link-local/reserved resolved IPs, pin reqwest DNS to the validated addresses, disable redirects, and stream with a hard byte cap. Do not add new ad hoc `reqwest.get(url).bytes()` paths for chat images. Fetchers that must follow redirects (RSS feeds) use `download_url_bytes_following_redirects`, which re-validates every hop.
- Paired clipboard uploads are request-gated: `PairedClientRegistry::request_clipboard_image` records an outstanding request per token, and the pair WS handler drops any inbound `clipboard_image`/`clipboard_image_failed` payload whose token has no outstanding request (`take_clipboard_request`). This keeps a rogue paired client from queuing multi-MB decoded images into the bounded per-session channel.
- Inline image rendering detects likely image URLs in visible room messages, fetches them through the same secure downloader, rejects oversized decoded dimensions, retries transient failures with backoff, and caches an `InlineImagePreview` by message id. Inline previews are only the RGB block fallback used by scrolling chat rows. Kitty/iTerm2/Sixel native image data is fetched separately, lazily, only while the explicit selected-message image modal is open on a supported terminal. Inline previews are best-effort; failures are intentionally silent/noisy only at trace level.
- Kitty, iTerm2, and Sixel image support is intentionally narrow and modal-only. `files::terminal_image` detects Kitty-family terminals from PTY `TERM`, XTVERSION, and forwarded env hints: Kitty, Ghostty, Rio, Warp, and Konsole. It detects iTerm2-family support from `TERM_PROGRAM`/`LC_TERMINAL`, XTVERSION, `TERM_FEATURES`, `OSC 1337;Capabilities`, and env hints for iTerm2, WezTerm, mintty, and hterm-style identities. It detects Sixel from explicit identities (`windows terminal`, `foot`, `contour`, `mlterm`, `sixel`), `WT_SESSION`/`WT_PROFILE_ID` env hints, and DA1 (Primary Device Attributes) replies advertising attribute 4 — the DA1 probe is sent last at alt-screen entry and only fills in Sixel when no richer protocol was detected, so Kitty/iTerm2 always win over Sixel. If `TERM` is tmux, full image previews are intentionally disabled and chat uses the RGB block fallback; no tmux graphics passthrough is attempted. Unsupported or undetected terminals, including stock Alacritty, keep the RGB block preview. Kitty images use late.sh-owned ids in the `0x4C000000..0x4CFFFFFF` range plus a dedicated z-index so cleanup can target them by range/z-index as well as by visible placement. Sixel payloads are generated only for Sixel sessions, use adaptive palette fallback, and fail back to the RGB block preview if the final payload still exceeds the hard byte cap. Because Sixel has no terminal-side scaling, the image modal reports its image cell capacity into `TerminalImageFrame` during draw, the render loop feeds it back into chat state, and Sixel fetches encode to fit that capacity (first fetch is deferred one frame after the modal opens until capacity is known; a cached Sixel encode that no longer fits, e.g. after shrink, is re-fetched at the new capacity). A forced repaint resets terminal image placement state so modal images are re-emitted after clear/resize/drop recovery. Direct terminals get Kitty cleanup commands on enter/leave alt-screen. Alt-screen enter/leave and forced full repaint begin with an ST terminator so a killed session that left iTerm2/Sixel inside an unterminated DCS/OSC image payload can recover before normal clear/repaint bytes. Closing an iTerm2 or Sixel image modal forces a full repaint because those inline images are not tracked/deleted like Kitty placements.

---

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
- Digits `1..9` while reaction leader is active toggle quick reactions, exit reaction leader mode, and keep the message selected.
- Digit `0` while reaction leader is active opens the icon picker for a custom reaction.

Selection deltas are message-based, not row-based. Positive means older, negative means newer.

---

## 9b. Gilds

A gild is chips paid to mark someone else's message, permanently. It is a
purchase, not a reaction: there is no un-gild, and the row outlives every
session. `late-core/src/models/chat_message_gild.rs` owns the table
(migration 154) and `GildTier` (Bronze 500 / Silver 2,000 / Gold 10,000);
`chips.rs::UserChips::transfer_gild` owns the money.

- **Split.** The author receives `floor(price * 2 / 3)` as
  `ChipMove::GildReceived`; the buyer pays the full price as
  `ChipMove::GildSent` (floor-guarded like a gift). The last third has no
  ledger row at all: the burn *is* the gap between the two reasons.
  `GildReceived` is excluded from earnings, so Top Chips ranks what a player
  earned rather than who has generous friends.
- **Guards** (`ChatService::gild_message`, one closed `GildRefusal` enum with
  the wording): message gone, not a member, not a public room (so never a DM
  and never a private room), a game room (`kind = 'game'`: arcade tables,
  daily matches and `#user-live` stream chats are public by visibility but
  not on the Home rail, and the #lounge line must point somewhere people can
  go), self-gild, bot author, the 30s per-buyer cooldown, the same tier
  already held on this message (`AlreadyGilded`), a higher tier already held
  (`HeldHigher`: a gild never goes down), and the chip floor.
  Every refusal is uncharged, and a refusal after the cooldown was stamped
  releases it again.
- **Transaction** (`ChatService::settle_gild`): `SELECT ... FOR UPDATE` on the
  `chat_messages` row first, so every gild on one message serializes. That is
  what makes both the duplicate-tier check and the "third gild" count exact.
  Then insert (`ON CONFLICT DO NOTHING` = already gilded), move the chips,
  count, `pg_notify`, commit. Any early return drops the transaction.
- **Repaint is DB-backed, not broadcast-backed.** The gild transaction
  notifies `chat_message_gilded` with a `<message id>:<room id>` payload;
  `ChatService::start_gild_listener_task` (one connection per process, wired
  in `main.rs`, reconnecting after 5s) turns each notification into a local
  `ChatEvent::MessageGildsUpdated`. This process is not special-cased: it
  learns about its own gilds the same way a second replica does, so there is
  exactly one code path that draws a marker. `GildSucceeded` / `GildFailed`
  ride the in-process broadcast, but they only carry the two banners (buyer
  and author).
- **Rendering.** `message_gilds: HashMap<Uuid, ChatMessageGildSummary>` on
  `ChatState`, loaded with the room tail and patched by the notify. The
  marker leads the message footer, ahead of the reaction chips
  (`ui_text.rs::render_message_footer_lines`): the tier's glyphs
  (`GildTier::marker`, `◆` / `◆◆` / `◆◆◆`) alone, `◆◆◆ ×3` once more than one
  gild is on it, bold in the tier's `BADGE_BRONZE/SILVER/GOLD`. The same
  color runs a heavy bar (`┃`) down the gutter of every line the message
  renders (`ui_text.rs::Gutter`, the cell the yellow mention bar uses), so a
  gilded message reads as a block. `Gutter` is a closed enum ranking the two
  claims on that cell: a mention of you keeps its bar, since the footer chip
  still spells the gild and the mention bar has no other signal. The selection marker `▸` paints over whichever bar holds the cell (`ui.rs::visible_chat_rows` asks `Gutter::is_glyph`), so selection is always visible. Both ride
  the footer and gutter rather than the author header because a message
  inside a run has no header, and a paid marker that vanishes on the second
  line of a run is the one thing nobody would accept. Gild changes bump
  `room_version`, so the row cache repaints.
- **Feed.** `ActivityKind::MessageGilded` fires once, on the message's third
  buyer (`GILD_FEED_THRESHOLD`, `GildOutcome::fires_feed_line`), and names
  only the author: "mira got a message gilded 3 times in #lounge". Never per
  gild, and never on a raise, which adds no buyer. Keyed on the message id
  in the lounge repeat throttle.
- **Who gilded.** `ff` on the message (the reaction-owners overlay) lists the
  gilds above the reactions, one block per tier held with the buyers under
  it. Same event, same membership check (§10).
- **Profile.** `ChatMessageGild::counts_for_author` (owner-scoped in the
  query, off the denormalized `author_user_id`) fills `ProfileSnapshot.gild_counts`
  and renders as a "Gilds received" section, hidden entirely when empty.
- **IRC sees nothing.** The marker is a TUI footer chip; IRC clients get the
  message body and nothing else. There is no `/gild` command either: gilding
  is a message action on a selection, and IRC has no selection.
- **Message deletion takes the gilds with it** (`ON DELETE CASCADE`), and
  `ChatState::remove_message` drops the marker to match. The chip ledger rows
  survive; they are keyed on the message id as `source_ref`.

The tier picker is `chat/gild/` (`state.rs` selection only, `input.rs`
j/k/1-3/Enter/Esc, `ui.rs` the popup). It reads `App.chip_balance` at draw
time rather than caching a balance of its own, and dims tiers the balance
cannot cover; the floor guard in the chip move is what actually decides.

---

## 9c. The Crown

One slot, one holder, one 👑 after their name. The crown is not a rental and
not a Shop item: it is a single row you take off whoever has it by paying
more than they did, and every chip is destroyed.
`late-core/src/models/crown.rs` owns the table (migration 156) and the price
ladder; `late-ssh/src/app/crown/svc.rs` owns the transaction, the refusals,
the telemetry, and the #lounge line. It lives outside `chat/` because it is
its own domain; only the command and the glyph are chat's.

- **Price.** `next_price(paid)` = `max(500, ceil(paid * 1.5))`, and a
  vacant crown is always 500, a Bronze gild's price, so the month's race
  starts on day one. The ladder from empty is 500 / 750 / 1,125 / 1,688 /
  2,532 / 3,798 / 5,697 / 8,546. Nobody tunes it: it ratchets with whoever
  last paid, and the ratchet, not the floor, is what makes it dear.
- **Burn.** `ChipMove::CrownTaken` is a floor-guarded debit with
  `source_ref` = the reign id, and there is no matching credit reason
  anywhere. The whole price leaves the money supply, so the burn is the
  absence of a credit rather than a transfer to a house wallet. It is
  `counts_as_earnings = false` like `ShopPurchase`: taking the crown never
  lowers the buyer's Top Chips standing (pinned by
  `chips_test::earning_exclusions_and_reason_uniqueness`).
- **Guards** (`CrownService::take`, one closed `CrownRefusal` enum with the
  wording): you already wear it, and the chip floor. That is all: there is
  **no hold or cooldown**. A reign is takeable the moment it exists, at the
  next rung, so the month end is a real auction (the last take before
  midnight wins the badge) rather than a clock game around a hold window.
  The 1.5x ladder is the only throttle on a war, and a war is the story.
  Known cost: a take that races another one pays the rung the other just
  set, not the price `/crown` quoted a moment earlier; the receipt banner
  says what was paid. Every refusal is uncharged, and a refusal drops the
  transaction, so nothing is left behind.
- **Transaction** (`CrownReign::lock_open` then close, open, debit, notify).
  Two locks, and neither replaces the other: `pg_advisory_xact_lock` is what
  serializes a take from a *vacant* crown (there is no row to take
  `FOR UPDATE`, so without it two first takes would both insert and one
  would die on the partial unique index), and the `FOR UPDATE` keeps the
  read-then-write on an existing reign exact. A second racing take therefore
  reads the reign the first one opened and pays the next rung, not a
  constraint violation.
- **One open reign, ever.** `crown_reigns_single_open` is a unique index on
  the constant `(ended_at IS NULL)` filtered to open rows, so "at most one"
  is a table fact.
- **Month rollover has no sweeper.** A reign carries the first of the UTC
  month it was taken in (stamped in SQL from the same clock as `taken_at`,
  so a take blocked across midnight cannot be born stale), and
  `CrownReign::is_current` requires that month to still be the current one.
  At the boundary the crown simply reads as vacant at the minimum, the glyph
  stops resolving on the next 1Hz tick, and the next take closes the stale
  row on its way past. Same read-time expiry the rentals use. That makes
  `ended_at` the moment the row was replaced, not the moment the reign
  stopped counting; a history reader wants `LEAST(ended_at, month + 1
  month)`.
- **Distribution.** `CrownService` holds a process-shared
  `watch<Option<CrownHolder>>` (user id plus month), seeded by the listener
  and refreshed on the `crown_changed` notify
  (`start_listener_task`, one connection per process, wired in `main.rs`,
  reconnecting and re-seeding after 5s). The notify payload is a
  `CrownChange` (taker name, price, deposed id): the holder is re-read from
  the table, and the deposed holder's banner is raised from the payload as a
  `CrownEvent::Deposed`, so it lands on whichever replica they are on. The
  selling replica is not special-cased: it learns about its own take, and
  tells its own deposed holder, the same way a second replica does
  (`svc_test::a_listening_replica_learns_the_holder_and_tells_the_deposed`).
- **Rendering.** `App::tick` reads the watch on the same once-a-second edge
  as the flair directory, filters it through `CrownHolder::if_current`, and
  passes the holder into
  `common/username_effect.rs::resolve_all`, which sets `ResolvedName.crown`.
  Every surface that already reads `name_flair` therefore gets the crown for
  free and no render ever queries for it; a holder who has bought nothing
  else gets an entry of their own. The glyph follows the name after one
  space (`bob 👑, the night clerk 🐱`): it sits ahead of a rented title and
  ahead of the badge stack, carries no clickable segment of its own, and is
  painted in `AMBER_GLOW` rather than taking the name's effect
  (`ui.rs::build_author_prefix_and_segments_with_chat_badges` builds the
  range, `ui_text.rs::push_author_prefix_spans` paints it). The glyph is an
  emoji, two cells wide; chat measures it with `unicode_width`. The
  Clubhouse floor label does the same (`clubhouse/ui.rs::clubhouse_label`
  glues the glyph on, `put_label_styled` paints the char at `name_len`
  amber), and since the floor is one char per cell it spends two cells on
  it: the emoji plus a `WIDE_TAIL` sentinel the row flush skips, so the
  walls stay aligned. It is the only wide char a floor label can hold;
  names and titles are folded to single width.
- **Commands.** `/crown` and `/crown take` are parsed in `submit_composer`
  and drained by `App::tick_crown`. Both answers arrive as banners off
  `CrownEvent`, because the crown service is not `ChatService` and has its
  own broadcast (the `StreamService` shape). The deposed holder is told who
  took it (`CrownEvent::Deposed`, off the notify): a glyph vanishing off
  your own name with no explanation reads as a bug.
- **Feed.** `ActivityKind::CrownTaken` fires on every takeover and names both
  players. It is the one event with two #lounge surfaces: the usual ticker
  line ("tom stole the crown from mira for 1,688", `· `-prefixed, diverted
  into the one-row ticker like every system line) and a **headline**, a
  real message from `system` with no prefix, so it renders as a chat row
  and stays in history: "👑 tom stole the crown from mira for 1,688 chips.
  Next price: 2,532 chips." (`activity/filter.rs::lounge_headline`, the
  exhaustive twin of `lounge_includes`; posted by the lounge feed task right
  after the ticker line, behind the same repeat gate, keyed on the reign
  id). The event carries `next_price` so the headline quotes the ladder
  without re-deriving it. Unlike the gild line this one names the loser on
  purpose: the crown is a single slot, so the takeover *is* the story.
- **Award.** The month's last holder gets a `profile_awards` row, category
  `crown`, granted by the existing monthly snapshot
  (`snapshot_previous_month_profile_awards`, a `crown_holder` CTE reading
  last month's latest reign). The badge is `CRWN` with no rank digit
  (`profile_award::is_rankless_award`), and it is *not* a milestone, so it
  shows for the month after and then makes way for the next holder's.
- **IRC sees nothing, and cannot play.** The glyph is a TUI author-header
  span, so IRC clients get the message body and nothing else, and `/crown`
  is composer-parsed (`submit_composer`), which the ircd send path never
  reaches.

---

## 9d. The Round

One patron buys everyone at the bar a drink. The chips are burned like the
crown's; what the room gets is a #lounge line and a free drink each.
`late-core/src/models/drink_round.rs` owns both tables (migration 164), the
price, and the phrase list; `GhostService::bartender_round`
(`app/ai/ghost.rs`) owns the transaction's caller, the refusals, the
telemetry, and what @bartender says; `ChipService::buy_round` owns the money.
It touches chat twice, which is why it is documented here: the phrase gate
reads a chat message, and `chat/slur.rs` has to leave that phrase alone.

- **The trigger is a literal phrase.** `ROUND_PHRASES` ("round for everyone",
  "round for all", "round for the house", ...) matched case-insensitively on
  word boundaries, so "turn around for all of us" is not an order. No model
  decides this: it is the only bartender action that spends more than one
  drink's worth, the price is the size of the room, so the phrase is the
  confirmation. An order is a statement: `contains_round_request` rejects a
  phrase whose sentence runs on to a `?` and never looks inside backticks, so
  "how much is a round for everyone?" is a question the model answers, not a
  bill. (`round_phrase_spans`, the slur guard's view, still protects the
  words wherever they appear.) It also means a round costs no model call.
- **A settled round answers ahead of the mention ladder**, in both the event
  loop's pre-filter and the reply, because throttling a paid action would
  swallow a purchase in silence. A refusal is free, so it steps the ladder
  like any other answer and is dropped when throttled: repeating the phrase
  into an empty house costs the room one @bartender line per ladder window.
- **`slur.rs` never scrambles it.** Drunk text is stored, not rendered
  (§14 Drunk Text), so a wasted patron's order would otherwise reach the
  matcher as "ronud for eevryone" and the feature would break for exactly the
  people most likely to use it. `slur_segment` passes any token overlapping a
  `round_phrase_spans` range through untouched, and `with_hiccup` will not
  drop a `*hic*` inside one. Both the guard and the matcher read the one list
  in `drink_round.rs`, so they cannot drift apart. The words around the order
  still take their beating.
- **Price** 100 (`ROUND_PRICE_PER_PATRON`) for every credit that actually
  landed, never for the heads counted: the grant's own `RETURNING` is what
  the charge is computed from, in the same transaction, so two rounds racing
  cannot both bill for the same patron. `ChipMove::RoundPurchase`,
  floor-guarded, burned whole, out of Top Chips like the crown.
- **Presence is `state::online_human_ids_excluding`**, the in-process
  `active_users` roster minus the bots and the buyer. Single-replica by
  choice (SHOP.md Phase 8 status); the credits it grants are DB rows and cash
  from anywhere. Excluding the buyer is what makes "nobody to buy for" a real
  refusal rather than a round bought for one.
- **Only the buyer is poured into**, on the spot, `ROUND_DRINK_POINTS` in the
  purchase transaction: they typed the order. Everyone else gets a
  `drink_credits` row, not a drink, because a pour makes someone type drunk in
  public and they did not ask. It is cashed only
  by ordering from @bartender, at most one open credit per patron across
  every round (a partial unique index; an expired one is re-used rather than
  blocking the slot forever), 24h to claim, and cashing is one guarded
  UPDATE so two orders cannot drink it twice.
- **A cashed drink pours a flat 300 points** whatever the bartender named it:
  three times what the buyer paid, exactly the buzzed threshold, so the room
  visibly moves a level for about an hour. `lifetime_spent` does not move.
  The open credit is read before the prompt is built and becomes the
  `BartenderTab` handed to `parse_bartender_order`: on a comped tab a "pour"
  (or an "offer", the model deciding they could not afford it) becomes
  `PourComped` and skips both price gates, since nothing is debited; that is
  what lets a patron sitting on the chip floor, or a model that obeyed "do
  not quote a price", still drink what was bought for them. If the credit is
  gone by the time the pour lands, nothing is poured or charged and a
  scripted line says so.
- **Scripted lines, not generated ones.** The announcement and all three
  refusals are consts in `ghost.rs`. This is the one line that has to quote
  the number the patron was charged, and it has to land the moment the chips
  move rather than after a model round trip that could time out or invent a
  different figure. The prompt hands out the phrase when asked but never
  claims a round happened.
- **Feed.** `ActivityKind::RoundBought` posts a ticker line and no headline:
  @bartender already says it in the room, and everyone it reached is online
  by definition, so a #lounge row would be the third telling of one drink.

---

## 10. Reactions, Ignores

Reactions:
- One reaction per `(message_id, user_id)`.
- Reactions are stored as icon text in `chat_message_reactions.icon`.
- Quick reaction keys `1..9` map to the default emoji set; `0` opens the full icon picker.
- UI appends reaction footer chips under the message body or news card.
- Reaction summaries live in `message_reactions: HashMap<Uuid, Vec<ChatMessageReactionSummary>>`.
- Reaction-owner overlay (`ff`) waits for a matching `ReactionOwnersListed` event keyed by `pending_reaction_owners_message_id`. The event also carries the message's gilds (`ChatMessageGild::list_for_message`, best tier first), which `reaction_owner_lines` lists above the reactions as one block per tier held (`◆◆◆ 1 Gold gild`, buyers under it), sharing the reaction blocks' name capping.

Ignores:
- `users.settings.ignored_user_ids` stores UUIDs, not usernames.
- `users.settings.friend_user_ids` stores private one-way friend marks as UUIDs.
- `/ignore @user` and `/unignore @user` resolve usernames at command time.
- A message is hidden if its author is ignored, OR if `chat_messages.reply_to_user_id` is an ignored user. The latter hides bot/automated replies directed at an ignored user so they cannot be heard by proxy through `@bot`/`@graybeard`/`@bartender`. Only bots set `reply_to_user_id` (via `ChatService::send_bot_reply_task`); human replies use `reply_to_message_id`. The shared filter helper is `state::message_is_ignored_in`.
- Ignore filtering applies to DMs too. An ignored peer's DM messages are filtered, and the DM room is hidden from the room rail/navigation while the peer is ignored (both mirrors skip DMs whose `dm_peer_id` is ignored: `visual_order_for_rooms` and the rail's own DM filter, via the shared `dm_peer_is_ignored`), so a new DM from the ignored user can't resurface the room or its unread badge. Unignoring restores the DM on the next render/snapshot.
- `IgnoreListUpdated` refilters local messages in place (all rooms, including DMs and `reply_to_user_id` matches) with no DB refetch, then refreshes the Mentions list/unread count.
- `unignore` does not retroactively restore already-filtered local messages until a future tail/snapshot naturally reloads them.

---

## 11. Synthetic Entries

Synthetic entries are selected from the room list but are not normal `ChatRoom`s.

### News

- Backed by persisted `articles`.
- `ArticleService::process_url` extracts title/summary/image, stores an article, and posts a compact `---NEWS---` announcement into `#lounge`.
- Publishing pays the sharer `NEWS_SHARE_REWARD_CHIPS` (500) as `ChipMove::NewsShared`. `Article::create_shared` (`late-core/src/models/article.rs`) is the only path a user-facing share may take, so the News composer and an RSS `s` share pay exactly the same. Chips are minted, not moved, and count toward Top Chips.
- The reward is capped at one per URL per user and at `NEWS_SHARE_MAX_PAID_PER_DAY` (3) paid shares per UTC day, and the `chip_ledger` row is what enforces both, keyed on `(user_id, url)` and counted by `created_at` date like pot tickets (hence `source_ref` holds the URL, not an article id; migration 163 indexes the lookup). The `articles` row cannot be the record of payment: deleting a story frees its URL, so paying on insert alone would let one player share, delete, and re-share the same link forever. `articles.url` is unique, so while a story is live only its first sharer was paid.
- A repeat or capped share still succeeds and still posts to `#lounge`; it just mints nothing. `Article::create_shared` returns a closed `NewsShareReward` (`Paid` / `RepeatUrl` / `DailyCapReached`) that rides `ArticleEvent::Created` and `FeedEvent::EntryShared`, so `news::state::news_share_banner` says what the ledger did ("+500 chips" / "Already paid for this link" / "Today's 3 paid shares are used up") and `metrics::record_news_shared` labels `late_ssh_news_shares_total` by the same outcome. An RSS entry marked shared because its link was already in News carries `reward: None` and raises no second banner over "Already shared.".
- Insert, ledger lookup, and credit are one transaction under a `pg_advisory_xact_lock` keyed on `('news_share', user_id)`, the shape `GamePayout` and the pot use: a failed credit leaves no orphan article squatting on a globally unique URL, and two shares by one person landing together serialize, so the day cap is exact rather than read-then-write.
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

`ui.rs::draw_room_header` owns everything between the room rail and the messages, and returns the area left for messages. Each content row pairs live state on the left with the keys or commands that act on it flushed right (`primitives::row_with_hint`): the voice row (`voice::ui::voice_strip_line`, present only for voice-enabled rooms), a dim full-width rule when voice and a topic are both present, the topic row with a `/rules` hint when the room has rules, and a closing rule that separates the block from the conversation. A room with neither voice nor a topic keeps the full height for messages, and the whole header yields if it would leave fewer than two rows for them.
- The stream row (`stream_header_line`) fits everything around its watch link, not the other way round: `row_with_hint` drops a hint it cannot fit rather than wrapping it, and here the hint *is* the URL. So the hint is measured first, the title clips to whatever is left, and the ` · N watching` count drops when even that is not enough. A `watch: https://…/live/<id>` plus its load-bearing trailing cell runs 51 columns, which is why stream capability ids moved from 32-char hex to 22-char base64url (`stream/CONTEXT.md` §2): at hex width the link almost never rendered, and a title budget guessed ahead of the hint (a hardcoded `width - 30`) meant the link was what got dropped rather than the title. The watcher count survives down to 73 columns.
- `/` opens an inline substring filter over room slugs (footer shows the live query); typing edits it, `selected`/`visible_items` track the filtered subset, and `Esc` clears+closes it. While `discover.is_filtering()`, `app::input::handle_byte_event` and `chat::input::handle_byte` route every byte (digits, `space`, `h`/`l`) into the filter so it captures an unrestricted query; arrows still navigate. `start_slash_command_composer` excludes Discover so `/` never starts a slash command there.

---

## 12. Rendering Constraints

Home chat center:
- The room rail is rendered by `draw_room_list_rail` outside the center pane when the terminal is wide enough.
- The center pane renders messages or a synthetic entry, with the composer at the bottom.
- Composer height is dynamic but capped at 8 lines.

Home lounge dashboard chat:
- Uses `DashboardChatView`.
- Composer is capped at 5 visible lines.
- Lounge chrome is controlled by the user's Dashboard Header setting, then by vertical priority: the top activity/quest/shop strip drops before chat when space is tight.

Embedded game chat:
- Uses `EmbeddedRoomChatView`.
- Composer is capped at 4 visible lines.
- Game-backed chat rooms are joined through their surface's idempotent `join_game_room_chat` (fired from `App::tick`), not the Home room rail.

Message rendering:
- Local message storage is newest-first.
- Rendering reverses to oldest-first rows with newest at the bottom.
- Selected messages replace the leading pad with a selection marker.
- Highlighted reply targets get background styling across the whole row range.
- Message wrapping is word-aware and uses Unicode display width, not codepoint count; hard splits are only valid for a single word longer than width.
- Display author labels are plain usernames without leading `@`; mention syntax still uses `@username`.
- Author labels render as `username[ 👑][, title] [profile awards] [special...] [bonsai] [badge] [flag] [milestone] [brb]`. The crown and the title are marks on the name and lead; everything after the first space is the badge stack, in that order. Special badges come from a hardcoded per-username allowlist in `chat/special_badges.rs` and must stay in `mod`, `developer`, `artist` order. The bonsai glyph comes from `bonsai_glyphs` keyed by user_id. Profile award badges come from `profile_award_badges` keyed by user_id: top-3 last-completed-UTC-month leaderboard awards plus the best rankless Lateania boss achievement badge (`LAD` unless `LFK` is also present, then `LFK`), ordered by rank and then category priority, rendered as one bracketed group. Equipped store badge and flag are split for separate hit targets and rendered badge before flag. A burn milestone (`milestone_badge`, see `hub/CONTEXT.md`) follows both of them and never displaces either: rentals cost a hundred chips and a milestone costs at least fifty thousand, so nothing anyone rents may hide one. It rides `ResolvedName.milestone` off the same flair map as the crown (so no render queries for it), takes `HeaderTarget::StoreMilestone` for its own hit target (the Ultimates tab, where the ladder is sold), and is a plain badge otherwise: no color of its own, no clickable name behavior, no expiry. The `/brb` moon badge is derived from shared `ActiveSession.afk`, not message metadata, so it is visible to all viewers while the author is away. Two Shop-driven decorations share the bare-username range in `AuthorTint`, on different style axes so they compose instead of colliding: the bartender drink tint paints the background, and a bought username effect (Name Glow/Gradient/Shimmer, 24h or 30-day tier, see `hub/CONTEXT.md`) paints the foreground per character via `App.name_flair`: the effect fg deliberately overrides own-amber/friend-gold/default author fg while keeping bg and modifiers. A rented title is the third Shop decoration and deliberately claims its own range instead of the name's: `build_author_prefix_and_segments_with_chat_badges` returns a `title_range` for the `, <title>` it writes between the name and the badge stack, `AuthorTint` carries it, and `push_author_prefix_spans` paints it in `TEXT_DIM`: a title is text, so it never takes the name's color effect, and it carries no clickable segment of its own (the name beside it already opens the profile). The badge segments' columns shift past it. Resolved `ResolvedName` changes (style, title, crown, or milestone) bump `App::chat_ctx_epoch` (1Hz value compare in `tick.rs`), so shimmer repaints at most once a second. Migration 104's retired Bot Username Color remains the cautionary tale: it was per-viewer; these are globally visible. Do not add a third decoration on the author label without retiring one of these.
- Author badge glyphs are separated by `AUTHOR_BADGE_SEPARATOR` (` `). The separator was intentionally returned to a plain space after dot separators failed to prevent terminal-cell drift.
- Investigation note: if a known author glyph is missing on a newly rendered message but appears after terminal resize, first suspect Ratatui/crossterm diff rendering of wide emoji cells, not author metadata. Sent-message events reload author metadata before `push_message`, chat row cache counters cover `bonsai_glyphs`, `chat_badges`, `profile_award_badges`, and AFK state, and resize forces a full terminal clear/redraw. A prior workaround forced full repaint on message-selection scroll, but it was removed because it caused visible flicker; prefer a targeted ratatui/backend fix for wide/VS16 emoji cell drift.
- Ratatui wide/VS16 investigation detail: Ratatui owns the buffer diff model: it renders widgets into a buffer, diffs current vs previous, then writes only changed cells to the backend. Official docs describe that flow at `https://ratatui.rs/concepts/rendering/under-the-hood/`. In this app's failure mode, `ratatui-core` emits extra trailing-cell updates for wide VS16 emoji, while `ratatui-crossterm` prints `cell.symbol()` but tracks the last position as if every printed symbol advances exactly 1 cell. A glyph like `🛡️` is one visible grapheme but 2 terminal cells wide, so the backend's "next update is adjacent, no `MoveTo` needed" optimization can become wrong after wide glyphs. This should be treated first as a Ratatui backend/diff issue, not a `crossterm` crate issue: crossterm is printing what Ratatui asks it to print, while Ratatui's backend decides when cursor moves are needed.
- Proposed upstream path: build a tiny repro outside late.sh that renders rows with `🛡️ 🔨️ 🌼`, then shifts/swaps rows like chat scrolling or room switching; add a Ratatui regression test around wide VS16 glyph diff/backend output; then patch either `ratatui-crossterm` cursor accounting or `ratatui-core`'s VS16 trailing-cell strategy. The naive backend fix is to track printed width instead of cell count, but test it carefully because Ratatui's explicit trailing-cell update may also need adjustment. A failing test/repro first will make the PR easier to get accepted.
- The small Markdown subset supports headings, bold, italic, inline code, blockquotes, and simple `- ` list items.
- `---NEWS---` cards use special boxed rendering.

Cache:
- `ChatRowsCache` stores wrapped rows plus selected/highlighted row ranges.
- Validity is an O(1) counter compare, not a content hash. The key (`ChatRowsCacheKey`) is `ChatRowsVersions` (rendered `room_id`, that room's `room_version`, `ChatState::context_epoch`, `App::chat_ctx_epoch`) plus width, theme, current minute, unread marker, current user, and flag-fallback. Nothing hashes message bodies per frame anymore.
- `room_version` bumps on any message-store change for that room: `push_message`, `remove_message`, `replace_message` (edits/pins), `merge_room_tail`, reaction updates, and snapshot merges whose `(id, updated)` sequence differs.
- `ChatState::context_epoch` bumps when author context actually changes: usernames (`note_username`/`extend_usernames`), countries, friends, ignore refilters, glyphs/badges (`set_context_value`), inline image cache changes, and snapshot applies with real diffs. Snapshots arrive every 10s regardless of change, so every snapshot write compares before bumping.
- `App::chat_ctx_epoch` covers App-owned inputs: AFK set and username directory (Arc pointer compare in `render.rs`), drunk levels and resolved name styles (value compare in the 1Hz `tick.rs` block).
- When adding a new render input to chat rows, either put it in `ChatRowsCacheKey` directly (cheap values) or bump one of the counters at its mutation site; a missed bump is a stale-rows bug.
- Composer wrapped rows are cached separately in `ChatState`; invalidate when text or width changes.

---

## 13. Keybindings

### Home Chat Center

| Key | Action |
|-----|--------|
| `h` / `l` / `left` / `right` | Switch room/synthetic selection |
| `Ctrl+N` / `Ctrl+P` | Next/previous room |
| `Space` | Room-jump mode |
| `j` / `k` / arrows | Move message selection or synthetic-list selection; inside a selected message taller than the pane they scroll it by rows first (§9) |
| `Ctrl+D` / `Ctrl+U` | Approximate half-page message selection |
| `i` | Start composing in selected room, or start News composer when selected |
| `/` | Start command composer in selected room |
| `Enter` | Submit composer; open selected chat news preview; jump reply target; copy URL in News; join Discover; jump Mention |
| `G` | Jump a selected reply to its loaded original, even when the reply contains an inline image (Enter opens the image instead) |
| `Alt+Enter` / `Ctrl+J` | Insert newline in main chat composer |
| `Alt+S` | Submit main chat composer and keep it open. Dropped (no-op) while the `keep_composer_focused` Tweaks setting is on; Enter then owns send-and-stay. |
| `Esc` | Cancel compose/overlay/autocomplete/room jump |
| `r` | Reply to selected message |
| `e` | Edit selected own/admin message |
| `d` | Delete selected own/admin message (press `dd` to confirm) or News article |
| `p` | Open selected author's read-only profile |
| `c` | Copy selected message body |
| `t` | Translate selected message; press again to collapse, again to reopen. A message already in your target language banners instead of spending a call. |
| `g` | Open the gild tier picker on the selected message (your own message, a DM, a private room, or a game/stream chat banners the refusal instead of opening). In the picker: `j`/`k` or `1`-`3` pick a tier, `Enter` buys, `Esc` cancels. |
| `f` | Favorite/unfavorite the selected real room |
| `[` / `]` | Move the selected favorite up/down in the room rail |
| `f` then `1..9` | Quick-react to selected message |
| `f` then `0` | Open icon picker for a custom reaction |
| `f` then `f` | Open reaction-owner overlay |
| `Ctrl+]` | Open icon picker; inserts only into main chat composer |
| Double-click composer bar | Enter compose mode (same as `i`). Dashboard + embedded game chat only. |
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
Dashboard's `selected_room_id` or the open game surface's chat room
chat-room id.

The chat scroll itself uses the same capture-on-draw pattern: each draw site
that paints messages (Home `#lounge` dashboard card, Home chat center
real-room branch, and embedded game chat) publishes a `ChatHitLayout` into
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
| Cyberspace feeds | `j/k` navigate, Enter open thread (or link modal when unlinked), `p` post, `c` copy entry link, `n` notifications, `r` refresh/reply, `b` back |
| Cyberspace pickers (`/cs chat`, `/cs mail`, command only) | `j/k` move, Enter pin the room or conversation into the rail section or take it off, Esc close |
| Cyberspace room / C-Mail conversation (a rail entry) | `j/k` scroll, `g` newest, `i`/Enter focus the composer (the normal chat composer slot, titled with the room), Enter send, Esc back to reading then leave, `b` leave |
| Mentions | `j/k` navigate, Enter open the Ctrl+/ single-message preview (Enter again jumps to the room) |
| Discover | `j/k` navigate, Enter join selected public room, `/` open slug filter (type to narrow, Enter join, `Esc` clear) |

Directory Projects and Profiles reshuffle their listing on page/tab entry. News keeps its chronological order — only mine-only filtering applies. The slash-command composer in `app/input.rs` skips itself when News is selected so `/` reaches the synthetic-entry handler; Directory page 5 routes `/` directly to Projects/Profiles filtering.

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

### Drunk Text

A patron deep enough into the tavern's drinks types like it. `ChatService::slurred_body` runs `chat/slur.rs` over the body as the last step of `send_message`, immediately before the row is written.

- **Stored, not rendered.** The slurred text *is* the message body, so IRC, search, replies, and every viewer agree on one version. This is deliberate: the drunk level is decay-based, so a render-time transform would re-evaluate against the reader's *current* level and quietly sober up an old message hours later. The level at the moment of typing is the only one that ever made sense. The cost is that the original is unrecoverable, including for moderation.
- **Public rooms only** (`room.visibility == "public"`). DMs and private rooms can carry something that genuinely needs reading. The `UserDrinks::find` level lookup only runs where it can matter, and sober users and the ghost bots (who never drink) short-circuit to an unchanged body.
- **Runs last**, so report markers, `contains_link` cooldown, and slow mode all judged the sober text. `create_mentions_task` gets the slurred body so the notification preview matches the room.
- **Readability rests on one rule:** a word's first and last character never move. Only interior letters are reordered (never added or dropped), which is the typoglycemia effect and is why level 4 stays legible at all. Two dials climb per level: what share of words get scrambled (6/32/60/85%) and how far each goes (one swap, one swap, one-or-two swaps, full interior shuffle). Tipsy and buzzed deliberately share a depth: the same fumble, just far more often. The change in *kind* lands at sloshed. Measured over ordinary prose that is roughly 3/21/33/58% of *all* words visibly changed, since short words are ineligible; `each_drink_reads_harder_than_the_last` pins those bands.
- **Protected tokens are never touched:** `@mentions` (they drive notifications and the mention wash), `#slugs`, URLs, backtick code spans, `---NEWS---`-family markers, the leading `> ` reply quote line (someone else's words), and anything non-ASCII (so CJK and emoji pass through whole). The level-4 `*hic*` only widens an existing gap and respects the same exclusions.
- `slur(body, level, seed)` is pure with a caller-supplied seed; `svc.rs::slur_seed` supplies a fresh one per message. Tests live in `slur_test.rs`.

### Translation

Chat messages translate on demand (`t`) or, opt-in, automatically. The model call lives in `app/ai/translate.rs` (`TranslationService`, a Gemini `generate_json` schema call); `ChatState` owns only per-session display state.

- **Cost scales with messages written, not readers.** Every result is cached in `message_translations` keyed `(message_id, target_lang)`, so the first viewer's call covers everyone after them, forever, including a session reconnecting tomorrow. Two guards keep that true: single-flight dedupe in the service (twelve sessions rendering the same new message make one call) and a bulk cache-only lookup when entering a room.
- **Same-language is a cached verdict, not an absence.** The model reply carries a `same_language` flag (an echoed body counts too, the guard for URL-only messages); the verdict lands in `message_translations.same_language` so nobody pays for the call again, renders as nothing (no `↳` line), and `t` on it banners "Already written in X". This is what lets English translate: `En` claims no script (`TranslateLang::script`), so French/Spanish/German bodies reach the model for English readers like any shared-script target, and the model, not the script check, decides what was already readable. The old behavior (English claiming Latin, silently refusing to translate any Latin-script language for the English majority) was the top user complaint about the feature.
- **The script precheck survives as the cheap first filter.** `needs_translation` (`late-core/src/models/message_translation.rs`) still clears unambiguous cases locally (a Han body for a Han target, unscripted bodies, over-cap walls) so they never reach the API. It is a *script* detector, not a language detector; targets without their own script (English and the Latin roster; Russian/Ukrainian on Cyrillic) send every scripted body to the model and let the cache absorb the cost.
- **Replies translate the reply, not the quote.** The composer bakes `> @author: preview` into the stored body, and `translation_source_text` strips that first line everywhere translation looks (the precheck, the model prompt, the cached text), so the quoted author never gets re-worded and the `↳` line carries only the reply. The staleness guard still compares the full stored body, since that is what an edit rewrites.
- **Authors can pre-share in English, and shared means shown.** `Ctrl+O` → Settings → Translation → "Translate my messages to English" (`users.settings.translate_mine_to_en`, off by default): the send and edit paths (`ChatService::pretranslate_for_author`) fire `TranslationService::request_shared` for the author's own message. The cache row lands with `author_shared = true`, the broadcast event carries the flag, and every session whose target language matches displays the `↳` line with no auto mode and no `t`, live via the event and later via the room-entry sweep. The author's own session never shows it (they wrote the original), and a reader's private `t` rows stay private: display is the author's choice, never a side effect of someone else reading. One call per message written by the opted-in author, spent from the same daily cap.
- **Auto mode is live-only, by policy.** A foreign-script message arriving in the room *on screen* auto-expands; history does not, which is what bounds auto mode's cost and matches what the feature is for (following a live conversation, not machine-translating the archive). History is always one `t` away. Cached history *is* shown pre-expanded on room entry, since reading the cache is free.
- **The render rule is one line:** foreign script + cache hit → show expanded; cache miss + live → fire a call; cache miss + history → collapsed, `t` on offer. The room-entry cache sweep runs for **every** session (it is cache-only, so it costs one batched read and zero API calls); the drain decides display: auto mode shows all hits, everyone else shows author-shared rows only. A `t`-collapse is a per-session override (`translation_hidden`) that wins over auto mode, the cache, and author-shared rows, so a message you dismissed does not spring back open next frame.
- **Invalidation is mandatory, not hygiene.** A cached translation describes the exact body it translated: `ChatService::edit_message` deletes the message's rows inside the edit transaction, and `ChatState::forget_translation` drops the session's copy on edit and delete. Skipping either leaves a translation asserting something the author no longer said. Changing the target language clears every stored translation for the same reason, and late results for the old target are dropped on arrival. The cache write itself is conditional (`upsert_if_current` checks the live `chat_messages.body` against the body that was translated), which closes the race where an edit lands while the model call is in flight: the edit's row delete finds nothing to delete, and the stale result is then discarded instead of cached.
- **Guardrails are for bugs and abuse, not for legitimate traffic**, which is orders of magnitude below them: a global daily call cap (`TRANSLATE_DAILY_CAP`, degrading to "translation unavailable" until UTC rollover), a 4-way concurrency gate so a burst queues instead of tripping API rate limits, and a body-length cap. Failures never retry on their own; `t` is the retry. `record_chat_translation` reports every outcome (`cache_hit` / `translated` / `same_language` / `failed` / `cap_exhausted` / `stale`), so the cache hit ratio and the cap are both visible.
- **DMs and private rooms are included.** The *viewer* opted in and it is their received text, unlike Drunk Text above, which is excluded from private rooms because it rewrites what the *author* said.
- Target language, auto mode, and the author-side English share are per account (`users.settings`: `translate_to`, `auto_translate`, `translate_mine_to_en`), edited under `Ctrl+O` → Settings → Translation; the first two sync into `ChatState` each tick, the third is read by `ChatService` at send/edit time. The settings row reads "Target language" because `translate_to` now decides two things: what `t` and auto mode translate into, and which authors' shared translations this session receives (a shared English row shows only to English-target readers).

### History Modal

Scroll-driven room history (`history_modal/`), opened by `/history` or by a search hit/mention older than the loaded tail. Three open modes on `ChatHistoryModalState`: `open_at_tail`, `open_at_message` (anchored, highlighted), and `open_at_unread`. `/history` picks between tail and unread by this session's AFK line for the room (`ChatState::afk_lines`); the cutoff is always that, never `chat_room_members.last_read_at`, which records presence rather than reading. `/history` only *reads* the line and never clears it: opening scrollback is how you go and look at the backlog, not proof you got through it. `ChatService::load_history_unread_task` resolves the exact first unread server-side (`ChatMessage::first_unread_after`, same read-scope/ignore/system filters as history pages, so it can sit further back than the tail's 500) and answers through the anchor pipeline; when nothing qualifies it degrades to a plain tail page under the same request id, which the modal accepts as a tail open. The modal draws the same `new messages` divider as the live tail, before the first loaded message from someone else past the cutoff (`unread_divider_target`); a room with no AFK line gets no divider, mirroring the live tail.

### Summary

`/summary` is the AI room catch-up: one call summarizing the visible public room, either since the reader last left the app on this device or over a window they typed (`/summary 6h`). `app/ai/summary.rs` owns the whole pipeline (`SummaryService`, translate.rs-shaped); `ChatState` only parses the command, hands over the device mark (`parse_summary_arg` + `catch_up_window`), and drains `SummaryEvent`s in tick: ready summaries open the shared `Overlay`, waiting in `pending_summary_overlay` when another overlay is already up (a banner says so, and the summary takes the surface as soon as it frees); everything else banners.

- **Window**: the command layer parses the argument into a closed `SummaryArg` (`CatchUp`, `Window`, `Unparseable`, `TooShort`, `TooLong`, each with its own banner) and resolves `CatchUp` through `catch_up_window(device_left_at)` into a `SummaryWindow` arm. The mark is a fact about this terminal that lives only in the session (and on the device key), so the session is what supplies it, and the service keeps no cursor of its own. `summary::window_start` resolves every arm and reports a closed `SummaryBasis` plus a `capped` flag alongside the start, so the overlay head can say which sentence it is telling.
  - `SinceLeftApp(at)` (the device has a mark) → exactly that instant; basis `LeftApp`, head "since you left · stamp". **Nothing widens it.** A mark ten minutes old means a ten-minute catch-up, which is the entire point of resting on when the reader left.
  - `Default` (no mark: keyless, never ended a session here, or the write was lost) → `SUMMARY_DEFAULT_WINDOW_HOURS` (24h), basis `Default`, head "since stamp · the last 24h", with no claim about absence either way. The only window that is handed out rather than derived.
  - `Explicit(duration)` → what the user typed, bounded only by the 48h max; the mark is ignored outright.
  - A mark older than `SUMMARY_MAX_WINDOW_HOURS` is clamped to it and the head appends "· capped at 48h": the stamp is then the cap, not the moment they left, and the head says which.
- **The 24h default must not come back as a blanket clamp.** It used to widen *every* bare catch-up because the cursor underneath it was `chat_room_members.last_read_at`, which records presence rather than reading: a terminal parked on a visible room marks each arriving message read, so a marker-only window answered "nothing new" to exactly the person who missed the day. The fix was to stop deriving the window from a cursor that cannot know, not to pad it.
- **Nothing is written on delivery.** The mark is the reader's, not the summary's; a second `/summary` reads the same stretch again (see `The Two Marks`). A failed summary therefore costs nothing but the request.
- **The typed window requires its unit** (`6h`, `90m`; case-insensitive): a bare `6` is refused rather than guessed at, and a window past 48h is refused rather than silently clamped, so the answer is never narrower than the question. Both refusals land as banners before any request goes out.
- **Bounded two ways** before the model sees anything: wall clock (48h) and transcript size (`SUMMARY_PROMPT_CHAR_BUDGET`, 200k chars, newest kept, oldest dropped). The SQL fetch limit is derived from the char budget (`SUMMARY_FETCH_LIMIT`, budget / minimum line size), so it bounds memory without being a third policy knob. The overlay header says when the caps cut messages.
- **`Empty` carries its basis**, because "nothing new since you left" and "nothing said in the last 24h" are different claims, and the banner tells them apart.
- **The head line is dated on the reader's clock.** The overlay opens with `3 messages since Aug 28 16:30 CEST`, formatted by `common/time.rs::instant_for_viewer` from `ChatState.viewer_tz`, a mirror of the account timezone synced by `App::tick` (and at bootstrap) the same way the translate settings are. No account zone, or an unparseable one, keeps the old `Aug 28 14:30 UTC`: the zone is always named, so the window can never be read against the wrong clock.
- **Public rooms only**, enforced in the SQL (`ChatMessage::list_public_room_since`: membership required, `visibility = 'public'`, system lines and ignored users excluded) with a fast client-side banner first. Private rooms and DMs never reach the summarizer.
- **Guardrails**: a per-user-per-room slot, reserved under one lock before any work (a duplicate submit while a request runs collapses into it instead of spending a second fetch, cap slot, and model call) and armed as the cooldown on success only (`SUMMARY_COOLDOWN`, 10 min; failures release the slot so `/summary` is its own retry); a global daily cap (`SUMMARY_DAILY_CAP`, tripping logs the requesting user); a 2-way concurrency gate; and `record_chat_summary` telemetry per outcome. Results are per viewer (everyone's cursor differs), so unlike translation there is no shared cache; the slot is what absorbs repeats.
- The system prompt marks the transcript as untrusted chat content: instructions inside messages are reported, not followed.

### Tail And Delta Recovery

1. Visible-room changes request a tail.
2. Tail checks membership and loads newest 500 messages plus reactions and author metadata. It carries **no** read cursor: `RoomTailLoaded` fans out to every session of the account, so a cursor on it let one device rewrite another's divider.
3. Tail merge dedupes by id, sorts newest-first, truncates to 500, and preserves ignored-user filtering.
4. Broadcast lag requests a visible-room tail reload.
5. Delta sync checks membership and loads up to 256 messages after `(created, id)`.

### Room Membership Commands

1. `/public #room` gets or creates a public topic room, forces `auto_join=false`, and joins only caller. `/join #room` is an alias for it, parsed in the same `parse_public_room_command` so the two can never drift; it exists because IRC users type `/join` first. Public rooms are hosted, not owned: opening one grants nothing, and a brand-new one posts two plain system-bot lines (deliberately without the system-line prefix, so they render as messages), one in the room asking the creator to describe it and one in #moderators reporting it.
2. `/private #room` opens the room-info form (`app/room_info_modal`) and creates the room with its topic/rules and `created_by` in one go.
3. `/roominfo` opens the same form for the selected room. Authority is decided once in `ChatService::set_room_info`: mods for any room, otherwise the derived owner of a private topic room. `ChatState::room_info_authority` mirrors the rule for what the UI offers (and what the refusal banner says); DMs and game rooms have no info at all. A successful write broadcasts `RoomInfoUpdated`, which banners for the editor and refreshes the room list of every session sitting in that room, so no header waits on the 10s snapshot.
4. `/rules` shows the selected room's rules in the shared overlay (`Overlay`, the same surface `/active` uses), titled `#slug rules`, one entry per stored line: rules are multi-line and a banner is one line. A room with no rules answers with a banner instead of an empty overlay.
5. `/kick @user` runs the moderation service's room action (`ModerationService::room_command` then `room_action`), so membership removal, voice removal, audit log and the target's live session behave exactly as from the mod surface. A private room's owner is granted `Caps::KICK_FROM_ROOM` for that one room via `Permissions::as_room_owner`, which leaves the tier alone so staff stay out of reach. Chat-originated room actions name the room by id (`RoomRef::Id`), never by slug: slugs are only unique per namespace (a topic room and a stream room can share one), so the id is the only exact name of the room the actor is sitting in. The mod surface still resolves its typed slug.
6. `/ban @user [duration] [reason]` and `/unban @user` run the same room action with `RoomModAction::Ban`/`Unban`. Staff act by rank anywhere; a streamer additionally holds the `STREAM_OWNER` grant (kick + ban + unban) inside their own stream room only — the full story, including why ban rather than kick and the voice-ticket refusal, lives in `stream/CONTEXT.md` §6. When the grant comes from ownership rather than rank, an *active* ban placed by another actor refuses both the unban and a re-ban, so a streamer can never lift or soften a staff decision on their room; expired bans are history and do not block.
7. `/invite @user` requires caller membership and rejects DMs.
8. `/leave` rejects permanent rooms.
9. Admin `/fill-room #room` works only for public rooms, bulk-adds all users, and sets `auto_join=true`.
10. DMs always preserve canonical endpoints; sending repairs membership for both endpoints.

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

Message search stays cheap by construction: debounce + 3-char minimum + one in-flight latest-wins request per session + `read_permits` + `LIMIT 50` + the migration-114 trigram index. Do not add unbounded result paging; refining the query is the pagination.

Known risks:
- `ChatRowsCache` validity is counter-based (`ChatRowsVersions`); any new mutation path that changes rendered rows must bump the room version or a context epoch, or rows go stale.
- Summary snapshot merge clones preserved message vectors for rooms with empty incoming message lists.
- Unread count SQL counts rows newer than `last_read_at`; if message volume grows, run `EXPLAIN ANALYZE`.
- Tail reload is the recovery path for lagged broadcasts, so keep it bounded and membership-protected.

Do not reintroduce the old per-session "load every room's history every 10s" behavior.

---

## 16. Tests

Repo-wide rule from root context still applies:
- Pure unit tests stay inline under `src/`.
- DB/service tests go in adjacent `_test.rs` files beside the source they exercise (see tree above).
- LLM agents must not run `cargo test`, `cargo nextest`, or `cargo clippy`; note expected commands for the human owner instead.

Existing DB-backed coverage:
- `src/app/announcements_test.rs`: login #announcements loading, read cursor behavior, paging.
- `svc_test.rs`: send, reactions, summaries, room tails, ignored users, discover listing/joining, public room create/fill, delete events, ignore/unignore, message search (membership/game-room/ignored exclusions, room scoping, LIKE-metacharacter escaping, context-window ordering).
- `news/svc_test.rs`: article snapshots, empty list, author resolution, duplicate URL failure, direct DB inserts appearing after list refresh.
- `sheet_test.rs`: character sheet model/upsert plus `open_sheet_task`/`save_sheet_task` room-scoped authorization.
- `showcase/svc_test.rs`: create event/snapshot, non-owner update failure, admin delete, unread cursor behavior.
- `work/svc_test.rs`: profile create/update snapshot behavior, public slug preservation, non-owner update failure, admin delete, unread cursor behavior.
- `state_test.rs`: placeholder; direct `ChatState` tests need accessors or indirect UI/input tests.
- `state_internal_test.rs`: `t` toggle over a cached translation (pending → ready → collapse → reopen, plus the same-script no-op banner), target-language switching dropping stale translations, auto mode firing without a pending placeholder, and author-shared display (shared row by someone else shows with no auto mode or `t`; private rows and the viewer's own shared row stay hidden). Note the harness gotcha these pinned: snapshots carry rooms with **empty** message vectors, so a test needing a concrete message must pull the room tail (`load_room_tail`), not wait for a snapshot.
- `svc_test.rs::gild`: the split landing (two ledger rows, author counts), and one test per refusal (self, DM, private room, game room, bot author, non-member, cooldown, short balance), each asserting the ledger stayed empty; `raises_a_held_gild_at_full_price_and_never_lowers_it` lifts the cooldown to walk one buyer through a raise (full price, no new buyer, no feed line) and both `AlreadyGilded` and `HeldHigher`.
- `late-core/src/models/chat_message_gild_test.rs`: tier roster (prices, split, markers), the one-slot-per-buyer placement (`a_buyers_gild_only_ever_goes_up`), the self-gild CHECK, owner-scoped counts, page summaries, and that only a committed gild notifies `chat_message_gilded`.
- `app/ai/translate_test.rs`: cache-hit service path with AI disabled, the failure path clearing single-flight so `t` can retry, and author-shared rows broadcasting their flag (request and sweep alike).
- `late-core/src/models/message_translation_test.rs`: script detection against each target, language key round-trip, cache upsert/read/cascade-delete, and `author_shared` surviving a later private rewrite.

Existing unit coverage:
- `state.rs`: command parsing, autocomplete ranking, visual order, reply preview/target helpers, DM sort keys, textarea theme behavior.
- `input.rs`: room navigation aliases and reaction leader key parsing.
- `ui.rs`: title fitting, composer title degradation, visible rows, room-list rows, hit testing, scroll helpers.
- `ui_text.rs`: news parsing/rendering, message footer (gild marker leading the reaction chips), wrapping, composer rows.
- Synthetic modules: selection clamp/move helpers, tag parsing, URL validation, payload sanitation, loading transitions.

Test gaps:
- Dedicated notification-service DB-backed tests for mention creation/list/mark-read.
- Direct input-handler tests for News/Showcase/Work/Notifications/Discover.
- Direct `ChatState` synthetic-panel tests.
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
- Ignore filtering covers all rooms including DMs, and also hides bot replies whose `reply_to_user_id` is ignored. DMs with an ignored peer are hidden from the room rail entirely.
- `#announcements` admin-only currently depends on the provided `room_slug`; stale/missing slug is a fragile path.
- Login `#announcements` modal marks `chat_room_members.last_read_at` only when dismissed; do not add a separate announcement-read table unless the room model itself changes.
- Reaction tasks are async; UI should not assume optimistic success.
- A gild marker repaints off the Postgres notify, not off `evt_tx`. If `start_gild_listener_task` is not running (tests, or a process wired without it) the marker only appears on the next room tail load. Do not "fix" that by broadcasting locally as well: two paths would mean two repaints and a marker that behaves differently on the replica that sold it.
- Poll create/vote tasks are async; `ChatEvent::PollUpdated` patches the local active-poll map and `ChatSnapshot.active_polls` refreshes authoritative visibility. Successful poll creation spawns a sleep-until-expiry finalizer that atomically claims the expired poll in Postgres, marks it inactive, and posts compact results into the room as the poll creator. `ChatService::start_poll_finalizer_recovery_task` runs a coarse 10-minute recovery scan for expired active polls so restarts/redeploys do not strand result posts; the DB claim is the cross-replica duplicate guard.
- Poll vote shortcuts use `va/vb/vc` when the selected/visible real room has an active poll, leaving music `v1/v2/v3` selectors available.
- Room visual order must stay consistent between state and UI hit-testing/row-building.
- Mouse hit-testing reconstructs a temporary `ChatRenderInput`; room-list layout changes must keep hit tests in sync.
- Chat-scroll mouse hit-testing is driven by `ChatRowsCache` extras (`row_message`, `row_kind`, `header_segments`) and a per-frame `ChatHitLayout` published into `ChatState::last_chat_hit_layout`. If you change how author headers, inline images, or reaction footers contribute rows in `ensure_chat_rows_cache` / `wrap_chat_entry_to_lines`, update both the parallel `row_*` vectors and the segment math in `build_author_prefix_and_segments` so a click still resolves to the right message/segment.
- News payload fields must sanitize the separator and newlines.
- Showcase and Work posts do not create chat messages; News posts do.
- Game rooms must remain opt-in and `auto_join=false`.
- Private `kind='game'` rooms (daily match chat) are membership-fixed at creation; no join path may admit a third user, and they stay hidden from the rail/Mentions/IRC like all game rooms. The daily sweeper hard-deletes them 30 days after the match ends.
