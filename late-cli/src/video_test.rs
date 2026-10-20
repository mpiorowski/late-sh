use super::*;

fn kitty_env() -> HashMap<String, String> {
    HashMap::from([
        ("TERM".to_string(), "xterm-kitty".to_string()),
        ("KITTY_WINDOW_ID".to_string(), "1".to_string()),
    ])
}

fn tty() -> TtySize {
    // 200x50 cells of 10x20 pixels.
    TtySize::new(200, 50, 2000, 1000).unwrap()
}

fn passing_blockers(vars: &HashMap<String, String>) -> Vec<VideoBlocker> {
    blockers(
        SshMode::Native,
        "linux",
        vars,
        Some(tty()),
        MpvProbe::Found {
            major: 0,
            minor: 36,
        },
        true,
    )
}

#[test]
fn a_ready_setup_has_no_blockers() {
    assert_eq!(passing_blockers(&kitty_env()), vec![]);
    assert_eq!(kitty_terminal(&kitty_env()), Some(KittyTerminal::Kitty));
    for (key, value, terminal) in [
        ("TERM_PROGRAM", "ghostty", KittyTerminal::Ghostty),
        ("TERM_PROGRAM", "WezTerm", KittyTerminal::WezTerm),
        ("TERM_PROGRAM", "rio", KittyTerminal::Rio),
        (
            "GHOSTTY_RESOURCES_DIR",
            "/usr/share/ghostty",
            KittyTerminal::Ghostty,
        ),
        (
            "WEZTERM_EXECUTABLE",
            "/usr/bin/wezterm-gui",
            KittyTerminal::WezTerm,
        ),
        ("KONSOLE_VERSION", "240202", KittyTerminal::Konsole),
    ] {
        let vars = HashMap::from([(key.to_string(), value.to_string())]);
        assert_eq!(
            passing_blockers(&vars),
            vec![],
            "{key}={value} is kitty family"
        );
        assert_eq!(kitty_terminal(&vars), Some(terminal), "{key}={value}");
    }
}

/// mpv does not fall back when a terminal lacks shared-memory transfer: the
/// result is a black box that reports `playing`. Only the two terminals
/// known to implement it get `use-shm`.
#[test]
fn shared_memory_frames_go_only_to_terminals_that_read_them() {
    let cases = [
        (KittyTerminal::Kitty, true),
        (KittyTerminal::Ghostty, true),
        (KittyTerminal::WezTerm, false),
        (KittyTerminal::Konsole, false),
        (KittyTerminal::Rio, false),
    ];
    for (terminal, shm) in cases {
        assert_eq!(terminal.supports_shm(), shm, "{terminal:?}");
    }
    // `LATE_VIDEO_SHM` overrides the table either way, for trying the
    // other path on a terminal that tears or one the table is wrong about.
    let mut ghostty = HashMap::from([("TERM_PROGRAM".to_string(), "ghostty".to_string())]);
    assert!(use_shared_memory(&ghostty));
    ghostty.insert("LATE_VIDEO_SHM".to_string(), "no".to_string());
    assert!(!use_shared_memory(&ghostty));
    let mut wezterm = HashMap::from([("TERM_PROGRAM".to_string(), "WezTerm".to_string())]);
    assert!(!use_shared_memory(&wezterm));
    wezterm.insert("LATE_VIDEO_SHM".to_string(), "yes".to_string());
    assert!(use_shared_memory(&wezterm));
    wezterm.insert("LATE_VIDEO_SHM".to_string(), "maybe".to_string());
    assert!(
        !use_shared_memory(&wezterm),
        "anything but yes or no is the table"
    );
}

/// `f` and a resize move mpv's output in place: the six geometry options
/// are set over the socket, then the output is torn down and rebuilt so
/// mpv reads them, while the stream and its position carry on. Verified
/// against mpv 0.41: setting the options alone changes nothing, and
/// setting `vo` to its current value is a no-op, so the swap goes
/// through `null`.
#[test]
fn a_place_moves_the_output_in_place_through_the_socket() {
    let full = VideoRect {
        left: 0,
        top: 0,
        cols: 160,
        rows: 39,
    };
    let commands: Vec<String> = place_commands(full, tty())
        .iter()
        .map(ToString::to_string)
        .collect();
    assert_eq!(
        commands,
        [
            r#"{"command":["set_property","vo-kitty-left",1]}"#,
            r#"{"command":["set_property","vo-kitty-top",1]}"#,
            r#"{"command":["set_property","vo-kitty-cols",160]}"#,
            r#"{"command":["set_property","vo-kitty-rows",39]}"#,
            r#"{"command":["set_property","vo-kitty-width",1600]}"#,
            r#"{"command":["set_property","vo-kitty-height",780]}"#,
            r#"{"command":["set_property","vo","null"]}"#,
            r#"{"command":["set_property","vo","kitty"]}"#,
        ]
    );
}

#[test]
fn each_failing_precondition_names_itself() {
    let vars = kitty_env();
    let mpv_ok = MpvProbe::Found {
        major: 0,
        minor: 41,
    };
    let cases = [
        (
            blockers(SshMode::OpenSsh, "linux", &vars, Some(tty()), mpv_ok, true),
            VideoBlocker::NotNative,
        ),
        (
            blockers(
                SshMode::Subprocess,
                "macos",
                &vars,
                Some(tty()),
                mpv_ok,
                true,
            ),
            VideoBlocker::NotNative,
        ),
        (
            blockers(SshMode::Native, "windows", &vars, Some(tty()), mpv_ok, true),
            VideoBlocker::Platform,
        ),
        (
            blockers(SshMode::Native, "linux", &vars, None, mpv_ok, true),
            VideoBlocker::NoPixelSize,
        ),
        (
            blockers(
                SshMode::Native,
                "linux",
                &vars,
                Some(tty()),
                MpvProbe::NotFound,
                true,
            ),
            VideoBlocker::NoMpv,
        ),
        (
            blockers(
                SshMode::Native,
                "linux",
                &vars,
                Some(tty()),
                MpvProbe::Found {
                    major: 0,
                    minor: 35,
                },
                true,
            ),
            VideoBlocker::MpvTooOld,
        ),
        (
            blockers(
                SshMode::Native,
                "linux",
                &vars,
                Some(tty()),
                MpvProbe::Unrecognized,
                true,
            ),
            VideoBlocker::MpvTooOld,
        ),
        (
            blockers(SshMode::Native, "linux", &vars, Some(tty()), mpv_ok, false),
            VideoBlocker::NoYtDlp,
        ),
    ];
    for (found, expected) in cases {
        assert_eq!(found, vec![expected]);
    }

    for (key, value) in [("TMUX", "/tmp/tmux-1000/default,1,0"), ("STY", "1.pts-0")] {
        let mut vars = kitty_env();
        vars.insert(key.to_string(), value.to_string());
        assert_eq!(
            passing_blockers(&vars),
            vec![VideoBlocker::Multiplexer],
            "{key}"
        );
    }
    let in_tmux = HashMap::from([
        ("TERM".to_string(), "tmux-256color".to_string()),
        ("KITTY_WINDOW_ID".to_string(), "1".to_string()),
    ]);
    assert_eq!(passing_blockers(&in_tmux), vec![VideoBlocker::Multiplexer]);
    let plain = HashMap::from([("TERM".to_string(), "xterm-256color".to_string())]);
    assert_eq!(passing_blockers(&plain), vec![VideoBlocker::NotKitty]);
}

#[test]
fn every_failing_precondition_is_reported_at_once() {
    let vars = HashMap::from([("TERM".to_string(), "screen-256color".to_string())]);
    let found = blockers(
        SshMode::OpenSsh,
        "windows",
        &vars,
        None,
        MpvProbe::NotFound,
        false,
    );
    assert_eq!(
        found,
        vec![
            VideoBlocker::NotNative,
            VideoBlocker::Platform,
            VideoBlocker::Multiplexer,
            VideoBlocker::NotKitty,
            VideoBlocker::NoPixelSize,
            VideoBlocker::NoMpv,
            VideoBlocker::NoYtDlp,
        ]
    );
    assert_eq!(
        found
            .iter()
            .map(|blocker| blocker.wire_name())
            .collect::<Vec<_>>(),
        [
            "not_native",
            "platform",
            "multiplexer",
            "not_kitty",
            "no_pixel_size",
            "no_mpv",
            "no_ytdlp"
        ]
    );
}

#[test]
fn a_tty_without_pixels_has_no_size() {
    assert_eq!(TtySize::new(200, 50, 0, 0), None);
    assert_eq!(TtySize::new(0, 0, 2000, 1000), None);
}

#[test]
fn mpv_versions_parse() {
    let cases = [
        (
            "mpv v0.41.0 Copyright © 2000-2025 mpv/MPlayer/mplayer2 projects\n built on ...",
            MpvProbe::Found {
                major: 0,
                minor: 41,
            },
        ),
        (
            "mpv 0.36.0 Copyright",
            MpvProbe::Found {
                major: 0,
                minor: 36,
            },
        ),
        (
            "mpv v0.38.0-dirty Copyright",
            MpvProbe::Found {
                major: 0,
                minor: 38,
            },
        ),
        ("mpv git-2f1c3a Copyright", MpvProbe::Unrecognized),
        ("", MpvProbe::Unrecognized),
    ];
    for (text, expected) in cases {
        assert_eq!(parse_mpv_version(text), expected, "{text:?}");
    }
}

fn os_args(args: &[&str]) -> Vec<OsString> {
    args.iter().map(OsString::from).collect()
}

/// A window, resumed from a saved position: `--start` carries it.
#[test]
fn mpv_args_for_a_window() {
    let args = mpv_args(&MpvLaunch {
        url: "https://www.youtube.com/watch?v=abc",
        rect: VideoRect {
            left: 10,
            top: 3,
            cols: 100,
            rows: 28,
        },
        tty: tty(),
        use_shm: true,
        ipc_path: Path::new("/run/user/1000/late-video-42.sock"),
        ytdlp_override: None,
        log_path: Path::new("/home/me/.local/state/late/video.log"),
        start_s: 1391,
    });
    assert_eq!(
        args,
        os_args(&[
            "--no-config",
            "--really-quiet",
            "--terminal=no",
            "--log-file=/home/me/.local/state/late/video.log",
            "--vo=kitty",
            "--vo-kitty-use-shm=yes",
            "--vo-kitty-alt-screen=no",
            "--vo-kitty-config-clear=no",
            "--vo-kitty-left=11",
            "--vo-kitty-top=4",
            "--vo-kitty-cols=100",
            "--vo-kitty-rows=28",
            "--vo-kitty-width=1000",
            "--vo-kitty-height=560",
            "--ytdl=yes",
            "--ytdl-format=bestvideo[protocol^=m3u8][height<=720]+bestaudio[protocol^=m3u8]/bestvideo[height<=720]+bestaudio/best[height<=720]",
            "--start=1391",
            "--volume=100",
            "--keep-open=no",
            "--input-ipc-server=/run/user/1000/late-video-42.sock",
            "--",
            "https://www.youtube.com/watch?v=abc",
        ])
    );
}

/// Full screen, with a `LATE_YTDLP_BIN` override, on a terminal without
/// shared memory. Cells that are not whole pixels round down.
#[test]
fn mpv_args_for_full_screen_without_shared_memory() {
    let args = mpv_args(&MpvLaunch {
        url: "https://youtu.be/abc",
        rect: VideoRect {
            left: 0,
            top: 0,
            cols: 213,
            rows: 56,
        },
        tty: TtySize::new(213, 57, 1917, 1140).unwrap(),
        use_shm: false,
        ipc_path: Path::new("/tmp/late-video-7.sock"),
        ytdlp_override: Some(Path::new("/home/me/.local/bin/yt-dlp")),
        log_path: Path::new("/tmp/video.log"),
        start_s: 0,
    });
    assert_eq!(
        args,
        os_args(&[
            "--no-config",
            "--really-quiet",
            "--terminal=no",
            "--log-file=/tmp/video.log",
            "--vo=kitty",
            "--vo-kitty-use-shm=no",
            "--vo-kitty-alt-screen=no",
            "--vo-kitty-config-clear=no",
            "--vo-kitty-left=1",
            "--vo-kitty-top=1",
            "--vo-kitty-cols=213",
            "--vo-kitty-rows=56",
            "--vo-kitty-width=1917",
            "--vo-kitty-height=1120",
            "--ytdl=yes",
            "--ytdl-format=bestvideo[protocol^=m3u8][height<=720]+bestaudio[protocol^=m3u8]/bestvideo[height<=720]+bestaudio/best[height<=720]",
            "--script-opts=ytdl_hook-ytdl_path=/home/me/.local/bin/yt-dlp",
            "--volume=100",
            "--keep-open=no",
            "--input-ipc-server=/tmp/late-video-7.sock",
            "--",
            "https://youtu.be/abc",
        ])
    );
}

#[test]
fn every_control_maps_to_its_mpv_command() {
    let cases = [
        (
            VideoControl::TogglePause,
            r#"{"command":["cycle","pause"]}"#,
        ),
        (VideoControl::SeekBack, r#"{"command":["seek",-10]}"#),
        (VideoControl::SeekForward, r#"{"command":["seek",10]}"#),
        (VideoControl::SeekBackFar, r#"{"command":["seek",-60]}"#),
        (VideoControl::SeekForwardFar, r#"{"command":["seek",60]}"#),
        (
            VideoControl::Restart,
            r#"{"command":["seek",0,"absolute"]}"#,
        ),
        (VideoControl::ToggleMute, r#"{"command":["cycle","mute"]}"#),
    ];
    for (control, expected) in cases {
        assert_eq!(ipc_command(control).to_string(), expected, "{control:?}");
    }
}

#[test]
fn controls_parse_from_their_wire_names() {
    for (wire, expected) in [
        ("toggle_pause", VideoControl::TogglePause),
        ("seek_back", VideoControl::SeekBack),
        ("seek_forward", VideoControl::SeekForward),
        ("seek_back_far", VideoControl::SeekBackFar),
        ("seek_forward_far", VideoControl::SeekForwardFar),
        ("restart", VideoControl::Restart),
        ("toggle_mute", VideoControl::ToggleMute),
    ] {
        let parsed: VideoControl = serde_json::from_value(json!(wire)).unwrap();
        assert_eq!(parsed, expected);
    }
    assert!(serde_json::from_value::<VideoControl>(json!("rewind")).is_err());
}

#[test]
fn every_state_has_its_payload() {
    let cases = [
        (
            VideoState::Starting,
            json!({ "event": "video_state", "play_id": 7, "state": "starting" }),
        ),
        (
            VideoState::Playing {
                position_s: 83,
                duration_s: Some(724),
            },
            json!({ "event": "video_state", "play_id": 7, "state": "playing", "position_s": 83, "duration_s": 724 }),
        ),
        (
            VideoState::Paused {
                position_s: 83,
                duration_s: None,
            },
            json!({ "event": "video_state", "play_id": 7, "state": "paused", "position_s": 83, "duration_s": null }),
        ),
        (
            VideoState::Ended,
            json!({ "event": "video_state", "play_id": 7, "state": "ended" }),
        ),
        (
            VideoState::Failed(VideoFailure::Exited {
                code: Some(2),
                reason: None,
            }),
            json!({ "event": "video_state", "play_id": 7, "state": "failed", "message": "mpv exited with status 2" }),
        ),
        (
            VideoState::Stopped,
            json!({ "event": "video_state", "play_id": 7, "state": "stopped" }),
        ),
    ];
    for (state, expected) in cases {
        assert_eq!(video_state_payload(7, &state), expected, "{state:?}");
    }
}

#[test]
fn mpv_properties_read_as_a_state() {
    assert_eq!(playback_state(None, false, None), VideoState::Starting);
    assert_eq!(
        playback_state(Some(83.9), false, Some(724.2)),
        VideoState::Playing {
            position_s: 83,
            duration_s: Some(724)
        }
    );
    assert_eq!(
        playback_state(Some(-0.02), true, None),
        VideoState::Paused {
            position_s: 0,
            duration_s: None
        }
    );
}

/// What mpv 0.41's kitty output actually writes around its frames, captured
/// with stdout on a pipe: on start hide cursor, any-motion mouse on, delete
/// every image; per frame a cursor move and a graphics command; on exit
/// delete every image, show cursor, mouse off, a cursor move.
const MPV_START: &[u8] = b"\x1b[?25l\x1b[?1003h\x1b_Ga=d;\x1b\\";
const MPV_FRAME: &[u8] =
    b"\x1b[2;3f\x1b_Ga=T,t=s,f=24,s=266,v=200,C=1,q=2,m=1;bXB2LWtpdHR5\x1b\\";
const MPV_END: &[u8] = b"\x1b_Ga=d;\x1b\\\x1b[?25h\x1b[?1003l\x1b[40;0f";

/// Feed `reads` through one filter and return what each tty write carries,
/// checking every write is wrapped in a cursor save and restore: the SSH
/// forwarder writes between any two of them, and finds the cursor where it
/// left it.
fn relay_writes(reads: &[&[u8]]) -> Vec<Vec<u8>> {
    let mut filter = TtyFilter::default();
    reads
        .iter()
        .map(|read| filter.feed(read))
        .filter(|write| !write.is_empty())
        .map(|write| {
            match write
                .strip_prefix(b"\x1b7")
                .and_then(|rest| rest.strip_suffix(b"\x1b8"))
            {
                Some(inner) => inner.to_vec(),
                None => panic!("a write without a cursor save and restore: {write:?}"),
            }
        })
        .collect()
}

/// The relay lets only mpv's frames through, whole, however the reads split
/// them: its terminal modes are dropped, its cursor moves travel with the
/// graphics commands they position and never alone, and its delete of every
/// image becomes a delete of the frames it placed.
#[test]
fn the_relay_strips_mpv_terminal_modes_and_passes_frames_whole() {
    let stream = [MPV_START, MPV_FRAME, MPV_FRAME, MPV_END].concat();
    let expected = [
        MPV_FRAME,
        MPV_FRAME,
        b"\x1b_Ga=d,d=P,x=3,y=2,q=2\x1b\\".as_slice(),
    ]
    .concat();

    // Every split point, including inside `ESC[?10`, inside `_Ga=d;` and
    // between ESC and the backslash that closes a graphics command.
    for split in 0..=stream.len() {
        let written = relay_writes(&[&stream[..split], &stream[split..]]).concat();
        assert_eq!(written, expected, "split at {split}");
    }

    // A sequence the read cut in half is held, not written half, and the
    // cursor move before it waits with it.
    let cut = MPV_FRAME.len() / 2;
    assert_eq!(
        relay_writes(&[&MPV_FRAME[..cut], &MPV_FRAME[cut..]]),
        vec![MPV_FRAME.to_vec()]
    );

    // Modes the TUI itself uses are not mpv's and pass untouched.
    assert_eq!(
        relay_writes(&[b"\x1b[?1006h\x1b[?2004htext".as_slice()]),
        vec![b"\x1b[?1006h\x1b[?2004htext".to_vec()]
    );
}

/// Without shared memory a frame is many graphics commands, and the
/// terminal places it at the cursor when the last one lands. The SSH
/// forwarder writes between any two relay writes and moves the cursor, so
/// each command goes out behind mpv's cursor move in the same write: the
/// frame lands in the rectangle, never where the TUI left the cursor.
#[test]
fn a_frame_split_across_reads_is_placed_where_mpv_put_it() {
    let cursor = b"\x1b[4;11f".as_slice();
    let first = b"\x1b_Ga=T,f=24,s=8,v=4,C=1,q=2,m=1;AAAA\x1b\\".as_slice();
    let middle = b"\x1b_Gm=1;BBBB\x1b\\".as_slice();
    let last = b"\x1b_Gm=0;CCCC\x1b\\".as_slice();

    let writes = relay_writes(&[[cursor, first].concat().as_slice(), middle, last]);

    assert_eq!(
        writes,
        vec![
            [cursor, first].concat(),
            [cursor, middle].concat(),
            [cursor, last].concat(),
        ]
    );
}

/// A move tears mpv's output down and rebuilds it in the new rectangle, and
/// frames drawn before the teardown can still be in the pipe when the
/// move's own settle goes out. mpv deletes every image as it tears the
/// output down; the relay turns that into the settle of the rectangle its
/// frames were in, so the delete lands after the last old frame.
#[test]
fn moving_the_output_deletes_the_old_frames_after_the_last_one() {
    let window = VideoRect {
        left: 10,
        top: 3,
        cols: 100,
        rows: 28,
    };
    let full = VideoRect {
        left: 0,
        top: 0,
        cols: 200,
        rows: 49,
    };
    // mpv's cursor moves for `--vo-kitty-top/left` of each.
    let image = b"\x1b_Ga=T,t=s,f=24,s=8,v=4,C=1,q=2,m=1;cGF0aA==\x1b\\".as_slice();
    let in_window = [b"\x1b[4;11f".as_slice(), image].concat();
    let in_full = [b"\x1b[1;1f".as_slice(), image].concat();

    let written = relay_writes(&[
        MPV_START,
        in_window.as_slice(),
        in_window.as_slice(),
        MPV_END,
        MPV_START,
        in_full.as_slice(),
        MPV_END,
    ])
    .concat();

    assert_eq!(
        written,
        [
            in_window.as_slice(),
            in_window.as_slice(),
            settle_sequence(window).as_bytes(),
            in_full.as_slice(),
            settle_sequence(full).as_bytes(),
        ]
        .concat()
    );
}

/// mpv anchors the image at the rectangle's top-left cell, so one cell
/// delete there removes every frame it placed and nothing else. `P` frees
/// the image data too; `q=2` keeps the terminal from answering on stdin.
#[test]
fn settling_deletes_the_placements_at_the_rectangle_origin() {
    let rect = VideoRect {
        left: 10,
        top: 3,
        cols: 100,
        rows: 28,
    };
    assert_eq!(settle_sequence(rect), "\x1b_Ga=d,d=P,x=11,y=4,q=2\x1b\\");
}

fn idle_player() -> VideoPlayer {
    VideoPlayer::new(VideoTools {
        mpv: PathBuf::from("/nonexistent/mpv"),
        ytdlp_override: None,
        use_shm: true,
        startup_tty: Some(tty()),
        ipc_path: PathBuf::from("/nonexistent/late-video.sock"),
        log_path: PathBuf::from("/nonexistent/video.log"),
    })
}

/// Messages for a play this CLI is not running are dropped, not reported.
#[tokio::test]
async fn messages_for_another_play_are_dropped() {
    let mut player = idle_player();
    let rect = VideoRect {
        left: 0,
        top: 0,
        cols: 80,
        rows: 24,
    };
    assert_eq!(player.place(7, rect).await, PlaceOutcome::Unchanged);
    assert_eq!(player.control(7, VideoControl::TogglePause).await, None);
    assert_eq!(player.stop(7).await, None);
    assert_eq!(player.poll().await, None);
    assert_eq!(player.abandon(), None);
}

/// The URL is the one string that becomes a command argument: anything but
/// http(s) fails the play before anything is spawned.
#[test]
fn a_non_http_url_never_reaches_mpv() {
    let mut player = idle_player();
    let update = player.play(
        3,
        "file:///etc/passwd".to_string(),
        VideoRect {
            left: 0,
            top: 0,
            cols: 80,
            rows: 24,
        },
        0,
    );
    assert_eq!(
        update,
        VideoUpdate {
            play_id: 3,
            state: VideoState::Failed(VideoFailure::UnsupportedUrl),
            residue: None,
        }
    );
    assert!(!player.is_active());
}

/// With `--terminal=no` mpv's stderr is silent, so the reason a play
/// failed lives only in its log file. The last error or fatal line is
/// what the server's log gets, in the CLI's own words (mpv exited 2 on
/// YouTube's 403 tonight; the log said why).
#[test]
fn the_last_error_line_of_the_mpv_log_is_the_failure_reason() {
    let log = "\
[   0.001][v][cplayer] Command line options: '--no-config' '--terminal=no'
[   1.934][w][ffmpeg] https: HTTP error 403 Forbidden
[   1.934][e][stream] Failed to open https://rr3---sn-oxup5-f5fy.googlevideo.com/videoplayback?expire=1792468455&ei=h5HWaq7
[   1.934][e][demux] EDL: Could not open source file 'https://rr3---sn-oxup5-f5fy.googlevideo.com/videoplayback?expire=1792468455'.
[   1.934][f][cplayer] No video or audio streams selected.
[   1.935][i][cplayer] Exiting... (Errors when loading file)
[   1.939][v][clipboard/wayland] Deregistering seat 0x1
";
    assert_eq!(
        mpv_log_reason(log),
        Some("[f][cplayer] No video or audio streams selected.".to_string())
    );
    assert_eq!(
        VideoFailure::Exited {
            code: Some(2),
            reason: mpv_log_reason(log),
        }
        .message(),
        "mpv exited with status 2: [f][cplayer] No video or audio streams selected."
    );
    assert_eq!(
        VideoFailure::Exited {
            code: None,
            reason: None,
        }
        .message(),
        "mpv was killed by a signal"
    );
    assert_eq!(mpv_log_reason("[   0.001][v][cplayer] fine\n"), None);
}
