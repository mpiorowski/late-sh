# late-ssh Live Strip Context

## Metadata
- Domain: the live strip, one thing happening in the house painted at the top of the Home #lounge card, with keys to join it, and the Live panel, the list of everything joinable, on the right sidebar and in a Zen Live tile
- Primary audience: LLM agents working in `late-ssh/src/app/live` and the sources that feed it
- Status: Active
- Parent context: `../../../../CONTEXT.md`

---

## 0. The rules at a glance

A queue with two lanes, and one overlay. Every clock is a database stamp or the wall clock, so every session on every replica sees the same thing in the same order (the aim is the one exception, §8).

**Nothing is filtered by who is looking.** A source's candidates never depend on the viewer: your own match, your own stream, your own door game are offered exactly as anyone else's. The same holds for the Games hub rail's `live` rows (`LiveGamesService::live_rows`: every roster entry whose name is a handle, which is what a late.sh player's always is). A candidate drops out only because the thing itself is not live (a pending stream, a match result past its horizon).

**The queue** (`pick_queued`, `replay`):
1. Two lanes, first come first served: **News** (shared links) and **Rest** (match moves, match results, booth tracks, streams going live, door games starting). When the strip hands over, it takes the oldest link if one is waiting, else the oldest of the rest.
2. Whatever is up stays **at least its minimum**, counted from when it went up.
3. After its minimum it hands over as soon as anything is waiting. With **nothing waiting it stays up to `LIVE_MAX_UP` (5 min)** from when it went up, then the strip comes down.
4. Anything that waited in its lane longer than **`LIVE_MAX_WAIT` (10 min)** is dropped, so a busy evening never builds a backlog.

**The overlay** (`aim_overlay`): while a pool shooter moves the cue (an aim inside `LIVE_AIM_WINDOW`, 8s), that table is drawn over whatever the queue has up, or over an empty strip, **never over a link**. It does not touch the queue: the clock of what it covers keeps running, and the queue carries on when the cue is still.

| Source | Lane | Joins its lane | Minimum | Glows | `o` / click | `r` (card only) |
|---|---|---|---|---|---|---|
| `NewsArticle` | News | the share (`created`) | `LIVE_NEWS_MIN` (5 min, equal to the max, so exactly 5) | never | the article modal | reply in #lounge quoting the title |
| `DailyMatch` | Rest | the claim, and again on every move (`updated`) | `LIVE_MATCH_MIN` (1 min) | while aimed | the board | nothing |
| `DailyResult` | Rest | the finish (`finished_at`) | `LIVE_MATCH_MIN` (1 min) | yes | nothing (the board left the lobby) | nothing |
| `BoothTrack` | Rest | when it was queued, not when it starts playing | `LIVE_TRACK_MIN` (2 min) | while it is the one playing | tune in to YouTube, or the booth if already there | nothing |
| `Stream` | Rest | when its media first flowed (`went_live_at`), never at `/golive` | `LIVE_STREAM_MIN` (2 min) | yes | the streamer's room on Home | nothing |
| `DoorGame` | Rest | when the game started on its door host (`started_unix` on the roster) | `LIVE_DOOR_GAME_MIN` (2 min) | yes | the open watch on the Games screen | nothing |

The cases, in words:
- **A link is shared** while a track is up: the track keeps its two minutes, then the link goes ahead of everything waiting in Rest and stays five minutes.
- **A match moves again.** Its entry is its newest move, so it joins the back of Rest again; with nothing waiting, it goes straight back up and its five minutes restart. A session showing it keeps it for its minimum even though the replay's history moved (`pick_queued`).
- **A YouTube track.** Every track playing or waiting in the booth is an entry, stamped when it was queued. Queue three and each takes at least two minutes in order, reading "#3 in line", "up next" or "playing now" as it stands. A track that starts playing long after it was queued does not join again.
- **A door game starts.** Somebody launches DCSS, NetHack or Brogue: it joins Rest once, stamped with the host's start time, and takes at least two minutes like a stream. It is a candidate until the game ends, so with nothing waiting it stays its five minutes. The player's own game included.
- **A stream goes live.** It joins Rest once, at the `Pending -> Live` edge, and takes at least two minutes like a track. A refresh through grace keeps its stamp, so it does not join again; it is a candidate until the stream ends, so with nothing waiting it stays its five minutes.
- **A match ends.** Its move leaves with the active row and its result joins Rest, showing the final board and the result line.
- **Something disappears** (a track skipped, a link deleted, a stream ended): if it was up, the strip moves on at once; if it was waiting, it leaves its lane.
- **The viewer is reading.** Going up or coming down changes the card's height, so it waits while a message is selected, unless what the strip was showing is gone. Switching what is shown while it stays up does not wait. A Zen tile's size is its own, so nothing waits there.

## 1. What it is

A window onto what the room is doing right now, for discovery, living where the room is: the top of the Home #lounge card (no other room, not the chat center, not the Zen chat tiles), in the topic row's place while it is up. It cannot be turned off there. It is up only while something just happened, so it appearing is the news. On screen it is named `now`: the `── now ──` rule on the card and the `now` segment of the status line (`Now` in its customizer). `live` is the domain's name in the code, and on screen it is the panel's (§1): what is on, beside what just happened.

The right sidebar's Live panel (`panel.rs`, `RightSidebarComponent::Live`, on by default, reordered and toggled like every panel) is the list to the strip's headline: four rows under a `── live ──` rule, one per thing the house can still join or read, for as long as that is true, not only while it is news. Three kinds: a stream that has gone live (a pending one has no stamp and is not listed), a live game on a watchable door, and a News share younger than `NEWS_LIFETIME` (1h), the one piece of news the room has no other way to catch up on, since a share posts nothing to #lounge. Newest on top within a kind: what just started is what the room is talking about, and the hub rail keeps the door-by-door order for browsing. **The floor rule** (`panel::arrange`) shares the four rows: every kind with something gets one row first, then the leftover goes streams, then games, then news, and what does not fit folds into `+N more` in the last row; so two streams never push the games off, and a stream and a game never push the fresh link off. Four of everything reads stream, game, link, `+9 more`; nothing live reads four links. The rows that made it show grouped, streams on top, games, news last, newest first within a kind: the floor rule decides what is on the panel, not where. A row is a seven-column kind, a middle, an edge, and its key at the right (`s1` to `s4`, in the music panel's `v1` key style). The kind is `stream` (amber), the game's name (`dcss`, `nethack`, `brogue`, `SpectateGame::slug`, dim) or `news` (faint): what the row is, told apart at a glance by the word and its color. The middle is the streamer and the stream's title, where the game's player is as the host read it (or `12m in` until it has; the handle is the watch header's, not the row's), or the link's title. The edge: `·N` viewers for a stream (the watch page's count) or a game (the people with the watch open, `OpenWatches::watchers_of`), an amber `●` for a link while it is unread (newer than the reader's News cursor, `news::state::ReadCursor`: the badge's rule, so a visit to the News room clears it; not how long ago, the panel's hour is short enough). None reads `nobody playing`; the panel never changes shape. `s` then the row's digit (`input::open_from_prefix`; `s` is the Live prefix, armed in `app/input.rs` on Home like `z` and in `zen/input.rs` while a Live tile is on the page, swallowing any other suffix) or a click on the row (`input::open_from_panel_click`) opens it exactly as the strip's click would, both over the sources the last draw recorded in `LiveState::panel_hit`, so the key opens what the eye sees and does nothing while the rail is not drawn: the stream's room, the watch, the article. A deadchannel run or fight joins as a `LivePanelRow` variant with its own text and its own `LiveSource`.

Zen's Live tile (`TileKind::Live`, placed like any other) is not the strip: it draws the Live panel's rows (§1 below, `panel::draw_live_inline` over the same rows the frame built for the sidebar), the same four rows with their `s1` to `s4` keys, wider, so less is cut. The strip's sources that the panel does not list (a daily match, a booth track) are still on Zen through the status line's Live segment.

It is also where News shares land: a share posts nothing into the #lounge chat (`../chat/CONTEXT.md` §11 News), so the strip is how the room sees a link, and `r` is how it answers one (`r` on a story in the News room starts the same reply, `../chat/CONTEXT.md` §11 News).

## 2. File map

| File | Role |
|---|---|
| `mod.rs` | Declarations only. |
| `pick.rs` | Pure rules: `LiveSource` (the closed enum of sources), `Lane` and `lane`, `min_for`, `LiveCandidate`, `Featured`, `pick_queued` over `replay` and `take_next`, `aim_overlay`, and the clocks `LIVE_AIM_WINDOW` (8s), `LIVE_MATCH_MIN` (1 min), `LIVE_TRACK_MIN` (2 min), `LIVE_STREAM_MIN` (2 min), `LIVE_DOOR_GAME_MIN` (2 min), `LIVE_NEWS_MIN` (5 min), `LIVE_MAX_UP` (5 min), `LIVE_MAX_WAIT` (10 min), `LIVE_STAMP_HORIZON` (the two added: the oldest stamp the strip can still show). |
| `state.rs` | Per-session `LiveState`: what the queue has up (`queued`), what the strip shows (`shown`: the overlay, else the queue), the render-recorded `hit` rect. `refresh` is the pure rule over candidates and clocks; `tick` and `view` are the glue that reads `DailyState`, `AudioState`, the session's News snapshot (`chat.news.all_articles()`), its copy of the stream registry (`chat.live_streams`) and the watchable doors' live games (`LiveGamesService::live_rows`). `tick` also keeps the featured booth track's thumbnail rendered for this session's terminal (`TrackPicture`), so the frame never renders an image. `LiveStripView` is what gets painted, one variant per kind of body. |
| `ui.rs` | The frame: `fit_live_strip` (the form, from the card's size alone), `draw_live_strip`, `live_strip_lines`, `live_strip_compact_line`, the `── now ──` rule, `PICTURE_ROWS` (8), `PICTURE_COLS` (21), `StripBody` (what a source hands the frame: picture, words, the key `hint`, glow), `key_hint_spans` over `HintPart` (the key hint every body carries: keys in amber, words faint), `status_text` (the one-row form laid out for the status line's one title cap, `statusline::data::TITLE_COLS`, 20, for its Live segment). |
| `input.rs` | `open_from_key` (`o` on the card), `open_from_click`, `open_from_panel_click` (a click on a row of the sidebar's Live panel), `open_from_prefix` (`s` then the row's digit, on Home or on Zen with a Live tile on the page), `reply_from_key` (`r`, the card only): exhaustive matches on `LiveSource` that say what each key does to each source. |
| `panel.rs` | The sidebar's Live panel: `LivePanelRow` (the closed enum of what a row lists: a stream, a door game, a News share), `rows` from the stream registry's copy, the doors' live games and their open-watch counts, and the News snapshot under `NEWS_LIFETIME`, through the floor rule `arrange`; `draw_live_inline` over the pure `panel_lines`, `LIVE_PANEL_HEIGHT` (4). |

## 3. Sources

| `LiveSource` | Candidates | Body |
|---|---|---|
| `DailyMatch(match id)` | `DailyState::live_candidates`: every active match, the viewer's own included, plus the shooter's aim while fresh | `lobby/daily/live_strip.rs` over `live_board.rs`: the board, the players, where the match stands, what just happened |
| `DailyResult(match id)` | `DailyState::live_candidates`: every match that ended inside `LIVE_STAMP_HORIZON` (`note_results`) | the same body with the final board and the result line (`live_result_view`) |
| `BoothTrack(queue item id)` | `AudioState::live_candidates`: every track playing or queued in the YouTube booth | `audio/booth/live.rs`: the thumbnail, the title, channel and length, who queued it and where it stands |
| `NewsArticle(article id)` | `chat/news/live.rs::candidates`: every article in the News snapshot (the newest `NEWS_FEED_LIMIT`) | `chat/news/live.rs`: the article's ASCII art centred in the picture column, the title, the first two summary lines, who shared it, `o read · r reply` |
| `Stream(streamer's user id)` | `stream/live.rs::candidates`: every stream in `ChatState::live_streams` that has gone live (`went_live_at`); a pending one is never offered | `stream/live.rs`: a drawn screen with `⦿ LIVE`, the title, how many are watching, who went live, `o or click for the room` |
| `DoorGame(LiveGameKey)` | `door/spectate/live.rs::candidates`: every game on the watchable doors' rosters (DCSS, NetHack, Brogue; `LiveGamesService::live_rows`), the viewer's own included. The key is the door plus the player's handle held inline (`spectate::state::LiveGameKey`, handles being ASCII and at most 20 bytes), so the source stays `Copy` | `door/spectate/live.rs`: the door's own dungeon drawn in its ASCII (a `#`-walled crawl room, a NetHack room with its little dog and fountain, a Brogue cavern with grass and water), the door, where the player is (`XL3 Lair:2`, or the time in) and who is watching, who is playing, `o or click to watch` |

What each key does (`input.rs`). On Zen there is no `o` and no `r`: the Live tile is the panel, opened by `s` then a row's number from any tile (`open_from_prefix`), `r` flips the split there, and a #lounge draft would sit under whichever chat tile holds the composer.
- `o` / click on a match: `open_board` with `BoardEntry::LoungeStrip`, then `Screen::DailyMatch`. The return screen is `lobby/modal_input.rs::return_screen_for_opening`, not the raw current page: the status line's Live segment reaches this from a board or a house table, and the new board keeps the page that one was opened from. On the match's own board the click opens nothing, so the cursor and a pending move stay. On a result: nothing, `LiveState::opens` never offers it.
- `o` / click on a track: on another source, switch to YouTube (`App::set_paired_playback_source`); already on YouTube, open the booth modal. Nothing if the track left the booth since the last tick (`AudioState::in_booth`).
- `o` / click on an article: `ChatState::open_news_modal_for_article`, the article modal (Enter copies the link, `n` jumps to it in News). Nothing if the article left the snapshot.
- `o` / click on a stream: `select_room_slot` on the stream's room, then `Screen::Dashboard`, the path its rail row takes: joined lazily on the first visit, counted as a named viewer (`opened_stream_room`), and the room header carries the watch link. From Zen it leaves the page for Home. Nothing if the stream ended since the last tick.
- `o` / click on a door game: `spectate::input::open_live_game`, the open watch on `Screen::Games` (the game across the page, its watch chat beside it), starting the stream unless this session already watches that game. The open watch is the last stop on the backtick cycle, so `` ` `` hops straight back to Home. Nothing if the game left the roster since the last tick.
- `r` on an article: `ChatState::begin_reply_to_article` opens the #lounge composer with a reply target of `ReplyTo::Article`; the sent message carries `> @sharer: 📰 Title` on top and no `reply_to_message_id`, since there is no message to point at. `r` on anything else falls through.

Sources are state, not events: a candidate exists while the thing is still going on (the match is active, the track is in the booth, the article is in News, the stream is live), so the strip never advertises something that is over. A result is the exception that proves it: the finished list (`list_finished_unseen`) drops a row once both players have seen it, which in a live pool game is a second after the finish, so `DailyState` keeps each result it saw, stamped with the shared `finished_at`, for `LIVE_STAMP_HORIZON`: long enough to wait its longest and then stay up its longest. A result both players saw before this session's first snapshot is never noted.

### Adding a source
1. Add a variant to `LiveSource`. The build breaks at `pick.rs::lane` and `min_for`, `state.rs::view` and `opens`, `input.rs`, and wherever a body is matched.
2. In the source's own domain, offer candidates (a stamp every replica shares) and a view, and paint a `StripBody` (its key `hint` apart from the words) plus the one-row form.
3. Add a `LiveStripView` variant if the body is a new kind, and extend `LiveState::tick`.
4. Say what `o` and `r` do in `input.rs`.
5. Add its row to §0.

## 4. The pick (`pick.rs`)

`replay` runs the queue from the candidates' stamps up to now. It walks from one moment that matters to the next (a stamp joining its lane, the end of the up source's minimum, the end of its `LIVE_MAX_UP`), dropping entries past `LIVE_MAX_WAIT` and handing over by `take_next` (oldest link, else oldest of the rest). A stamp ahead of the clock has not happened yet. `LiveSource`'s ordering only breaks a tie between two equal stamps.

The replay reads only each candidate's latest stamp, so a match that moves again rewrites the history. `pick_queued` is the replay plus one session rule: what this session has up keeps it for its minimum from when this session put it up, as long as it is still a candidate. A session with nothing up takes the replay's pick as it stands, so one that connects mid-queue sees what the room sees.

`aim_overlay` is separate on purpose: it reads the aims and what the queue has up, and nothing else reads it back.

## 5. What the strip shows (`state.rs::refresh`)

Run each tick after the chat, daily and audio ticks (`app/tick.rs`): `pick_queued`, then `aim_overlay` over its result, then the reading hold (§0). A change of what the strip shows is a `changed` tick; the aim animation rides the half-tick (`LiveState::aiming`).

## 6. The frame (`ui.rs`)

The picture sits in a `PICTURE_COLS` column, centred in `PICTURE_ROWS`, with the words to its right, the key hint on the words' seventh row, then the rule under them. The frame cuts every picture row to its column (`frame_lines`), so nothing a source draws can run into the words. Two fixed forms picked by the card's size and never by what is shown: the 9-row strip on a card at least 56 wide with 10 rows left for messages (so the chat keeps the larger share), else one row (`── now 8ball eggy v weslin`, `── now booth mat · Naima`, `── now news mat · Some Title`), else nothing. The rule's label glows when the source says so (§0).

The Zen Live tile does not draw the strip: it draws the Live panel (§1).

`draw_live_strip` records `LiveState::hit` when what it drew opens something, and `panel::draw_live_inline` records `LiveState::panel_hit`; `App::render` clears both before every draw.

## 7. Wiring outside this directory

- `app/state.rs`: `App::live`.
- `app/tick.rs`: `self.live.tick(&self.daily, &self.audio, self.chat.news.all_articles(), &self.chat.live_streams, &self.live_games.live_rows(), reading, picture_settings)` on every tick whatever the page, after the chat tick has drained the News snapshot and `tick_stream` has copied the stream registry, the settings being the session's `inline_image_render_settings`; `App::lounge_card_shown` gates the reading hold; `App::live_strip_shown` (the #lounge card; Zen's Live tile is the panel, which does not animate) gates the aim's half-tick repaint and the wake hint's half tier.
- `app/render.rs`: builds the view when `home_selected`, never for the chat center. It also builds it on any page while the status line carries the Live segment, for `status_text`. It builds the Live panel's rows (`panel::rows`, from `chat.live_streams`, the hub's live rows, `chat.news.all_articles()` and `LiveGamesService::open_watches`, at one `Utc::now()` the draw ages the rows by) on every frame and hands them to the sidebar with the panel's hit slot (`common/sidebar.rs`, `SidebarProps::live`), clearing the slot with the strip's before the draw.
- `app/common/sidebar.rs`: `RightSidebarComponent::Live` draws `panel::draw_live_inline` under its rule, at `LIVE_PANEL_HEIGHT`.
- `app/input.rs`: the click on a panel row, next to the strip's, under the same modal gate.
- `app/statusline`: the Live segment (`StatusComponent::Live`, `now` on the bar) reads `status_text` and its click is `open_from_key`, so the strip's reading and its key reach every page (`../statusline/CONTEXT.md`).
- `app/zen`: `TileKind::Live` draws the Live panel's rows (`ZenView::live_panel`, `panel::draw_live_inline`, the rows the frame built for the sidebar); `zen/input.rs::handle_common` arms the `s` prefix while a Live tile is drawn and spends it on the next key (`open_from_prefix`), ahead of the focused tile's own keys, as `app/input.rs::handle_byte_event` does on Home ahead of the slash composer and the page keys.
- `app/chat/ui.rs`: `DashboardChatView.live_strip` + `live_strip_hit`; `draw_dashboard_chat_card` carves the strip off the top of the messages, above the poll strip. While it is up the room header drops its topic row and closing rule (stream and voice rows stay).
- `app/input.rs`: `o` and `r` in `handle_global_key`, gated on `App::lounge_card_shown` and no composer; `r` also on no message being selected, so `r` on a selected message still replies to the message. The click, on either surface, gated on `chat_scroll_clicks_blocked` and taken before a click focuses a Zen tile.

## 8. Across replicas

The pick reads only stamps that live in the database, so it is as shared as the snapshot that carries them.
- Daily matches and results: every write fires `daily_match_changed` and every replica re-reads (`../lobby/daily/CONTEXT.md`). The aim stays on the writer's replica by choice.
- News articles: every `articles` write fires `articles_changed` and every replica re-reads its snapshot (`../chat/CONTEXT.md` §11 News), so an article reaches every strip.
- Door games: the rosters come from the door hosts, which every replica follows (`LiveGamesService`), and the stamp is the host's start time, so a game reaches every strip.
- Streams: the registry is in-process by design (`../stream/CONTEXT.md`), so a stream and its `went_live_at` exist only on the replica that hosts it.
- Booth tracks: the queue snapshot is published by the replica that handled the write and by its own reconcile; `../audio/CONTEXT.md` documents audio as single-replica. A track queued on another replica reaches this one's strip only when this one next publishes.

## 9. Tests

- `pick_test.rs`: the lanes handing over at each minimum with a link going first and the strip coming down at the max; a stream queuing behind a track and keeping its two minutes; a burst of links with one dropped past `LIVE_MAX_WAIT`; a session keeping its minimum when a match moves again, and not when its source is gone; the aim drawn over a move and an empty strip but not a link, and not past its window.
- `state_test.rs`: a track holding its two minutes, a move, the match's result joining in its place, a link going up at the next handover and the strip coming down, asserted as the whole of what the strip shows and opens at each step; the height held under a selection.
- `ui_test.rs`: the form picked by the card's size, and by a Zen tile's; the hint and the rule drawn on the card and not in a tile; a picture wider than its column cut clear of the words; every key in a hint lit, one faint run once it no longer fits.
- Bodies are tested with their source: `../lobby/daily/live_strip_test.rs`, `../audio/booth/live_test.rs`, `../chat/news/live_test.rs`, `../stream/live_test.rs` (only a stream that went live offered, its words and hint), `../door/spectate/live_test.rs` (every game offered at its start, its words, hint and one-row form, the time in until the host read a status); results in `../lobby/daily/state_test.rs` (offered on a replica that wrote nothing, outliving their row once both players saw them).
- The Zen tile: its empty 30/70 split in `../zen/ui_test.rs`, Enter on it and `o` from a chat tile opening a shared article in `../input_flow_test.rs`.
- End to end in `../dashboard_flow_test.rs`: `o` opening a match, closing back to the card, `o` on a booth track tuning in then opening the booth, `o` on a track that left the booth changing nothing, `o` on a shared article opening the article modal, `r` on one sending a reply that quotes its title, and `o` on a live door game opening the watch, which `` ` `` then toggles with Home (the roster published through `LiveGamesService::publish_roster_for_tests`).
