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
    for (key, value) in [
        ("TERM_PROGRAM", "ghostty"),
        ("TERM_PROGRAM", "WezTerm"),
        ("TERM_PROGRAM", "rio"),
        ("GHOSTTY_RESOURCES_DIR", "/usr/share/ghostty"),
        ("WEZTERM_EXECUTABLE", "/usr/bin/wezterm-gui"),
        ("KONSOLE_VERSION", "240202"),
    ] {
        let vars = HashMap::from([(key.to_string(), value.to_string())]);
        assert_eq!(
            passing_blockers(&vars),
            vec![],
            "{key}={value} is kitty family"
        );
    }
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
        volume_percent: 30,
        resume: None,
        ipc_path: Path::new("/run/user/1000/late-video-42.sock"),
        ytdlp_override: None,
    });
    assert_eq!(
        args,
        os_args(&[
            "--no-config",
            "--really-quiet",
            "--terminal=no",
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
            "--ytdl-format=bestvideo[height<=720]+bestaudio/best[height<=720]",
            "--volume=30",
            "--keep-open=no",
            "--input-ipc-server=/run/user/1000/late-video-42.sock",
            "--",
            "https://www.youtube.com/watch?v=abc",
        ])
    );
}

/// Full screen after `f`: a respawn that keeps position, pause and mute,
/// with a `LATE_YTDLP_BIN` override. Cells that are not whole pixels round
/// down.
#[test]
fn mpv_args_for_full_screen_resume() {
    let args = mpv_args(&MpvLaunch {
        url: "https://youtu.be/abc",
        rect: VideoRect {
            left: 0,
            top: 0,
            cols: 213,
            rows: 56,
        },
        tty: TtySize::new(213, 57, 1917, 1140).unwrap(),
        volume_percent: 70,
        resume: Some(Resume {
            position_s: 83.46,
            paused: true,
            muted: true,
        }),
        ipc_path: Path::new("/tmp/late-video-7.sock"),
        ytdlp_override: Some(Path::new("/home/me/.local/bin/yt-dlp")),
    });
    assert_eq!(
        args,
        os_args(&[
            "--no-config",
            "--really-quiet",
            "--terminal=no",
            "--vo=kitty",
            "--vo-kitty-use-shm=yes",
            "--vo-kitty-alt-screen=no",
            "--vo-kitty-config-clear=no",
            "--vo-kitty-left=1",
            "--vo-kitty-top=1",
            "--vo-kitty-cols=213",
            "--vo-kitty-rows=56",
            "--vo-kitty-width=1917",
            "--vo-kitty-height=1120",
            "--ytdl=yes",
            "--ytdl-format=bestvideo[height<=720]+bestaudio/best[height<=720]",
            "--script-opts=ytdl_hook-ytdl_path=/home/me/.local/bin/yt-dlp",
            "--volume=70",
            "--keep-open=no",
            "--start=83.5",
            "--pause",
            "--mute=yes",
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
            VideoState::Failed(VideoFailure::Exited(Some(2))),
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

fn idle_player() -> VideoPlayer {
    VideoPlayer::new(VideoTools {
        mpv: PathBuf::from("/nonexistent/mpv"),
        ytdlp_override: None,
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
    assert_eq!(player.place(7, rect).await, None);
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
        30,
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
