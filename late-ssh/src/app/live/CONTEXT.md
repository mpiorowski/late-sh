# late-ssh Live Strip Context

## Metadata
- Domain: the live strip, one thing happening in the house painted at the top of the Home #lounge card, with keys to join it, and the Live panel, the list of everything joinable, on the right sidebar and in a Zen Live tile
- Primary audience: LLM agents working in `late-ssh/src/app/live` and the sources that feed it
- Status: Active
- Parent context: `../../../../CONTEXT.md`

---

## 0. The rules at a glance

One queue and one overlay. Every clock is a database stamp or the wall clock, so every session on every replica sees the same thing in the same order (the aim is the one exception, §8).

**Nothing is filtered by who is looking.** A source's candidates never depend on the viewer: your own match, your own stream, your own door game are offered exactly as anyone else's. The same holds for the Games hub rail's `live` rows (`LiveGamesService::live_rows`: every roster entry whose name is a handle, which is what a late.sh player's always is). A candidate drops out only because the thing itself is not live (a pending stream, a match result past its horizon).

**The queue** (`pick_queued`, `replay`):
1. One queue, first come first served: match moves, match results, booth tracks, streams going live, door games starting, board events, realm moments. When the strip hands over, it takes the oldest entry waiting. A News share is never on the strip: it is its card in #lounge (`../chat/news/CONTEXT.md`).
2. Whatever is up stays **at least its minimum**, counted from when it went up.
3. After its minimum it hands over as soon as anything is waiting. With **nothing waiting it stays up to `LIVE_MAX_UP` (5 min)** from when it went up, then the strip comes down.
4. Anything that waited in the queue longer than **`LIVE_MAX_WAIT` (10 min)** is dropped, so a busy evening never builds a backlog.

**The overlay** (`aim_overlay`): while a pool shooter moves the cue (an aim inside `LIVE_AIM_WINDOW`, 8s), that table is drawn over whatever the queue has up, or over an empty strip. It does not touch the queue: the clock of what it covers keeps running, and the queue carries on when the cue is still.

| Source | Joins the queue | Minimum | Glows | `o` / click |
|---|---|---|---|---|
| `DailyMatch` | the claim, and again on every move (`updated`) | `LIVE_MATCH_MIN` (1 min) | while aimed | the board |
| `DailyResult` | the finish (`finished_at`) | `LIVE_MATCH_MIN` (1 min) | yes | nothing (the board left the lobby) |
| `BoothTrack` | when it was queued, not when it starts playing | `LIVE_TRACK_MIN` (2 min) | while it is the one playing | tune in to YouTube, or the booth if already there |
| `Stream` | when its media first flowed (`went_live_at`), never at `/golive` | `LIVE_STREAM_MIN` (2 min) | yes | the streamer's room on Home |
| `DoorGame` | when the game started on its door host (`started_unix` on the roster) | `LIVE_DOOR_GAME_MIN` (2 min) | yes | the open watch on the Games screen |
| `BoardEvent` | an hour before the event starts (`starts_at - STRIP_LEAD`), and again at the start (`starts_at`) | `LIVE_BOARD_EVENT_MIN` (2 min) | once it started | the board on the event's day with its details up |
| `Realm` | each moment the row's `news` records (a world rising, an arrival, a take, a fall, the end), at the instant the write stamped it | `LIVE_MATCH_MIN` (1 min) | while the world is being fought over | the realm board, to play or to watch |

The cases, in words:
- **A match moves again.** Its entry is its newest move, so it joins the back of the queue again; with nothing waiting, it goes straight back up and its five minutes restart. A session showing it keeps it for its minimum even though the replay's history moved (`pick_queued`).
- **A YouTube track.** Every track playing or waiting in the booth is an entry, stamped when it was queued. Queue three and each takes at least two minutes in order, reading "#3 in line", "up next" or "playing now" as it stands. A track that starts playing long after it was queued does not join again.
- **A door game starts.** Somebody launches DCSS, NetHack or Brogue: it joins the queue once, stamped with the host's start time, and takes at least two minutes like a stream. It is a candidate until the game ends, so with nothing waiting it stays its five minutes. The player's own game included.
- **A stream goes live.** It joins the queue once, at the `Pending -> Live` edge, and takes at least two minutes like a track. A refresh through grace keeps its stamp, so it does not join again; it is a candidate until the stream ends, so with nothing waiting it stays its five minutes.
- **An event is an hour off.** A board post joins the queue an hour before its start, stamped then, reading `in 58m · 6 in`; it joins again at its start, stamped then, lit and reading `on now`. It is a candidate until it ends. Both stamps are the row's own instant, so every replica agrees.
- **A match ends.** Its move leaves with the active row and its result joins the queue, showing the final board and the result line.
- **Something disappears** (a track skipped, a stream ended): if it was up, the strip moves on at once; if it was waiting, it leaves the queue.
- **The viewer is reading.** Going up or coming down changes the card's height, so it waits while a message is selected, unless what the strip was showing is gone. Switching what is shown while it stays up does not wait. A Zen tile's size is its own, so nothing waits there.

## 1. What it is

A window onto what the room is doing right now, for discovery, living where the room is: the top of the Home #lounge card (no other room, not the chat center, not the Zen chat tiles), in the topic row's place while it is up. It cannot be turned off there. It is up only while something just happened, so it appearing is the news. On screen it is named `now`: the `── now ──` rule on the card and the `now` segment of the status line (`Now` in its customizer). `live` is the domain's name in the code, and on screen it is the panel's (§1): what is on, beside what just happened.

The right sidebar's Live panel (`panel.rs`, `RightSidebarComponent::Live`) is the list to the strip's headline: four rows, one per thing the house can still join or read for as long as that is true (a live stream, a live door game, a timed board event from an hour before its start, a News share under an hour old), shared by **the floor rule** (`panel::arrange`: every kind with something gets a row first) and opened with `s` then the row's digit or a click. Zen's Live tile draws the same rows. The rows, the floor rule and the keys are in [`PANEL.md`](PANEL.md).

A News share is not on the strip: it posts its card into #lounge, which is how the room sees a link (`../chat/news/CONTEXT.md`). The panel lists it for an hour.

## 2. File map

| File | Role |
|---|---|
| `mod.rs` | Declarations only. |
| `pick.rs` | Pure rules: `LiveSource` (the closed enum of what the strip can show), `min_for`, `LiveCandidate`, `Featured`, `pick_queued` over `replay`, `aim_overlay`, and the clocks `LIVE_AIM_WINDOW` (8s), `LIVE_MATCH_MIN` (1 min), `LIVE_TRACK_MIN` (2 min), `LIVE_STREAM_MIN` (2 min), `LIVE_DOOR_GAME_MIN` (2 min), `LIVE_MAX_UP` (5 min), `LIVE_MAX_WAIT` (10 min), `LIVE_STAMP_HORIZON` (the two added: the oldest stamp the strip can still show). |
| `state.rs` | Per-session `LiveState`: what the queue has up (`queued`), what the strip shows (`shown`: the overlay, else the queue), the render-recorded `hit` rect. `refresh` is the pure rule over candidates and clocks; `tick` and `view` are the glue that reads `DailyState`, `AudioState`, the session's copy of the stream registry (`chat.live_streams`), the watchable doors' live games (`LiveGamesService::live_rows`), the board (`CalendarState`) and the realm snapshot (`RealmState`). `tick` also keeps the featured booth track's thumbnail rendered for this session's terminal (`TrackPicture`), so the frame never renders an image. `LiveStripView` is what gets painted, one variant per kind of body. |
| `ui.rs` | The frame: `fit_live_strip` (the form, from the card's size alone), `draw_live_strip`, `live_strip_lines`, `live_strip_compact_line`, the `── now ──` rule, `PICTURE_ROWS` (8), `PICTURE_COLS` (21), `StripBody` (what a source hands the frame: picture, words, the key `hint`, glow), `key_hint_spans` over `HintPart` (the key hint every body carries: keys in amber, words faint), `status_text` (the one-row form laid out for the status line's one title cap, `statusline::data::TITLE_COLS`, 20, for its Live segment). |
| `input.rs` | `open_from_key` (`o` on the card), `open_from_click`, `open_from_panel_click` (a click on a row of the sidebar's Live panel), `open_from_prefix` (`s` then the row's digit, on Home or on Zen with a Live tile on the page): exhaustive matches on `LiveSource` that say what each key does to each source, and on `panel::PanelTarget` for a panel row (a stream, a game or an event opens as the strip opens it, a News share opens the reader). |
| `panel.rs` | The sidebar's Live panel: `LivePanelRow` (the closed enum of what a row lists: a stream, a door game, a board event, a News share) and `PanelTarget` (what each row opens, one per kind, recorded in `LiveState::panel_hit`), `rows` from the stream registry's copy, the doors' live games and their open-watch counts, the board's live timed events, and the News snapshot under `NEWS_LIFETIME`, through the floor rule `arrange`; `draw_live_inline` over the pure `panel_lines`, `LIVE_PANEL_HEIGHT` (4). |

## 3. Sources

| `LiveSource` | Candidates | Body |
|---|---|---|
| `DailyMatch(match id)` | `DailyState::live_candidates`: every active match, the viewer's own included, plus the shooter's aim while fresh | `lobby/daily/live_strip.rs` over `live_board.rs`: the board, the players, where the match stands, what just happened |
| `DailyResult(match id)` | `DailyState::live_candidates`: every match that ended inside `LIVE_STAMP_HORIZON` (`note_results`) | the same body with the final board and the result line (`live_result_view`) |
| `BoothTrack(queue item id)` | `AudioState::live_candidates`: every track playing or queued in the YouTube booth | `audio/booth/live.rs`: the thumbnail, the title, channel and length, who queued it and where it stands |
| `Stream(streamer's user id)` | `stream/live.rs::candidates`: every stream in `ChatState::live_streams` that has gone live (`went_live_at`); a pending one is never offered | `stream/live.rs`: a drawn screen with `⦿ LIVE`, the title, how many are watching, who went live, `o or click for the room` |
| `DoorGame(LiveGameKey)` | `door/spectate/live.rs::candidates`: every game on the watchable doors' rosters (DCSS, NetHack, Brogue, Zork; `LiveGamesService::live_rows`), the viewer's own included. The key is the door plus the player's handle held inline (`spectate::state::LiveGameKey`, handles being ASCII and at most 20 bytes), so the source stays `Copy` | `door/spectate/live.rs`: the door's own dungeon drawn in its ASCII (a `#`-walled crawl room, a NetHack room with its little dog and fountain, a Brogue cavern with grass and water, the white house with its mailbox), the door, where the player is (`XL3 Lair:2`, or the time in) and who is watching, who is playing, `o or click to watch` |
| `BoardEvent(event id)` | `calendar/live.rs::candidates`: every timed event in the board's live set (`CalendarState::live_at`: the replica's shared snapshot minus what ended) inside `STRIP_LEAD` of its start or on; never an all-day event, never the viewer's own | `calendar/live.rs`: a drawn calendar page (month, day, weekday, start time in the viewer's zone), the title, `in 58m · 6 in` or `on now`, who posted it, `o or click for the board` |
| `Realm(game id)` | `door/realm/live.rs::candidates`: the standing world and the last finished one while it is listed (`RealmSnapshot`), each stamped with its `news.at`, the instant of its last moment as the write that made it recorded it in the row; a claim on empty land is not a moment, so it never knocks one off | `door/realm/live.rs`: a drawn scrap of coastline, `Realm · Season N`, the moment (`mat took France from odolha`, `mat marched in as Crimson`, `a new world rises, the gates open in 23h`, `the world fell to mat`), who is alive and how much is claimed, `o or click to enter the realm` |

What each key does (`input.rs`). On Zen there is no `o`: the Live tile is the panel, opened by `s` then a row's number from any tile (`open_from_prefix`), and the layout keys are uppercase there (`F` flips the split).
- `o` / click on a match: `open_board` with `BoardEntry::LoungeStrip`, then `Screen::DailyMatch`. The return screen is `lobby/modal_input.rs::return_screen_for_opening`, not the raw current page: the status line's Live segment reaches this from a board or a house table, and the new board keeps the page that one was opened from. On the match's own board the click opens nothing, so the cursor and a pending move stay. On a result: nothing, `LiveState::opens` never offers it.
- `o` / click on a track: on another source, switch to YouTube (`App::set_paired_playback_source`); already on YouTube, open the booth modal. Nothing if the track left the booth since the last tick (`AudioState::in_booth`).
- `o` / click on a stream: `select_room_slot` on the stream's room, then `Screen::Dashboard`, the path its rail row takes: joined lazily on the first visit, counted as a named viewer (`opened_stream_room`), and the room header carries the watch link. From Zen it leaves the page for Home. Nothing if the stream ended since the last tick.
- `o` / click on a door game: `spectate::input::open_live_game`, the open watch on `Screen::Games` (the game across the page, its watch chat beside it), starting the stream unless this session already watches that game. The open watch is the last stop on the backtick cycle, so `` ` `` hops straight back to Home. Nothing if the game left the roster since the last tick.
- `o` / click on a board event: `CalendarState::show_event` with the upcoming set's copy, then `Screen::Calendars`: the event's day with its details up. Counted in `late_ssh_calendar_opens_total` by `Origin` (the strip, or the panel below). Nothing if the event ended or was taken down since the last tick.
- `o` / click on the realm: the board of that world, as the Games hub card opens it (Realm's own `door/realm/input.rs::open_from_live`, handed the return screen from `return_screen_for_opening`; it owns the "already on that board" and "left the listing" checks), read-only unless the viewer plays in it; a board already open on it is resumed. On that board already there is nothing to open. Nothing if the world left the listing since the last tick.

Sources are state, not events: a candidate exists while the thing is still going on (the match is active, the track is in the booth, the stream is live), so the strip never advertises something that is over. A result is the exception that proves it: the finished list (the unseen-result part of `DailyMatch::list_lobby`) drops a row once both players have seen it, which in a live pool game is a second after the finish, so `DailyState` keeps each result it saw, stamped with the shared `finished_at`, for `LIVE_STAMP_HORIZON`: long enough to wait its longest and then stay up its longest. A result both players saw before this session's first snapshot is never noted.

### Adding a source
1. Add a variant to `LiveSource`. The build breaks at `pick.rs::min_for`, `state.rs::view` and `opens`, `input.rs`, and wherever a body is matched.
2. In the source's own domain, offer candidates (a stamp every replica shares) and a view, and paint a `StripBody` (its key `hint` apart from the words) plus the one-row form.
3. Add a `LiveStripView` variant if the body is a new kind, and extend `LiveState::tick`.
4. Say what `o` does in `input.rs`.
5. Add its row to §0.

## 4. The pick (`pick.rs`)

`replay` runs the queue from the candidates' stamps up to now. It walks from one moment that matters to the next (a stamp joining the queue, the end of the up source's minimum, the end of its `LIVE_MAX_UP`), dropping entries past `LIVE_MAX_WAIT` and handing over to the oldest entry waiting. A stamp ahead of the clock has not happened yet. `LiveSource`'s ordering only breaks a tie between two equal stamps.

The replay reads only each candidate's latest stamp, so a match that moves again rewrites the history. `pick_queued` is the replay plus one session rule: what this session has up keeps it for its minimum from when this session put it up, as long as it is still a candidate. A session with nothing up takes the replay's pick as it stands, so one that connects mid-queue sees what the room sees.

`aim_overlay` is separate on purpose: it reads the aims and what the queue has up, and nothing else reads it back.

## 5. What the strip shows (`state.rs::refresh`)

Run each tick after the chat, daily and audio ticks (`app/tick.rs`): `pick_queued`, then `aim_overlay` over its result, then the reading hold (§0). A change of what the strip shows is a `changed` tick; the aim animation rides the half-tick (`LiveState::aiming`).

## 6. The frame (`ui.rs`)

The picture sits in a `PICTURE_COLS` column, centred in `PICTURE_ROWS`, with the words to its right, the key hint on the words' seventh row, then the rule under them. The frame cuts every picture row to its column (`frame_lines`), so nothing a source draws can run into the words. Two fixed forms picked by the card's size and never by what is shown: the 9-row strip on a card at least 56 wide with 10 rows left for messages (so the chat keeps the larger share), else one row (`── now 8ball eggy v weslin`, `── now booth mat · Naima`), else nothing. The rule's label glows when the source says so (§0).

The Zen Live tile does not draw the strip: it draws the Live panel (`PANEL.md`).

`draw_live_strip` records `LiveState::hit` when what it drew opens something, and `panel::draw_live_inline` records `LiveState::panel_hit`; `App::render` clears both before every draw.

## 7. Wiring outside this directory

- `app/state.rs`: `App::live`.
- `app/tick.rs`: `self.live.tick(&self.daily, &self.audio, &self.chat.live_streams, &self.live_games.live_rows(), &self.calendar, &self.realm, reading, picture_settings)` on every tick whatever the page, after `tick_stream` has copied the stream registry, the settings being the session's `inline_image_render_settings`; `App::lounge_card_shown` gates the reading hold; `App::live_strip_shown` (the #lounge card; Zen's Live tile is the panel, which does not animate) gates the aim's half-tick repaint and the wake hint's half tier.
- `app/render.rs`: builds the view when `home_selected`, never for the chat center. It also builds it on any page while the status line carries the Live segment, for `status_text`. It builds the Live panel's rows (`panel::rows`, from `chat.live_streams`, the hub's live rows, the board's live events (`calendar/live.rs::panel_events`), `chat.news.all_articles()` and `LiveGamesService::open_watches`, at one `Utc::now()` the draw ages the rows by) on every frame and hands them to the sidebar with the panel's hit slot (`common/sidebar.rs`, `SidebarProps::live`), clearing the slot with the strip's before the draw.
- `app/common/sidebar.rs`: `RightSidebarComponent::Live` draws `panel::draw_live_inline` under its rule, at `LIVE_PANEL_HEIGHT`.
- `app/input.rs`: the click on a panel row, next to the strip's, under the same modal gate.
- `app/statusline`: the Live segment (`StatusComponent::Live`, `now` on the bar) reads `status_text` and its click is `open_from_key`, so the strip's reading and its key reach every page (`../statusline/CONTEXT.md`).
- `app/zen`: `TileKind::Live` draws the Live panel's rows (`ZenView::live_panel`, `panel::draw_live_inline`, the rows the frame built for the sidebar); `zen/input.rs::handle_common` arms the `s` prefix while a Live tile is drawn and spends it on the next key (`open_from_prefix`), ahead of the focused tile's own keys, as `app/input.rs::handle_byte_event` does on Home ahead of the slash composer and the page keys.
- `app/chat/ui.rs`: `DashboardChatView.live_strip` + `live_strip_hit`; `draw_dashboard_chat_card` carves the strip off the top of the messages, above the poll strip. While it is up the room header drops its topic row and closing rule (stream and voice rows stay); on a day the board has an all-day event that row otherwise reads the board's day (`chat/ui.rs::TopicRow::BoardDay`, `calendar/live.rs::day_text`).
- `app/input.rs`: `o` in `handle_global_key`, gated on `App::lounge_card_shown` and no composer. The click, on either surface, gated on `chat_scroll_clicks_blocked` and taken before a click focuses a Zen tile.

## 8. Across replicas

The pick reads only stamps that live in the database, so it is as shared as the snapshot that carries them.
- Daily matches and results: every write fires `daily_match_changed` and every replica re-reads (`../lobby/daily/CONTEXT.md`). The aim stays on the writer's replica by choice.
- News articles (the panel only): every `articles` write fires `articles_changed` and every replica re-reads its snapshot (`../chat/news/CONTEXT.md`), so a share reaches every panel.
- Door games: the rosters come from the door hosts, which every replica follows (`LiveGamesService`), and the stamp is the host's start time, so a game reaches every strip.
- Streams: the registry is in-process by design (`../stream/CONTEXT.md`), so a stream and its `went_live_at` exist only on the replica that hosts it.
- Booth tracks: the queue snapshot is published by the replica that handled the write and by its own reconcile; `../audio/CONTEXT.md` documents audio as single-replica. A track queued on another replica reaches this one's strip only when this one next publishes.
- Board events: every `calendar_events` or `calendar_rsvps` write fires `calendar_changed` and every replica re-reads the board's live events into its shared watch (`../calendar/CONTEXT.md`); the stamps are the row's `starts_at`, so an event reaches every strip at the same instant. A viewer's own events never reach the strip.
- The realm: every `realm_games` write fires `realm_game_changed` and every replica re-reads its snapshot (`../door/realm/CONTEXT.md`); the stamp is the `news.at` the writing replica put in the row, so a moment reaches every strip at the same instant.

## 9. Tests

- `pick_test.rs`: the queue handing over at each minimum, oldest first, and the strip coming down at the max; a stream queuing behind a track and keeping its two minutes; a burst of booth tracks with one dropped past `LIVE_MAX_WAIT`; a session keeping its minimum when a match moves again, and not when its source is gone; the aim drawn over whatever is up and an empty strip, and not past its window.
- `state_test.rs`: a track holding its two minutes, a move, the match's result joining in its place and opening nothing, and the strip coming down, asserted as the whole of what the strip shows and opens at each step; the height held under a selection.
- `ui_test.rs`: the form picked by the card's size, and by a Zen tile's; the hint and the rule drawn on the card and not in a tile; a picture wider than its column cut clear of the words; every key in a hint lit, one faint run once it no longer fits.
- Bodies are tested with their source: `../lobby/daily/live_strip_test.rs`, `../audio/booth/live_test.rs`, `../stream/live_test.rs` (only a stream that went live offered, its words and hint), `../door/spectate/live_test.rs` (every game offered at its start, its words, hint and one-row form, the time in until the host read a status); results in `../lobby/daily/state_test.rs` (offered on a replica that wrote nothing, outliving their row once both players saw them).
- The Zen tile: its empty 30/70 split in `../zen/ui_test.rs`, `s1` opening a shared article from the tile and from a chat tile in `../input_flow_test.rs`.
- End to end in `../dashboard_flow_test.rs`: `o` opening a match, closing back to the card, `o` on a booth track tuning in then opening the booth, `o` on a track that left the booth changing nothing, `Enter` on a News card opening it and `c` copying the link, `r` on one sending a reply that quotes its title, `Enter` on a share's #lounge card opening the reader, `s1` on the newer of two unread shares clearing its panel dot alone (and still read in a later session), and `o` on a live door game opening the watch, which `` ` `` then toggles with Home (the roster published through `LiveGamesService::publish_roster_for_tests`).
