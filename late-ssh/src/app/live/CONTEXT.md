# late-ssh Live Strip Context

## Metadata
- Domain: the live strip, one thing happening in the house painted at the top of the Home #lounge card, with a key to join it
- Primary audience: LLM agents working in `late-ssh/src/app/live` and the sources that feed it
- Status: Active
- Parent context: `../../../../CONTEXT.md`

---

## 1. What it is

A window onto what the room is doing right now, for discovery, living where the room is: the top of the Home #lounge card only (no other room, not the chat center, not the Zen chat tiles, not the sidebar), in the topic row's place while it is up. It cannot be turned off. It is up only while something just happened, so it appearing is the news and a quiet house leaves the card untouched; it never draws an empty state.

One slot. Sources offer candidates, the slot features one at a time, and `o` or a click opens it.

## 2. File map

| File | Role |
|---|---|
| `mod.rs` | Declarations only. |
| `pick.rs` | Pure rules: `LiveSource` (the closed enum of sources), `LiveCandidate`, `Featured`, `pick_featured`, `replay`, `strip_is_fresh`, and the clocks `LIVE_AIM_WINDOW` (8s), `LIVE_HOLD` (60s), `LIVE_STRIP_LINGER` (5 min). |
| `state.rs` | Per-session `LiveState`: the featured source, what the strip shows (`Showing`), the render-recorded `hit` rect. `refresh` is the pure rule over candidates and clocks; `tick` and `view` are the glue that reads `DailyState` and `AudioState`. `LiveStripView` is what gets painted, one variant per kind of body. `LIVE_FINISH_LINGER` (60s). |
| `ui.rs` | The frame: `fit_live_strip` (the form, from the card's size alone), `draw_live_strip`, `live_strip_lines`, `live_strip_compact_line`, the `── live ──` rule, `PICTURE_ROWS` (8), `PICTURE_COLS` (21), `StripBody` (what a source hands the frame). |
| `input.rs` | `open_from_key` (`o`) and `open_from_click`: one exhaustive match on `LiveSource` that says what opening each source does. |

## 3. Sources

| `LiveSource` | Candidates | Body | Opening it |
|---|---|---|---|
| `DailyMatch(match id)` | `DailyState::live_candidates`: every active match, the viewer's own included, stamped with the row's `updated`, plus the shooter's aim while fresh | `lobby/daily/live_strip.rs` over `live_board.rs`: the board, the players, where the match stands, what just happened | `open_board` with `BoardEntry::LoungeStrip`, then `Screen::DailyMatch` |
| `BoothTrack(queue item id)` | `AudioState::live_candidates`: every track playing or queued in the YouTube booth, stamped with when it was queued | `audio/booth/live.rs`: the thumbnail, the title, channel and length, who queued it and where it stands | On another source: switch to YouTube (`App::set_paired_playback_source`). Already on YouTube: open the booth modal |

A daily match that just ended is not a candidate. `DailyState` holds its final board and result (`live_finish_view`, `live_finish_at`) and the slot gives it `LIVE_FINISH_LINGER`; it opens nothing, since its board has left the lobby.

Sources are state, not events: a candidate exists while the thing is still going on (the match is active, the track is in the booth), so the strip never advertises something that is over.

### Adding a source
1. Add a variant to `LiveSource`. The build breaks at `state.rs::view`, `input.rs::open`, and wherever a body is matched.
2. In the source's own domain, offer candidates (a stamp every replica shares) and a view, and paint a `StripBody` plus the one-row form.
3. Add a `LiveStripView` variant if the body is a new kind, and extend `LiveState::tick`.
4. Say what opening it does in `input.rs`.

## 4. The pick (`pick.rs`)

- A candidate somebody is acting on right now (`aimed_at` inside `LIVE_AIM_WINDOW`; today only a pool shooter moving the cue) wins outright, the freshest first.
- Otherwise what the session is showing (`Featured`, the source and when it took the strip) keeps it for `LIVE_HOLD`, as long as it is still a candidate and its stamp is inside `LIVE_STRIP_LINGER`.
- Past that the pick is a replay of the candidates' stamps against the wall clock: whatever takes the strip keeps it for `LIVE_HOLD` from the moment it took it, and the next stamp in line takes over at its own time or the end of that hold, whichever is later. So a track queued and a move played a few seconds apart each get their minute, in order. `LiveSource`'s ordering only breaks a tie between two equal stamps.
- The replay reads only each candidate's latest stamp, so a match that moves again loses its old place in it; the session's hold is what keeps that from flipping the strip early, at the cost of a session following the replay one step behind until the house goes quiet. A session with nothing up starts from the replay, so one connecting mid-hold sees what the room sees.
- A queued candidate whose turn would come after its own linger ran out is skipped, so it never holds an empty strip in front of news that just landed.

## 5. What the strip shows (`state.rs::refresh`)

Run each tick after the daily and audio ticks (`app/tick.rs`). In order: a fresh aim on the featured source; else a daily match that ended inside `LIVE_FINISH_LINGER`; else the featured source while its stamp is inside `LIVE_STRIP_LINGER`; else nothing.

Going up or coming down changes the card's height, so it waits while the viewer has a message selected (`reading`, from `App::lounge_card_shown`), unless what the strip was showing is gone. The messages are bottom-anchored, so at the bottom the newest rows never move. A change of what the strip shows is a `changed` tick; a change of featured source repaints only while the strip is showing it; the aim animation rides the half-tick (`LiveState::aiming`).

## 6. The frame (`ui.rs`)

The picture sits in a `PICTURE_COLS` column, centred in `PICTURE_ROWS`, with the words to its right, then the rule under them. Two fixed forms picked by the card's size and never by what is shown: the 9-row strip on a card at least 56 wide with 8 rows left for messages, else one row (`── live ── 8ball eggy v weslin`, `── live ── booth mat · Naima`), else nothing. The rule's label glows when the source says so: a cue up or a result in for a match, the track that is playing for the booth.

`draw_live_strip` records `LiveState::hit` when what it drew opens something; `App::render` clears it before every draw.

## 7. Wiring outside this directory

- `app/state.rs`: `App::live`.
- `app/tick.rs`: `self.live.tick(&self.daily, &self.audio, reading)`; `App::lounge_card_shown` gates the reading hold, the half-tick repaint and the wake hint's half tier.
- `app/render.rs`: builds the view when `home_selected`, never for the chat center.
- `app/chat/ui.rs`: `DashboardChatView.live_strip` + `live_strip_hit`; `draw_dashboard_chat_card` carves the strip off the top of the messages, above the poll strip. While it is up the room header drops its topic row and closing rule (stream and voice rows stay).
- `app/input.rs`: `o` in `handle_global_key`, gated on `App::lounge_card_shown` and no composer; the click, gated on `chat_scroll_clicks_blocked`.

## 8. Across replicas

The pick reads only stamps that live in the database, so it is as shared as the snapshot that carries them.
- Daily matches: every write fires `daily_match_changed` and every replica re-reads (`../lobby/daily/CONTEXT.md`). The aim stays on the writer's replica by choice.
- Booth tracks: the queue snapshot is published by the replica that handled the write and by its own reconcile; `../audio/CONTEXT.md` documents audio as single-replica. A track queued on another replica reaches this one's strip only when this one next publishes.

## 9. Tests

- `pick_test.rs`: one scenario driving `pick_featured` from the stamps alone (three writes each taking their minute, an aim jumping the queue, the replay resuming), the hold surviving another candidate's newer write, a stale queued candidate skipped, `strip_is_fresh` on either side of the linger and the aim window.
- `state_test.rs`: a booth track and a daily move taking turns with a result cutting in and the strip coming down, asserted as the whole of what the strip shows and opens at each step; the height held under a selection.
- `ui_test.rs`: the form picked by the card's size.
- Bodies are tested with their source: `../lobby/daily/live_strip_test.rs`, `../audio/booth/live_test.rs`.
- End to end in `../dashboard_flow_test.rs`: `o` opening a match, closing back to the card, and `o` on a booth track tuning in then opening the booth.
