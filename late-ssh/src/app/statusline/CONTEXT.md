# Statusline Context

## Metadata
- Scope: `late-ssh/src/app/statusline`, the status bars painted on the app frame's two horizontal borders and on Zen's bottom row, plus their persisted model in `late-core/src/models/statusline.rs` and their customizer in `late-ssh/src/app/settings_modal`.
- Parent context: root `CONTEXT.md`.
- Status: Active

## 1. Shape

Framed pages use one component renderer on both borders. Zen is frameless, so it has no top bar and paints the bottom one on a row of its own (see Zen row below).

- **Top-right bar**: fixed UI policy, not persisted. The pot, then the chips, sharing the row with the page tabs. These are the ambient readings, kept in the one corner that never moves.
- **Bottom-left bar**: the user's arrangement, sharing the row with the sponsor line. By default it is the Keyhints, mentions (DMs counted), voice, live, and the date. All five stay visible while idle (`unread 0`, `mic -`, `live -`), so a newcomer sees the whole default bar and trims it in Settings. Every other reading is opt-in and starts auto-hiding. The default order (`StatusComponent::ALL`) runs most valuable first, since the order is also who keeps their room: Keyhints, mentions, voice, live, date, your move, care, quests, station, pot, chips, users online, time. Beside the sponsor link the five need about 140 columns while idle; on a narrower frame the later ones are dropped whole (at 120: Keyhints, unread, mic). Every title on the bar (the live reading, the station's track) is cut to one cap, 20 columns (`data::TITLE_COLS`, ellipsis past it), so a segment does not outgrow its room the moment something goes live or a long track comes on. That cap is part of the value, not of fitting: fitting still never shortens anything.
- **Move, never duplicate**: a component the bottom bar painted this frame is skipped on the top bar, so turning the pot or the chips on at the bottom moves the reading down. Painted, not merely enabled: a segment the bottom bar had no room for stays on the top. `render.rs` builds the bottom bar first and hands `StatusBar::painted` to `build_top_status_bar`.

- **Zen row**: Zen's last row (`build_zen_status_row`, `Placement::ZenRow`): the page's layout keys on the right (`ZEN_ROW_KEYS`, `zen_keys_line`: `? help  S split  F flip  X close  z zoom`, each key amber, its word dim), a cell of gap, and the bar in what is left, the same segments and fit, left-aligned, joined by faint `·` since there is no border to continue. The keys claim their end first (`title_width` in the fit), and a row narrower than them is all bar. The guide (`?`, Keyhints' `Guide ?`) opens on the Zen topic there. The row is there whenever the page draws, with every component off too (the keys still need it), so the tiles never move with a setting or a count. A page too small to draw (`zen::layout::rice_fits`) builds no row, so its too-small notice keeps no click targets, and a zoomed page has none either (the tile is the whole page, as the screensaver is); every Zen caller of `zen::layout::rice_areas` that draws, clicks, or sizes what is on show passes `zen::layout::rice_row` for the row.

Text labels and icon labels both precede their values (`unread 3`, `chips 1204`).

## 2. Module map

| File | Responsibility |
|---|---|
| `mod.rs` | Declarations only. |
| `data.rs` | `StatusData`, the per-frame inputs gathered once in `App::render`, and the value each component paints, titles cut to `TITLE_COLS`. Pure: the clock and the date arrive pre-formatted, so the draw path reads no wall clock, and the live reading arrives as the strip's own one-row text (`live::ui::status_text`, laid out for `TITLE_COLS`, built only while the Live segment is enabled). The station's track comes from the paired source: the current station's provider feed for radio (Nightride meta or the house Icecast now-playing, via `App::station_now_playing`), or the booth's current `Channel - Title` for YouTube (none while the fallback plays, so it names the station). |
| `bar.rs` | The three passes (build, fit, lay out), the fixed top bar (`build_top_status_bar`), Zen's row (`build_zen_status_row`, its keys `ZEN_ROW_KEYS`), the Keyhints copy, and `click_action`. |
| `late-core/src/models/statusline.rs` | The persisted model: `StatusComponent` roster, `LabelMode`, `StatusVariant`, `StatusComponentSetting`, and the `parse_` / `normalize_` / `_json` trio. |
| `app/render.rs::app_frame_bottom_titles` | Owns the sponsor line: sets its link's width aside, gives the bar the rest of the row, and adds the thanks when the bar leaves room. |
| `app/input.rs::handle_status_bar_click` | Routes a click to the segment under it. |
| `app/render.rs` (`Screen::Zen` arm) | Builds Zen's row and hands it to `zen::ui::draw_rice`, which paints it under the tiles. |
| `app/settings_modal` | The customizer: `StatuslinePane` / `StatuslineDial` in `state.rs`, `draw_statusline_tab` in `ui.rs`, `handle_statusline_input` in `input.rs`. |

## 3. Three passes

No segment knows its own x.

1. `build_segments` turns component settings plus this frame's `StatusData` into spans. Disabled components produce nothing, and so do auto-hiding components that read inactive.
2. `fit` keeps the segments that fit the columns it was given and drops the rest whole.
3. `lay_out` joins the survivors with separators (`─` on the frame, `·` on Zen's row) and converts accumulated widths into click rects, each tagged with its `StatusClick`.

Widths are measured with ratatui's own `Span::width`, the same function that decides which cells a span occupies, so a hit rect cannot disagree with what the user sees. That is what lets segments be reordered, resized, and dropped freely. The rects are rebuilt every frame into `App::last_status_hits`.

## 4. Fitting

There are no render rules beyond this one: **a segment fits whole or is dropped.** Nothing is shortened (no label shedding, no tighter wording), there is no priority dial, and no segment is special, the Keyhints included.

- The list order is the only priority. Segments claim room starting from the frame corner the bar is anchored to (`Placement::claim_order`): the leftmost first on the bottom-left bar, the rightmost first on the top-right one, so the chips outlast the pot.
- Each segment is kept or dropped on its own. One too wide for the room left does not block a narrower one after it.
- Getting a bar that fits a small terminal is the user's job: reorder, switch things off, or set Keyhints to Brief (`⚙ ^o · ⚄ ^g · ◉ ^s`). The default Keyhints are caret notation: `Settings ^O  Lobby ^G  Zen ^F  Shop ^S  Guide ?  Exit qq`.

**The sponsor link has first claim on the bottom row.** `app_frame_bottom_titles` sets the link's width aside and gives the bar the rest. The one thing on the row that flexes is the sponsor's own "thanks for hanging out", shown only when the fitted bar leaves room for it. A row too narrow for the link at all goes to the bar.

## 5. Persisted model

Stored account-wide in `users.settings.statusline_components` as `[{key, enabled, brief, label, auto_hide, variant}]`, in paint order. An absent key reads as the default list. Per-device scoping is not designed.

- `normalize_statusline_components` is the boundary: it drops duplicates, clears `brief` on anything but Keyhints, clears `auto_hide` where `can_auto_hide()` is false, replaces a variant that does not belong to its component with that component's default, and backfills missing components. Interior code trusts the result and does not re-check it.
- A component missing from a stored list backfills at its own `backfill_existing()`: the Keyhints (inserted at the front), voice, mentions, live, and the date are forced on because the frame shows them nowhere else (the live strip is drawn only on the Home #lounge card and a Zen tile); every other component, the station included, appends disabled. A forced-on component other than the Keyhints appends at the end of the stored list, so on an already saved bar live and the date arrive as its last, lowest-priority segments.
- `can_auto_hide()` is true exactly for the components that can read inactive: mentions, pot, your move, quests, care, voice, live. Keyhints, time, date, chips, users online, and station always have a reading, so they get no auto-hide dial. `default_auto_hide()` turns it on only for the opt-in components; what ships enabled stays visible while idle.
- Variants are stored by key, never by index. Each dial is read through one exhaustive match in `data.rs`, so a new `StatusVariant` breaks the build there.

## 6. Icons

Every icon must be Emoji_Presentation, unambiguously two cells wide. A text-default glyph that only becomes emoji through VS16 (`♟️`, `✉️`, `☎️`) is painted at a width the terminal and `unicode-width` disagree about, which slides every hit rect and can overrun the title at the other end of the row. Time's icon is hour-dependent (`clock_icon`) and Keyhints has none. Live is `🔴`, the date `📅`. Brief Keyhints paints literal text glyphs.

## 7. Clicks

`click_action` is the roster of what a segment does: mentions opens Home on the notifications feed, chips opens the Shop, your move opens the Lobby, care opens Zen, station opens the Music Booth, quests goes to The Arcade, users online goes to Profiles, live opens what the live strip shows, as `o` on the #lounge card does (the stream's room, the board, the booth, the article; nothing while the strip is down or holds a result, or on the board of the match it names). Time, date, voice, pot, and Keyhints are readouts and get no hit rect. On Zen's row, care does nothing (the companions are on the page). Every bar feeds the same hit list, `App::last_status_hits`, as `(StatusClick, Rect)`.

## 8. Customizer

Settings > Statusline, the tab after Tweaks. The list on the left reads top to bottom the way the bar reads left to right; the selected component's description and dials sit on the right.

- List: `j`/`k` or arrows select, `Space` toggles, `Enter` opens the dials.
- Dials: `Left`/`Right` or `Space` change the focused one, `Esc` returns to the list.
- `Shift+Up`/`Shift+Down` (or `[`/`]`) reorder from either pane; `Tab`/`Shift+Tab` switch settings tabs from either pane.
- Keyhints offers only Brief. Every other component offers Label, Auto-hide when it can read inactive, and its own variant dial when it has one: the clock (24-hour, AM/PM), mentions, quests, the station, and the date's Format (Short `Thu 1 Oct`, Full `Thursday, 1 October`, ISO `2026-10-01`).

Every change saves immediately, and the frame previews the draft while the modal is open. Switching mentions off leaves no unread counter on the frame; the Mentions entry in the Home rail still carries one.
