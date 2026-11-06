# Chat input: composer, commands and keys

The chat composer (reply, edit, mentions, autocomplete), every slash command it accepts (`commands.rs`, `ChatState::submit_composer`), the `/mod` modal's command set, and the keybindings. Part of the chat domain; the rest is `CONTEXT.md`.

## Composer, reply, edit

The main composer is a `ratatui_textarea::TextArea<'static>`.

`composer_room_id` is the authoritative send target while composing. This matters because Home and the embedded game surfaces do not necessarily drive `selected_room_id` in the same way.

`/me <action>` stores a CTCP-style action body through `chat/action.rs` and renders locally as italic `* name action`; IRC delivery unwraps it into the same readable action text. Keep new action handling on the shared helpers so TUI and IRC stay aligned. Editing an action line puts `/me <action>` back in the composer, and a `/me` submitted while editing rewrites that message (`edit_message_task`) instead of posting a new one.

`/gift @user <chips>` transfers chips through `ChipService` and `late-core::models::chips::UserChips::transfer_gift`. The transfer is one transaction: sender debit, recipient credit, two ledger rows each carrying the other party's id as `source_ref`, and chip notifications. It enforces the chip floor, rejects self-gifts, caps gift size, and applies a short per-sender cooldown in `ChatService`. Neither gift reason counts on Top Chips.

`/buy round` and `/buy @user` post `@bartender round for everyone` / `@bartender buy @user a drink` to the composer's room as a normal message (`buy_order_body`), so the purchase, its price, every refusal and his answer run through the bartender exactly as if the phrase had been typed (see `ai/ROUND.md`). Only in auto-join public rooms (`visibility == "public" && auto_join`), refused with a banner elsewhere: those are the rooms he is joined to, and in any other room the order would go unanswered (he leaves a mention in a room he is not in alone).

`/grant @user <chips>` is the admin mint. `parse_grant_command` takes a user and an amount and nothing else; the composer refuses it for non-admins, and `ChatService::grant_chips` decides admin by the session bootstrap's own rule, `users.is_admin || force_admin`, re-read from the database rather than trusted from the session (the `force_admin` half is what makes local and staging accounts admins). The credit goes through `ChipService::grant_chips` to `UserChips::admin_grant`, which by decision writes **no ledger row**: the ledger records what players did, and a grant is the house's doing. The recipient's row is ensured first, so a player who has never logged in lands on the stipend plus the grant. Both parties get a banner (`ChatEvent::GrantSucceeded` / `GrantFailed`).

`/members` renders a styled overlay with online members first, offline members second, each group sorted alphabetically. Preserve the fixed status-cell shape so overlay rows do not jump as online state changes.

The rows carry `OverlayInk` (`common/overlay.rs`), never colours. The overlay is built in `drain_events`, which runs during the session tick, and `theme`'s palette lives in a thread local that `App::render` sets afterwards, on whatever worker thread the session woke on: styling the spans here painted the member list in whichever session last rendered on that thread. `draw_overlay` resolves ink inside the draw. `app/paper` had the same bug and the same cure.

Profiles page 5 reads the Work and Showcase feed states from chat and writes through their `save`/`delete_*` calls; the forms themselves are the profile editor modal in `app/directory/editor`. The page's search (`s`) is independent of Home room search.

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
- Edit calls `edit_message_task`. A News share's #lounge card is never edited (`news/CONTEXT.md`, the #lounge card): the edit key banners `A story's card can't be edited.` and `ChatService::edit_message` refuses it too (a line linked as a card after its edit began). There is no edited flag: a message counts as edited when `updated > created`, which appends `(edited)` to the author header's stamp. Because of that, an edited message never groups as a continuation under the message above it (`ensure_chat_rows_cache`): it takes its own author header back so the marker has somewhere to live. Editing the second message in a run visibly breaks the run; that is the intent.
- An image upload started from the composer carries its reply target with it (`PendingUrlUpload`/`PendingClipboardImageUpload`, then `image_upload_reply_target`), because both `/upload` and `/paste-image` clear the composer at submit and the finished URL reopens it through `start_composing_in_room`. `tick.rs` restores the target after reopening, and drops it on a failed upload.
- Enter submits and closes.
- `Alt+S` submits and keeps the composer open.
- The `keep_composer_focused` Tweaks setting flips Enter to behave like
  `Alt+S` (send and stay) and disables the `Alt+S` binding while on; the
  composer title hint and Chat help section collapse to match.
- `Alt+Enter` and `Ctrl+J` insert a newline in the main chat composer.

The slash commands (user, runner, admin) and the `/mod` modal's command set are listed in `COMMANDS.md`.

Reply mode:
- Captures `ReplyTarget { message_id, author, preview }`.
- Enters compose mode and clears edit.
- On submit, stores `reply_to_message_id` and prefixes the stored body with a visible quote line for backward-compatible rendering.
- Enter on a selected reply jumps only if the target is already loaded in the current room tail.
- `G` on a selected reply also jumps to the loaded target. Enter is overloaded (an inline image opens first, then a YouTube link plays), so a reply whose own text carries either can only be followed with `G`, not Enter. A link inside the quote line does not count: it is the parent's, so Enter jumps there and a second Enter opens it. Lowercase `g` is the gild key.

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
- Pressing `/` while not composing on Home starts command compose for the active room, except on News where `/` is a synthetic-entry filter toggle. Directory Profiles/Projects use `/` as the mine-only filter inside page 5.

Image uploads and inline rendering:
- File-upload storage is optional per profile: `Config.files` is `Some(FilesConfig)` in prod (endpoint/bucket/URL literals plus `LATE_FILES_S3_ACCESS_KEY_ID`/`LATE_FILES_S3_SECRET_ACCESS_KEY` env secrets). Dev is `None` unless both R2 credentials are set in `.env.local`, which opts uploads into the prod bucket; a half-set pair is a startup error.
- Pasting raw PNG/JPEG/GIF/WebP bytes into the chat composer starts an upload because there is no stable URL to preview until the bytes are hosted.
- Pasting an image URL does not upload or rehost it. It is inserted as normal composer text; after send, inline rendering previews that URL best-effort.
- `/upload <url>` is the explicit URL upload path: it downloads a public image URL server-side, reuploads it to configured public file storage, and inserts the resulting URL into the composer for the user to send and preview.
- `/paste-image` is the explicit paired-CLI clipboard path. It requires an updated `late` paired client, not plain `ssh`.
- Non-admin uploads use a per-session `ChatState` cooldown. This is intentionally lightweight, not a server-side quota.
- URL downloads for upload and inline rendering must go through `files::image_upload::download_url_bytes`: validate `http(s)`, reject localhost/private/link-local/reserved resolved IPs, pin reqwest DNS to the validated addresses, disable redirects, and stream with a hard byte cap. Do not add new ad hoc `reqwest.get(url).bytes()` paths for chat images. Fetchers that must follow redirects (RSS feeds) use `download_url_bytes_following_redirects`, which re-validates every hop.
- Paired clipboard uploads are request-gated: `PairedClientRegistry::request_clipboard_image` records an outstanding request per token, and the pair WS handler drops any inbound `clipboard_image`/`clipboard_image_failed` payload whose token has no outstanding request (`take_clipboard_request`). This keeps a rogue paired client from queuing multi-MB decoded images into the bounded per-session channel.
- A YouTube video link gets the same inline preview, drawn from its thumbnail (`audio::thumbnail::thumbnail_url`, `i.ytimg.com/vi/<id>/mqdefault.jpg`) with the video's title on a row above it and a `▶ YouTube · Enter to watch` row under it: the title comes from YouTube's public oEmbed endpoint (`audio::youtube::fetch_oembed_title`, through `download_url_bytes`), fetched with the thumbnail and cached with it as `state::InlinePreview::Video`, and cut to the picture's width; when oEmbed names nothing (a private or deleted video) the title row is left out. Each session fetches its own, like the thumbnail; nothing is stored. An image link in the same message takes the preview instead (`state::inline_preview_source_in_body`; both scan the body through one `body_links`, and the video id is the player's rule, `video::state::youtube_video_id`). A reply's quote line is the parent's text, so a link there previews nothing (`state::body_without_reply_quote`). Enter on the message or a click on the thumbnail opens the watch player on any chat surface (`input::watch_selected_video`, `../video/CONTEXT.md`), the setup box when the session cannot play.
- Inline image rendering detects likely image URLs in visible room messages, fetches them through the same secure downloader, rejects oversized decoded dimensions, retries transient failures with backoff, and caches an `InlinePreview` by message id (an image's picture, or a video's thumbnail and title). Inline previews are only the RGB block fallback used by scrolling chat rows. Kitty/iTerm2/Sixel native image data is fetched separately, lazily, only while the explicit selected-message image modal is open on a supported terminal. Inline previews are best-effort; failures are intentionally silent/noisy only at trace level.
- Kitty, iTerm2, and Sixel image support is intentionally narrow and modal-only. `files::terminal_image` detects Kitty-family terminals from PTY `TERM`, XTVERSION, and forwarded env hints: Kitty, Ghostty, Rio, Warp, and Konsole. It detects iTerm2-family support from `TERM_PROGRAM`/`LC_TERMINAL`, XTVERSION, `TERM_FEATURES`, `OSC 1337;Capabilities`, and env hints for iTerm2, WezTerm, mintty, and hterm-style identities. It detects Sixel from explicit identities (`windows terminal`, `foot`, `contour`, `mlterm`, `sixel`), `WT_SESSION`/`WT_PROFILE_ID` env hints, and DA1 (Primary Device Attributes) replies advertising attribute 4 — the DA1 probe is sent last at alt-screen entry and only fills in Sixel when no richer protocol was detected, so Kitty/iTerm2 always win over Sixel. If `TERM` is tmux, full image previews are intentionally disabled and chat uses the RGB block fallback; no tmux graphics passthrough is attempted. Unsupported or undetected terminals, including stock Alacritty, keep the RGB block preview. The Settings → Tweaks "Terminal images" row (`TerminalImagesMode`, user setting `terminal_images`) sits on top of all of that: `Auto` uses the detected protocol, `Off` forces the RGB block preview, and `Sixel` forces Sixel even where nothing was detected (tmux included), for terminals that misreport. Detection writes `App.detected_image_protocol`; every reader goes through `App::terminal_image_protocol()`, which applies the tweak live. Kitty images use late.sh-owned ids in the `0x4C000000..0x4CFFFFFF` range plus a dedicated z-index so cleanup can target them by range/z-index as well as by visible placement. Sixel payloads are generated only for Sixel sessions, use adaptive palette fallback, and fail back to the RGB block preview if the final payload still exceeds the hard byte cap. Because Sixel has no terminal-side scaling, the image modal reports its image cell capacity into `TerminalImageFrame` during draw, the render loop feeds it back into chat state, and Sixel fetches encode to fit that capacity (first fetch is deferred one frame after the modal opens until capacity is known; a cached Sixel encode that no longer fits, e.g. after shrink, is re-fetched at the new capacity). A forced repaint resets terminal image placement state so modal images are re-emitted after clear/resize/drop recovery. Direct terminals get Kitty cleanup commands on enter/leave alt-screen. Alt-screen enter/leave and forced full repaint begin with an ST terminator so a killed session that left iTerm2/Sixel inside an unterminated DCS/OSC image payload can recover before normal clear/repaint bytes. Closing an iTerm2 or Sixel image modal forces a full repaint because those inline images are not tracked/deleted like Kitty placements.

## Slash commands

User commands:
- `/active` opens an overlay from in-memory `active_users`, including repeated-session counts.
- `/friend @user` privately marks a user as a friend; `/unfriend @user` removes the mark; `/friends` lists marked users.
- `/binds` opens the Chat help topic.
- `/cs` (alias `/cyberspace`) opens the Cyberspace `feeds` entry; `/cs post` opens its compose modal, `/cs chat` (alias `/cs rooms`) the chat-room picker that adds rooms as rail entries, `/cs mail` the C-Mail picker that pins conversations the same way, `/cs mail @user` starts (or finds) a conversation, pins it, and walks into it, `/cs link` the account-link modal, `/cs unlink` forgets the link. Parsed in `submit_composer` (`parse_cyberspace_command`), handled inline on `ChatState` (no `take_requested_*` plumbing; `pending_chat_screen_switch` pulls the user to Home).
- `/aquarium feed` (alias `/aq feed`) feeds the Shop-unlocked tank, which lives on the Zen page and in the opt-in sidebar Tank panel (the sprout is cut on its Shop row; there is no `/aquarium cut` and no Home tray); bare `/aquarium` answers with a usage banner. Parsed in `submit_composer`, drained via `take_requested_aquarium_command` in `handle_post_submit_requests`.
- There is no `/pet` command: the pet is fed by nothing and toggled by nothing. `ChatState::last_own_send_at` (stamped on `SendSucceeded`) is the pet's "chatty" signal, read by the tick; `/petname` stays.
- `/dm @user` opens/creates a DM.
- `/exit` opens quit confirm.
- `/golive [title]` registers this user's "watch me" stream (`/golive stop` ends it) and `/watch @user` opens a live stream. Both are parsed in `submit_composer` (`parse_golive_command` / `parse_user_command`) and drained by `App::tick_stream`, which owns the stream service, the publisher URL modal, and the paired-CLI `open_url` control; the domain contract is `late-ssh/src/app/stream/CONTEXT.md`.
- `/pot` prints the weekly pot's size, the tickets in it, what you hold and what it cost, how many more you may buy today, and the time to the draw; `/pot buy N` buys N tickets. Parsed in `submit_composer` (`parse_pot_command`, which is also the boundary that rejects any count outside `1..=10`, the daily cap) and drained by `App::tick_pot`, which owns the pot service and the banners. The status line is answered straight from the process-shared snapshot, so `/pot` costs no query; the domain contract is `late-ssh/src/app/pot/CONTEXT.md`.
- `/icons` opens the icon picker (same as `Ctrl+]`).
- `/picker` opens the room picker (same as `Ctrl+/`); drained via `take_requested_room_picker` into `open_room_search_modal_globally`. On Zen with a chat tile focused, the pick binds that tile (see the Zen CONTEXT).
- `/poll` opens a modal for the currently visible real room. Polls are room-scoped, support two or three options, can run for 10, 20, or 30 minutes, and are limited to one active poll per room. Active polls render at the top of the room message pane; while one is visible, `va`, `vb`, and `vc` vote for poll options. `v1`, `v2`, and `v3` remain music stream/station selectors. Failed starts show the remaining active wait in the banner.
- **Away is automatic; there is no status command.** A user reads as away (`💤`, the glyph alone, never a word) once every session they have open is away (`common/away.rs`). A TUI session goes away after `AWAY_AFTER` (30 min) with no input into `App::handle_input` (any byte counts, a mouse move included), or at once with `/brb`; the next input brings it back. An IRC connection goes away after 30 min without a PRIVMSG or NOTICE, or on `AWAY :msg` until a bare `AWAY` (see `ircd/CONTEXT.md`). A user with no sessions at all (the always-on ghost bots) is never away. The glyph must stay disjoint from every purchasable badge and carry no U+FE0F; `away_test.rs` scans the seed migrations for both.
  - **Where the flag lives:** `ActiveSession.away` on the process-wide `active_users` roster. `App::sync_away` recomputes it on the 1Hz edge and writes the roster only when it flips, so nothing is persisted and nothing is queried. The same edge resolves the roster into `App::away_user_ids` (`away::away_user_ids`), and bumps `chat_ctx_epoch` only when that set moves.
  - **Where it paints:** the chat author line (trailing presence badge, after LIVE), the DM rows of the room rail (`dm_display_label`), @-completion (`MatchPresence`: here, then away, then offline, with the glyph beside the name), `/active` (`format_active_user_lines`), the sidebar friends row and the Zen friends tile (`ActiveFriend.away`, away friends sorted last). The Ctrl+/ room picker does not paint it: its labels are also its search keys.
  - This is not the AFK line (`AFK_LINE_IDLE`, 10 min): that marks where *you* stopped reading and only you see it. Away is what everyone else sees about you. Both read `App::last_input_at`.
- `/roll [NdM ...]` rolls dice into the current room; bare `/roll` defaults to `d20`, caps are 100 dice per group and 1000 sides.
- `/search [query]` opens the Ctrl+/ modal in message-search mode, pre-filled with `?query`. Parsed in `submit_composer`, drained via `take_requested_message_search` in `handle_post_submit_requests` (the modal is App-owned).
- `/summary` asks the AI for a catch-up of the visible public room, from when you last left the app on this device (24h when the device has no mark), or exactly the window you type (`/summary 6h`, `/summary 90m`, up to 48h); see `../ai/CONTEXT.md`, Summary. `/history` opens the scroll-back modal, at the first message you missed when this session has an AFK line for the room; see `CONTEXT.md` §14 History Modal.
- `/paper` opens The Late Edition, @graybeard's daily paper (`app/paper`, App-owned modal); `/paper YYYY-MM-DD` opens an older edition if one was printed (`←`/`→` inside the paper leaf between editions, all in `app/paper`); `/paper print|preview|reset` are admin-only and banner for anyone else. Parsed in `submit_composer` into `requested_paper`, drained by `paper::svc::tick`.
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
- `/brb [reason]` sends this session away now instead of after 30 quiet minutes; the next key brings it back. Bare `/brb` posts nothing and works from anywhere. With a reason it first posts `🌙 brb: <reason>` to the composer's room. Only the composer's room, like `/me`: with none it banners `Use /brb <reason> from inside a room` and does not go away, rather than posting into a stale visible or selected room. The post is a normal send through `send_message_with_reply_task`, so a failed send (e.g. a non-staff `/brb` in a report-only room) shows the usual send notice while the session still goes away. It does not mute audio. Parsed in `submit_composer`, drained via `take_requested_brb` into `App::sent_away`. See away above.
- `/bug <text>` and `/suggest <text>` post a report card into `#bugs` / `#suggestions` regardless of the composer's current room (`ChatService::send_report_task` resolves the room by slug and joins the caller first). A report is a normal chat message whose body starts with `ReportKind::marker()` (`---BUG---` / `---SUGGESTION---`), so reactions, replies, pins, and deletes work unchanged; `ui_text::wrap_report_to_lines` renders the card. Text under 10 chars (`REPORT_MIN_CHARS`) banners usage instead of posting. Because the card lands out of sight, the submitting session banners `Posted to #bugs` / `#suggestions` on success (`pending_report_notices`); every other successful send has no banner, since its echo in the room is the confirmation. Those two rooms are report-only: `send_message` refuses free-text sends from non-staff (`SendRefusal::ReportOnly`, covers IRC too since it checks the DB slug; an edit that strips the marker fails the same way), while admins/moderators keep plain text so they can reply under a report; everyone keeps reactions ("+1"). The staff-flag DB lookup runs only on that rare gated path. Only the public topic rooms count (`ReportKind::for_room`, shared by the send gate, the edit gate and the composer): topic slugs are unique per visibility, so a private room named `#bugs` stays free text for its members. The TUI composer meets regular users before that gate (`ChatState::report_room_kind`): plain text typed in either room is filed as if typed after `/bug` / `/suggest`, while a reply there, or text under `REPORT_MIN_CHARS`, is refused on submit with the composer kept, so nothing typed is lost. Edits and slash commands pass through untouched.
- `/shrug` posts `¯\_(ツ)_/¯` to the current room. No arguments; sent through the same `send_message_with_reply_task` path as `/me` and `/coffee`, with a banner error when the composer isn't in a room.
- `/coffee` and `/tea` post a small ASCII-cup chat message to the current room as a coffee/tea-break ritual. No arguments. Steam pattern rotates per invocation through `CUP_VARIANT_COUNT` variants tracked on `ChatState::next_cup_variant` (session-local, not persisted). Routes through the normal `send_message_with_reply_task` send path — the body is a regular chat message subject to the same length/visibility rules.
- `/private #room` creates a private topic room and joins the caller.
- `/profile [@user]` opens a user's read-only profile modal. Bare `/profile` opens the caller's own profile as others see it. `@username` autocompletion is available after `/profile `. `/chips [@user]` is the same modal opened on its chips ledger (`ProfileSection::Chips` rides on `OpenProfileResolved` and the `requested_open_profile` handoff; the modal scrolls there on its first measured draw).
- `/onboard` starts the first-visit tour again from the tavern door (`app/clubhouse/CONTEXT.md` §5).
- `/public #room` (alias `/join #room`) opens or creates an opt-in public room for the caller only. A new room starts with `auto_join=false`; an existing room keeps its flag.
- `/sheet [@user]` (room-scoped to `#dnd`) opens the character sheet modal: bare form opens your own sheet editable (name + freeform body, saved per user per room on field submit via `ChatService::save_sheet_task`); targeted form opens another user's sheet read-only, or banners if they have none. Resolution and fetch happen in `ChatService::open_sheet_task`; saves and reads validate the shared `RoomScopedCommand` metadata plus room membership in `ChatService::ensure_room_scoped_command_access`; the modal lives in `app/sheet_modal`.
- `/settings` opens settings.
- `/shop` opens the Shop modal, as does Ctrl+S outside profile/job editors and active Artboard input. Locked-feature nudges use the same entry point.
- `/unignore [@user]` removes an ignored user.
- `/upload <url>` downloads a public image URL server-side, reuploads it to configured public file storage, and inserts the resulting URL into the composer for the user to send.

Runner commands:
- `/hit` hits the glyph in the scene above #lounge's composer (`deadchannel/lounge`).
  Parsed in `submit_composer` **only when `is_runner`** (the flag the
  lounge tick keeps from the runner directory; parser in
  `deadchannel/lounge/state.rs`): swallowed and never posted for a
  runner, whatever is there to hit; for anyone else the line posts as
  plain text, and the room wondering is the point. Sets `requested_hit`,
  drained by `deadchannel::lounge::svc::tick`. The spawn itself is
  chrome above the composer (`DashboardChatView.lounge`, three rows
  between the ticker and the composer that open while one shows; the
  composer block itself never changes), never a message.

Admin commands:
- `/haunt [arm|glitch|name|replay|invite|reset|welcome]` controls the
  first-contact haunting. Parsed in `submit_composer` **only when
  `is_admin`** (enum + parser live in `deadchannel/haunt/state.rs`): for
  everyone else, moderators included even though the ladder runs
  for them, the line posts as plain text, so the command does not
  observably exist (no usage banner, no autocomplete entry, no help
  line - the mystery is the feature). Drained by
  `deadchannel::haunt::svc::tick`. Chat's other haunting seams: the
  `own_message_landed` slot `push_message` records for the stage-2 name
  flicker (the message id and its room, since a won hit is put on the
  wire for that room), `ChatService::publish_name_hit` (the `pg_notify`
  on `deadchannel_name_hit`, fire and forget), `ChatEvent::NameHit` off
  the message listener into `note_name_hit`, which hands the beat over
  at once if the message is on screen and otherwise holds it in
  `pending_name_hits` for `push_message` to promote (a beat from another
  replica can arrive seconds before the room delta brings its message;
  one that never lands ages out after `NAME_HIT_WAIT`, 30s), the
  `witnessed_hit_landed` slot the haunting drains, `name_flicker` in the
  rows-cache key,
  `ChatService::send_first_contact_invitation_task`, and the runner:
  `join_deadchannel_room` creates the `deadchannel_runners` row with a
  random starter look, and `ensure_chat_rows_cache` takes
  `runner_looks: Option<&HashMap<Uuid, RunnerEntry>>` (`Some` only while
  `state::room_shows_portraits` holds for the rendered room, today
  `kind='deadchannel'` alone, the one switch to widen; from `DashboardChatView` /
  `ChatRenderInput.runner_looks`, the app's 1 Hz copy of the runner
  directory, look and level per standing runner): a runner's author
  header opens its badge stack with the level badge (`▚7`, the mark and
  the level, `AuthorPrefixInput.runner_badge`, painted in the level's
  band through `AuthorTint.runner`, adjacent to the name and
  title so the same painter tints it), the wire wraps every entry six cells short and seats the
  author's three-row portrait in that gutter beside a block-opening
  message: the hood level with the header, the eyes and the coat on the
  body rows under it, wearing what the block has rows for (a one-liner
  the head only, its coat seated on a continuation right under it when
  one follows, a taller message the coat itself, so no message grows a
  row for its face; `attach_portrait` / `seat_portrait_row`). The blank
  separator above the block stays blank and belongs to nobody, so two
  faces stacked down the wire never touch; the mention wash and the jump
  highlight cover the entry's rows. No other room changes. The domain
  contract is `late-ssh/src/app/deadchannel/runner/CONTEXT.md`.
- `/create-room #room` creates a permanent auto-join room and bulk-adds existing users. It is idempotent on rooms that are already permanent, and it promotes an existing non-permanent public room to permanent + auto-join (`ChatRoom::ensure_permanent` UPDATEs the row, then the caller bulk-adds users) — this is how a user-created `/public #voice` room becomes the permanent `#voice` core room. Because promotion bulk-adds every user to a room nobody can leave, `/create-room` is admin-only and a mistyped slug will promote whatever public room matches it.
- `/delete-room #room` deletes a permanent room.
- `/fill-room #room` bulk-adds all users to an existing public room and flips `auto_join=true`; private rooms cannot be filled.

Moderation modal commands (default help groups them under Lounge, Artboard, Bans/kicks, and Help/Admin):

The centered popup uses 80% of the available width and 93% of its height.

Help uses bright command names, muted subcommands, shaded argument tokens with
fainter brackets, pipes and ellipses, and emphasized section titles. A line is styled as a command only when its
first word is in `ModCommandHead` (`moderation/command.rs`), the roster the parser
dispatches on. Initial help
and successful help command responses have their own log kind; request IDs keep that styling tied to the correct
response even when commands complete out of order. Other success/error output
retains its usual colors, and help colors follow the active theme at render time.

- `rename-room <#oldname> <#newname>`
- `rename-user <@oldname> <@newname>`
- `view <@user|#room|bans|slows|audit|art|help> [pagenumber]`
- `artboard curate <live|YYYY-MM-DD> [reason...]`
- `artboard restore [YYYY-MM-DD] [reason...]`
- `artboard remove <piece-id-prefix> [reason...]` (takes a gallery piece down; the first 13 characters of the id are printed on the key line of the piece's full-frame view, 8+ are needed, must match one piece)
- `artboard feature <piece-id-prefix>` (pins a piece as today's Sliding Puzzle art at once, `ArtboardPiece::feature_now`; the day's previous holder returns to the queue; same prefix rule as remove)
- `deadchannel remove-poster @name [reason...]` (takes a runner's poster from PRINTS off their terminal and web profiles, `DeadchannelRunner::clear_poster_any`, a leaver's too; the profile shows the test card again; `REMOVE_DEADCHANNEL_POSTER` cap, moderators and up, audited as `deadchannel_remove_poster`)
- `room-voice <#room> <on|off>`
- `kick <server|voice|stream|#room> @name [reason...]`
- `ban <server|#room|art|audio|stream> @name [duration] [reason...]`
- `unban <server|#room|art|audio|voice|stream> @name [reason...]`
- `slow <server|#room> @name <interval> <duration|perma> [reason...]`
- `unslow <server|#room> @name [reason...]`
- `admin`
- `admin grant mod @name`
- `admin revoke mod @name`

Moderation list pages show 15 rows. Durations use positive `s/m/h/d` suffixes.
`artboard` remains accepted as the long spelling of the `art` view/ban scope,
and `permanent` is also accepted for `perma` slow-mode duration.

## Keybindings

### Home Chat Center

| Key | Action |
|-----|--------|
| `h` / `l` / `left` / `right` | Switch room/synthetic selection |
| `Ctrl+N` / `Ctrl+P` | Next/previous room |
| `Ctrl+H` / `Ctrl+L`, wheel over the rail | Scroll the room rail without changing room (not while composing) |
| `Space` | Room-jump mode |
| `j` / `k` / arrows | Move message selection or synthetic-list selection; inside a selected message taller than the pane they scroll it by rows first (`CONTEXT.md` §9) |
| `Ctrl+D` / `Ctrl+U` | Approximate half-page message selection |
| `i` | Start composing in selected room, or start News composer when selected |
| `/` | Start command composer in selected room |
| `Enter` | Submit composer; jump reply target; open the reader in News; join Discover; jump Mention; on a selected message, open its image, else watch its YouTube link (`../video/CONTEXT.md`) |
| `G` | Jump a selected reply to its loaded original, even when the reply contains an inline image or a YouTube link (Enter opens the image or watches the video instead) |
| `Alt+Enter` / `Ctrl+J` | Insert newline in main chat composer |
| `Ctrl+R` | Redo in the main chat composer (ratatui-textarea's keymap via `ctrl_byte_to_input`). An open composer takes it ahead of the global repaint chord in `app/input.rs::handle_reserved_global_chord`; `/redraw` repaints from inside one |
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
| `f` then `w` | Wave hi (👋) on selected message |
| `f` then `0` | Open icon picker for a custom reaction |
| `f` then `f` | Open reaction-owner overlay |
| `Ctrl+]` | Open icon picker; inserts only into main chat composer |
| Double-click composer bar | Enter compose mode (same as `i`). Dashboard + embedded game chat only. |
| Click message body | Move message selection to that block (same as `j`/`k` landing on it). |
| Double-click message body | Reply to that message (same as `r`). |
| Click username (or special / friend / bonsai / monthly award / brb badge) | Open that author's profile modal. Debounced ~280 ms so a fast double-click can promote to a mention instead. |
| Double-click username | Insert `@username ` into the composer for the current room. Cancels the debounced profile-open. |
| Click equipped chat-shop badge | Open Hub Shop on the Badges sub-store. |
| Click inline image preview | Select the message and open the image viewer modal; on a YouTube thumbnail, the watch player. |

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
| `Enter` | Jump selected reply target when loaded |

### Synthetic Entries

| Entry | Keys |
|-------|------|
| News | `j/k` navigate, `i` paste URL, Enter open the reader (submit while composing), `s` save for later or let go, `c` copy URL, `r` reply in #lounge to the selected article, `w` watch a video story, `d` delete own/admin article, `/` toggle filter to mine, `Esc` cancel |
| News reader | `j/k`/arrows/wheel scroll, `Space`/`b` page, `g`/`G` top/bottom, `1`-`9` copy link `[n]`, `c` copy the story's link, `s` save for later or let go, `r` reply in #lounge, `w` watch a video story, `d` delete own/admin, `N` select it in News, `Esc`/`[x]` close |
| Directory Projects | `j/k` navigate, `i` create, `e` edit own/admin, `d` delete own/admin, Enter copy/submit, Tab cycle fields while composing, `/` toggle filter to mine, `Esc` cancel |
| Directory Profiles | `j/k` navigate, `i` create/edit own, `e` edit own/admin, `d` delete own/admin, Enter/`c` copy public profile link, Tab cycle fields while composing, `/` toggle filter to mine, `Esc` cancel |
| Cyberspace feeds | `j/k` navigate, Enter open thread (or link modal when unlinked), `p` post, `c` copy entry link, `n` notifications, `r` refresh/reply, `b` back |
| Cyberspace pickers (`/cs chat`, `/cs mail`, command only) | `j/k` move, Enter pin the room or conversation into the rail section or take it off, Esc close |
| Cyberspace room / C-Mail conversation (a rail entry) | `j/k` scroll, `g` newest, `i`/Enter focus the composer (the normal chat composer slot, titled with the room), Enter send, Esc back to reading then leave, `b` leave |
| Mentions | `j/k` navigate, Enter open the Ctrl+/ single-message preview (Enter again jumps to the room) |
| Discover | `j/k` navigate, Enter join selected public room, `/` open slug filter (type to narrow, Enter join, `Esc` clear) |

Directory Projects and Profiles reshuffle their listing on page/tab entry. News lists the reader's saved stories first, then the feed in chronological order without them; only mine-only filtering applies (`news/CONTEXT.md`). The slash-command composer in `app/input.rs` skips itself when News is selected so `/` reaches the synthetic-entry handler; Directory page 5 routes `/` directly to Projects/Profiles filtering.

When changing keybindings, follow the keybinding checklist in `late-ssh/CONTEXT.md`.
