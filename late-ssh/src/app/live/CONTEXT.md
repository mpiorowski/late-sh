# late-ssh Live Strip Context

## Metadata
- Domain: the live strip, one thing happening in the house painted at the top of the Home #lounge card and in a Zen Live tile, with keys to join it
- Primary audience: LLM agents working in `late-ssh/src/app/live` and the sources that feed it
- Status: Active
- Parent context: `../../../../CONTEXT.md`

---

## 0. The rules at a glance

A queue with two lanes, and one overlay. Every clock is a database stamp or the wall clock, so every session on every replica sees the same thing in the same order (the aim is the one exception, §8).

**The queue** (`pick_queued`, `replay`):
1. Two lanes, first come first served: **News** (shared links) and **Rest** (match moves, match results, booth tracks). When the strip hands over, it takes the oldest link if one is waiting, else the oldest of the rest.
2. Whatever is up stays **at least its minimum**, counted from when it went up.
3. After its minimum it hands over as soon as anything is waiting. With **nothing waiting it stays up to `LIVE_MAX_UP` (5 min)** from when it went up, then the strip comes down.
4. Anything that waited in its lane longer than **`LIVE_MAX_WAIT` (10 min)** is dropped, so a busy evening never builds a backlog.

**The overlay** (`aim_overlay`): while a pool shooter moves the cue (an aim inside `LIVE_AIM_WINDOW`, 8s), that table is drawn over whatever the queue has up, or over an empty strip, **never over a link**. It does not touch the queue: the clock of what it covers keeps running, and the queue carries on when the cue is still.

| Source | Lane | Joins its lane | Minimum | Glows | `o` / Enter on Zen / click | `r` (card only) |
|---|---|---|---|---|---|---|
| `NewsArticle` | News | the share (`created`) | `LIVE_NEWS_MIN` (5 min, equal to the max, so exactly 5) | never | the article modal | reply in #lounge quoting the title |
| `DailyMatch` | Rest | the claim, and again on every move (`updated`) | `LIVE_MATCH_MIN` (1 min) | while aimed | the board | nothing |
| `DailyResult` | Rest | the finish (`finished_at`) | `LIVE_MATCH_MIN` (1 min) | yes | nothing (the board left the lobby) | nothing |
| `BoothTrack` | Rest | when it was queued, not when it starts playing | `LIVE_TRACK_MIN` (2 min) | while it is the one playing | tune in to YouTube, or the booth if already there | nothing |

The cases, in words:
- **A link is shared** while a track is up: the track keeps its two minutes, then the link goes ahead of everything waiting in Rest and stays five minutes.
- **A match moves again.** Its entry is its newest move, so it joins the back of Rest again; with nothing waiting, it goes straight back up and its five minutes restart. A session showing it keeps it for its minimum even though the replay's history moved (`pick_queued`).
- **A YouTube track.** Every track playing or waiting in the booth is an entry, stamped when it was queued. Queue three and each takes at least two minutes in order, reading "#3 in line", "up next" or "playing now" as it stands. A track that starts playing long after it was queued does not join again.
- **A match ends.** Its move leaves with the active row and its result joins Rest, showing the final board and the result line.
- **Something disappears** (a track skipped, a link deleted): if it was up, the strip moves on at once; if it was waiting, it leaves its lane.
- **The viewer is reading.** Going up or coming down changes the card's height, so it waits while a message is selected, unless what the strip was showing is gone. Switching what is shown while it stays up does not wait. A Zen tile's size is its own, so nothing waits there.

## 1. What it is

A window onto what the room is doing right now, for discovery, living where the room is: the top of the Home #lounge card (no other room, not the chat center, not the Zen chat tiles, not the sidebar), in the topic row's place while it is up. It cannot be turned off there. It is up only while something just happened, so it appearing is the news.

Zen also has a Live tile (`TileKind::Live`) the user places like any other. It shows the same strip, from the same per-session `LiveState`; while nothing is up it splits 30/70, a faint `nothing live` beside the #lounge activity feed, which takes the larger share, since a tile is always on the page.

It is also where News shares land: a share posts nothing into the #lounge chat (`../chat/CONTEXT.md` §11 News), so the strip is how the room sees a link, and `r` is how it answers one.

## 2. File map

| File | Role |
|---|---|
| `mod.rs` | Declarations only. |
| `pick.rs` | Pure rules: `LiveSource` (the closed enum of sources), `Lane` and `lane`, `min_for`, `LiveCandidate`, `Featured`, `pick_queued` over `replay` and `take_next`, `aim_overlay`, and the clocks `LIVE_AIM_WINDOW` (8s), `LIVE_MATCH_MIN` (1 min), `LIVE_TRACK_MIN` (2 min), `LIVE_NEWS_MIN` (5 min), `LIVE_MAX_UP` (5 min), `LIVE_MAX_WAIT` (10 min), `LIVE_STAMP_HORIZON` (the two added: the oldest stamp the strip can still show). |
| `state.rs` | Per-session `LiveState`: what the queue has up (`queued`), what the strip shows (`shown`: the overlay, else the queue), the render-recorded `hit` rect. `refresh` is the pure rule over candidates and clocks; `tick` and `view` are the glue that reads `DailyState`, `AudioState` and the session's News snapshot (`chat.news.all_articles()`). `tick` also keeps the featured booth track's thumbnail rendered for this session's terminal (`TrackPicture`), so the frame never renders an image. `LiveStripView` is what gets painted, one variant per kind of body. |
| `ui.rs` | The frame: `fit_live_strip` (the form, from the card's size alone), `draw_live_strip`, `fit_live_tile` and `draw_live_tile` (the Zen tile's), `live_strip_lines` for a `StripHost` (the card, or a Zen tile), `live_strip_compact_line`, the `── live ──` rule, `PICTURE_ROWS` (8), `PICTURE_COLS` (21), `StripBody` (what a source hands the frame: picture, words, the key `hint`, glow), `key_hint_spans` over `HintPart` (the key hint every body carries: keys in amber, words faint). |
| `input.rs` | `open_from_key` (`o` on the card, Enter on a focused Zen Live tile), `open_from_click`, `reply_from_key` (`r`, the card only): exhaustive matches on `LiveSource` that say what each key does to each source. |

## 3. Sources

| `LiveSource` | Candidates | Body |
|---|---|---|
| `DailyMatch(match id)` | `DailyState::live_candidates`: every active match, the viewer's own included, plus the shooter's aim while fresh | `lobby/daily/live_strip.rs` over `live_board.rs`: the board, the players, where the match stands, what just happened |
| `DailyResult(match id)` | `DailyState::live_candidates`: every match that ended inside `LIVE_STAMP_HORIZON` (`note_results`) | the same body with the final board and the result line (`live_result_view`) |
| `BoothTrack(queue item id)` | `AudioState::live_candidates`: every track playing or queued in the YouTube booth | `audio/booth/live.rs`: the thumbnail, the title, channel and length, who queued it and where it stands |
| `NewsArticle(article id)` | `chat/news/live.rs::candidates`: every article in the News snapshot (the newest `NEWS_FEED_LIMIT`) | `chat/news/live.rs`: the article's ASCII art centred in the picture column, the title, the first two summary lines, who shared it, `o read · r reply` |

What each key does (`input.rs`). On a Zen Live tile, Enter is `o` and there is no `r`: `r` flips the split there, and a #lounge draft would sit under whichever chat tile holds the composer.
- `o` / click on a match: `open_board` with `BoardEntry::LoungeStrip`, then `Screen::DailyMatch`. On a result: nothing, `LiveState::opens` never offers it.
- `o` / click on a track: on another source, switch to YouTube (`App::set_paired_playback_source`); already on YouTube, open the booth modal. Nothing if the track left the booth since the last tick (`AudioState::in_booth`).
- `o` / click on an article: `ChatState::open_news_modal_for_article`, the article modal (Enter copies the link, `n` jumps to it in News). Nothing if the article left the snapshot.
- `r` on an article: `ChatState::begin_reply_to_article` opens the #lounge composer with a reply target of `ReplyTo::Article`; the sent message carries `> @sharer: 📰 Title` on top and no `reply_to_message_id`, since there is no message to point at. `r` on anything else falls through.

Sources are state, not events: a candidate exists while the thing is still going on (the match is active, the track is in the booth, the article is in News), so the strip never advertises something that is over. A result is the exception that proves it: the finished list (`list_finished_unseen`) drops a row once both players have seen it, which in a live pool game is a second after the finish, so `DailyState` keeps each result it saw, stamped with the shared `finished_at`, for `LIVE_STAMP_HORIZON`: long enough to wait its longest and then stay up its longest. A result both players saw before this session's first snapshot is never noted.

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

The picture sits in a `PICTURE_COLS` column, centred in `PICTURE_ROWS`, with the words to its right, the key hint on the words' seventh row, then the rule under them. The frame cuts every picture row to its column (`frame_lines`), so nothing a source draws can run into the words. Two fixed forms picked by the card's size and never by what is shown: the 9-row strip on a card at least 56 wide with 10 rows left for messages (so the chat keeps the larger share), else one row (`── live 8ball eggy v weslin`, `── live booth mat · Naima`, `── live news mat · Some Title`), else nothing. The rule's label glows when the source says so (§0).

A Zen tile (`StripHost::ZenTile`) draws the same body without the hint and without the rule: every Zen tile names its keys in its title (`enter open` while the strip opens something) and its border parts it from its neighbours. Its forms go by the tile's inner area: the `PICTURE_ROWS` at least 8 rows and 56 wide, else the one row without its `── live` label, each centred top to bottom. No glow on Zen for now.

`draw_live_strip` and `draw_live_tile` record `LiveState::hit` when what they drew opens something; `App::render` clears it before every draw.

## 7. Wiring outside this directory

- `app/state.rs`: `App::live`.
- `app/tick.rs`: `self.live.tick(&self.daily, &self.audio, self.chat.news.all_articles(), reading, picture_settings)` on every tick whatever the page, after the chat tick has drained the News snapshot, the settings being the session's `inline_image_render_settings`; `App::lounge_card_shown` gates the reading hold; `App::live_strip_shown` (the card, or Zen holding a Live tile) gates the aim's half-tick repaint and the wake hint's half tier.
- `app/render.rs`: builds the view when `home_selected`, never for the chat center, and for Zen while it holds a Live tile (`ZenView::live`).
- `app/zen`: `TileKind::Live`; `zen/ui.rs::draw_live_tile` draws the strip or, with nothing up, the note beside the activity feed; `zen/input.rs::handle_live` sends Enter to `open_from_key`.
- `app/chat/ui.rs`: `DashboardChatView.live_strip` + `live_strip_hit`; `draw_dashboard_chat_card` carves the strip off the top of the messages, above the poll strip. While it is up the room header drops its topic row and closing rule (stream and voice rows stay).
- `app/input.rs`: `o` and `r` in `handle_global_key`, gated on `App::lounge_card_shown` and no composer; `r` also on no message being selected, so `r` on a selected message still replies to the message. The click, on either surface, gated on `chat_scroll_clicks_blocked` and taken before a click focuses a Zen tile.

## 8. Across replicas

The pick reads only stamps that live in the database, so it is as shared as the snapshot that carries them.
- Daily matches and results: every write fires `daily_match_changed` and every replica re-reads (`../lobby/daily/CONTEXT.md`). The aim stays on the writer's replica by choice.
- News articles: every `articles` write fires `articles_changed` and every replica re-reads its snapshot (`../chat/CONTEXT.md` §11 News), so an article reaches every strip.
- Booth tracks: the queue snapshot is published by the replica that handled the write and by its own reconcile; `../audio/CONTEXT.md` documents audio as single-replica. A track queued on another replica reaches this one's strip only when this one next publishes.

## 9. Tests

- `pick_test.rs`: the lanes handing over at each minimum with a link going first and the strip coming down at the max; a burst of links with one dropped past `LIVE_MAX_WAIT`; a session keeping its minimum when a match moves again, and not when its source is gone; the aim drawn over a move and an empty strip but not a link, and not past its window.
- `state_test.rs`: a track holding its two minutes, a move, the match's result joining in its place, a link going up at the next handover and the strip coming down, asserted as the whole of what the strip shows and opens at each step; the height held under a selection.
- `ui_test.rs`: the form picked by the card's size, and by a Zen tile's; the hint and the rule drawn on the card and not in a tile; a picture wider than its column cut clear of the words; every key in a hint lit, one faint run once it no longer fits.
- Bodies are tested with their source: `../lobby/daily/live_strip_test.rs`, `../audio/booth/live_test.rs`, `../chat/news/live_test.rs`; results in `../lobby/daily/state_test.rs` (offered on a replica that wrote nothing, outliving their row once both players saw them).
- The Zen tile: its empty 30/70 split in `../zen/ui_test.rs`, Enter on it opening a shared article in `../input_flow_test.rs`.
- End to end in `../dashboard_flow_test.rs`: `o` opening a match, closing back to the card, `o` on a booth track tuning in then opening the booth, `o` on a track that left the booth changing nothing, `o` on a shared article opening the article modal, and `r` on one sending a reply that quotes its title.
