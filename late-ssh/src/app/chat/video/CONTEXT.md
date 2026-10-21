# Chat video

Part of the chat domain (`../CONTEXT.md`). A YouTube link in a Home chat room plays inside its own thumbnail, in the user's terminal, through the paired CLI's `mpv` (`late-cli/VIDEO.md`). Not a byte of media crosses SSH. This slice also owns the session's one play on that CLI, which the News player (`../news/CONTEXT.md`, Watching) shares.

## Files

- `state.rs`: pure. The link rules (`youtube_link_in_body`, `youtube_video_id`, the canonical watch link and the `mqdefault` thumbnail), `video_rect` (where the picture sits among a frame's chat rows), and `InlineVideo`, the one inline video with its `Phase` and what the last frame showed of it (`Sight`, `Step`).
- `ui.rs`: paints the rectangle during the chat draw and checks it at the end of the frame (`check_clear`); `with_watch_hint` adds the hint row to a thumbnail's rows.
- `svc.rs`: the gate and keys, the session sync after every draw, delivery, report routing, the tick, and the one place an inline video is logged and counted.

## The thumbnail

- A message whose inline preview source is a YouTube link (`../state.rs::inline_preview_source_in_body`: an image link wins, then a YouTube link) gets its thumbnail fetched through the inline image pipeline (`../COMMANDS.md`) in the image's place. Every viewer sees it, plain SSH included.
- The rows cache appends the hint row, `▶ YouTube · Enter to watch`, under the picture (`../ui.rs`, at cache build, so its colour follows the theme). Picture and hint are the message's `Image` rows, so a click on either is the image click.
- The picture is the video's rectangle: `video_rect` finds the message's run of `Image` rows in the frame's hits and answers only when the whole run (picture and hint) is on screen and the picture fits the width, since mpv draws a whole picture or nothing. A 16:9 thumbnail at the preview's 12-row cap is about 43 columns.

## Playing

- Enter on the selected message (`../input.rs`, after the image modal, which goes first) or a click on its thumbnail (`app/input.rs`) calls `svc::watch_selected`. Home only (`Screen::Dashboard`): the other chat surfaces draw the thumbnail and hint but no video, and Enter there falls through.
- Both gates of the News player apply (`news::watch::svc::can_play`: a paired CLI advertising `video`, Tweaks images not `Off`); failing them opens the News setup box on the link (`news::State::open_setup_box`). A thumbnail that has not loaded banners instead, since there is nowhere to play.
- On the video already there, Enter toggles pause while mpv is up, and plays it again once it is over (`InlineVideo::replayed`: from the top after `ended`, from where it got to after `failed` or `lost`). Esc on Home stops it (`app/input.rs`, before the selection clear). There are no seek or mute keys inline; they all mean something else in chat.
- One inline video at a time; starting another replaces it, and opening the News player closes it (`ChatState::open_watch_player`).
- While it holds the rectangle (mpv up, or parked) the picture is painted black with nothing in it; once the play is over the thumbnail shows again. For as long as the video is there the hint row is its caption (`InlineVideo::caption`: `starting… · Enter pause · Esc stop`, `▶ 1:23 / 12:04 · Enter pause · Esc stop`, `⏸ 1:23 / 12:04 · Enter play · Esc stop`, `ended · Enter to watch again`, `couldn't play · Enter to try again`).
- A live inline video keeps the screensaver away like the News player (`App::screensaver_due`).

## What each frame decides

Every render: `InlineVideo::begin_frame` clears the record, the chat draw (`ui::draw` from `draw_dashboard_chat_card` and `draw_selected_content`, after the room effects, before the overlay and image modal) records the rectangle, and `ui::check_clear` runs last in the draw closure (`app/render.rs`, after the ultimate effects), on the finished buffer: every cell still a plain space on `Color::Black`, or the rectangle is covered. That one check stands for every overlay there is, so no list of modals has to be kept in step: mpv's frames sit above the text, and anything drawn over the rectangle would end up under the video.

`svc::sync` then applies `InlineVideo::step` (`ChatState::settle_inline_video`):

- **Gone** (no whole thumbnail on screen: scrolled off, another room, another page): the video closes.
- **Covered** while mpv is up: it parks. The CLI is sent the stop, and the video keeps the last reported position (or where the play began).
- **Clear** while parked: it comes back under a fresh `play_id`, `WATCH_RESUME_REWIND_S` before the parked position.

Parking stops mpv outright, so coming back costs a fresh yt-dlp resolve; the CLI has no message that hides the picture and keeps playing. A selected message pins near the top of the pane once it gets there (`../CONTEXT.md` §9), so a selected video stops moving as messages arrive; an unselected one moves up with every new message, and each move is a `video_place`.

## The session's play

- `ChatState` holds `live_play` (what the CLI was last told) and the `play_id` counter for both players. `ChatState::video_sync` hands the pure `news::watch::state::sync` the News player's wanted play first, the inline video's otherwise.
- `svc::deliver` sends every pair message for either player; one that reaches no CLI loses whichever play was live. `svc::apply_report` routes a `video_state` to the play holding its `play_id` and drops any other. `svc::tick` expires both, and `svc::end_session` stops both when the session ends.
- The play lifecycle (status, reports, the 10 s loss, keys, status text) is `news::watch::state::Play`, shared with the News player.

## Telemetry

`metrics::record_chat_video` labels `late_ssh_chat_videos_total` by the same closed `WatchOutcome` as the News counter (`opened`, `setup`, `played`, `ended`, `failed`, `lost`). A failure is logged with the CLI's reason (`chat_video_failed`, never shown), a loss with its cause (`chat_video_lost`), a close at `info`, parks at `debug`.
