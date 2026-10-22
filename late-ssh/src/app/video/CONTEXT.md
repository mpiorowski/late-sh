# Video

A YouTube video played inside the user's own terminal through the paired CLI's `mpv` (`late-cli/src/video.rs`, `late-cli/VIDEO.md`); not a byte of media crosses SSH. One player for the whole session, opened from two surfaces: a News story (`w` in the News room or the reader, `o` on the live strip or a Live row; `../chat/news/CONTEXT.md`) and a YouTube link in a chat message (Enter or a click on its thumbnail; `../chat/COMMANDS.md`). Held on `App::video`.

## Files

- `state.rs`: pure. `State` (the open `WatchModal`, what the CLI was last told, the `play_id` counter, the session's chat link positions), the `Player` and its `layout`, the `SetupBox` and its `decide`, `sync`, and the closed `WatchSource`, `WatchStatus`, `WatchOutcome`, `WatchMemory`.
- `ui.rs`: draws the player and the setup box over the whole terminal (`app/render.rs`, after the reader).
- `input.rs`: every key and click while the modal is up (`app/input.rs` routes there first).
- `svc.rs`: the gate, opening and closing, where each source's position comes from and goes back to, the sends, the reports, the tick, and the one place a watch is logged and counted.

## Opening

- **What plays** is one rule, `state::youtube_video_id` (`is_watchable` for a yes or no): a YouTube watch, `youtu.be`, embed (no-cookie included), shorts or live link with an 11-character id, the booth's own parse (`audio::youtube::video_id`). A channel or playlist page has no id and is not watchable. News decides its `watch` cards, `w`, the reader and `o` with it; chat decides its thumbnail and Enter with it.
- A caller hands `svc::watch` (or `svc::play`, once it checked `svc::can_play` itself) a `WatchRequest`: the `WatchSource`, the URL mpv gets, a title, who shared it and when. News builds one from the story (`news::State::watch_request`); chat builds one from the selected message (`ChatState::selected_video_in_room`: the canonical `audio::youtube::watch_url` for its id, `YouTube video` for a title).
- **Both gates** decide player or setup box (`svc::can_play`): the paired CLI advertises `video` (`PairedClientRegistry::has_video_cli`, `ClientAudioState::supports_video`; the CLI sits in the terminal and checked it, tmux included, and WezTerm reads as iTerm2 to the server's image detection, so that detection is no gate) and the Tweaks "Terminal images" row is not `Off`. `svc::watch` returns the closed `Watched` (`Player` / `Setup`) so News marks a story read only when it opened.
- Opening closes whatever modal was up first (`svc::close`), so its position is kept. Nothing else on the page gets a key or a click while it is up, so the News reader and the player never coexist: the reader's `w` closes the reader after handing the story over. The page-change and modal-opening glue closes both through one root helper, `app/input.rs::close_reader_and_player`.

## The player

- A ` Watching ` box (90 percent of the terminal, width capped at 160, title and `shared by @mat 2h ago` on one header row, a rule, a footer) or, on `f`, the whole terminal but the footer row (`Player::layout`, pure). Its body is painted black and is the rectangle the CLI gets.
- The modal is pure state: after every draw `svc::sync` asks `State::cli_messages` (the pure `sync` over what the CLI was last told, `LivePlay`, held on `State` past the modal) for the messages that bring the CLI in line, so opening sends `video_play`, `f` or a resize `video_place`, and anything that makes the player go away (Esc, `[x]`, a page change, another modal, the session's `Drop`) `video_stop`, all through one comparison.
- Keys relay `video_control` (`Space` pause, `←`/`→` or `h`/`l` seek 10 s, `Shift+←`/`→` or `H`/`L` a minute, `0` or Home back to the top, `m` mpv's own mute); `c` copies the link; `f` toggles the size.
- `play_id` is a per-session counter on `State`; every send goes through `send_control_to_video_cli`, and one that reaches nobody turns the player `Lost`. The CLI's `video_state` arrives as `SessionMessage::VideoState` (`api.rs`, routed by token like `clipboard_image`, not an `ArticleEvent`, since it belongs to one session, not every session of the user) and `svc::apply_report` updates the player only when its `play_id` matches.
- The footer status reads `starting…`, `▶ 1:23 / 12:04`, `⏸ …`, `ended`, `couldn't play`, or `lost the CLI` (no report for 10 s while playing or paused, `WATCH_LOST_AFTER`, checked on the world tick; a starting play is exempt, since the CLI says nothing between its one `starting` and mpv's first position and a yt-dlp resolve can outlast the timeout; `Lost` is final, the stop already went out, so a late report never replays); the last two show `Watch on the web: <url>` in the body, and a failure raises `Couldn't play the video.` once.
- A video outlasts the idle limit without a key, so the screensaver waits for the modal to close (`App::screensaver_due`).

## Resuming

- A player is told to start `WATCH_RESUME_REWIND_S` (3 s) before where its video was left (`video_play.start_s`, mpv's `--start`; zero for a fresh play), reads `resuming from 23:14…` while it starts and `▶ 23:11 / 48:30 · resumed` for `WATCH_RESUME_NOTE_FOR` (4 s) once it plays.
- What a closing player leaves is one pure decision, `Player::memory` (`WatchMemory`): the last reported position while partway through (`Save`, a failed or lost play included, since `last_seen` outlives the status); a clear once the play `Ended`, stopped inside the last `WATCH_SAVE_END_MARGIN_S` (30 s), or was taken back under `WATCH_SAVE_MIN_S` (10 s) with a position stored (the one it resumed from, or a checkpoint since); nothing when no position ever came in or a play with nothing stored stopped inside its first ten seconds.
- Where it comes from and goes back to is one `match` on `WatchSource` in `svc.rs` each way (`svc::play`, `svc::close`): a `NewsArticle` reads and writes the reader's `article_reads` row through News (`news::State::watch_position`, `remember_watch`, `save_watch_position`; `../chat/news/CONTEXT.md`, Resuming), so it resumes on any machine; a `ChatLink` has no row and lives on `State::link_positions`, keyed by URL, for the session only.
- While a News story plays, `svc::apply_report` asks `Player::checkpoint` after every report, which saves the position every `WATCH_SAVE_EVERY` (15 s) past the first ten seconds while playing or paused, so a dropped session or a dead CLI still resumes close to where it was; a play that is over is the close's to decide, so no final report writes a position the close then clears. A chat link's checkpoint is dropped: the close keeps it, and a dropped session loses it anyway.

## The setup box

` Watch in the terminal `, width capped at 72, lists what a play needs and marks each row `✓` holds, `✗` fails, or `·` unknown, all decided in one pure `SetupBox::decide` over `SetupInputs` (the paired CLI's last `client_state` via `PairedClientRegistry::cli_state`, the protocol, a tmux/screen `TERM`, a Windows Terminal env hint, the Tweaks row): `the late CLI` (✗ with no paired CLI; `late in native mode` ✗ for a paired CLI in OpenSSH or an old mode; a CLI reporting neither `video` nor any `video_blockers` predates video and reads `an up-to-date late CLI`), `a Kitty-graphics terminal` (once a CLI reports, its word: ✓ unless it sent one of `not_kitty`, `no_pixel_size`, `platform`; before that the server's detection only confirms: ✓ on Kitty, `·` otherwise), `no tmux or screen`, `mpv 0.36 or newer` (`mpv is too old` for `mpv_too_old`), `yt-dlp` (both `·` until a CLI that knows about video reports). Tweaks images `Off` leads with `terminal images on`. One install line follows: the CLI installer (`irm …` on Windows Terminal) without a reporting CLI, otherwise by the CLI's platform when a tool is missing (package manager, `brew install mpv yt-dlp`, `Windows is not supported yet`); then `Watch on the web: <url>`. `c` copies, Esc or `[x]` closes.

## Telemetry

`metrics::record_video_watch` labels `late_ssh_video_watches_total` by `source` (`news`, `chat`, from `WatchSource`) and the closed `WatchOutcome` (`opened`, `setup`, `played`, `ended`, `failed`, `lost`). A failure is logged with the CLI's reason (`video_watch_failed`, never shown), a loss with its cause (`video_watch_lost`), the rest at `info`; every line carries the source.

## Tests

`state_test.rs` pins the layouts, `sync`, the report lifecycle, resume, memory and checkpoints, the session's chat link positions, and every setup box row. End to end in `../dashboard_flow_test.rs`: `w` on a News video playing, placing, stopping and resuming through the paired CLI; a play outlasting the idle limit; the setup box without mpv; Enter on a chat message's YouTube link playing and resuming within the session.
