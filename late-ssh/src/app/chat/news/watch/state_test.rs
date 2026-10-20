use std::time::{Duration, Instant};

use chrono::Utc;
use late_core::models::article::{Article, ArticleFeedItem};
use late_core::models::article_body::BodyStatus;
use late_core::models::article_read::WatchPosition;
use ratatui::layout::Rect;
use uuid::Uuid;

use super::*;
use crate::app::audio::client_state::{ClientKind, VideoBlocker};

const URL: &str = "https://www.youtube.com/watch?v=dQw4w9WgXcQ";

fn video() -> ArticleFeedItem {
    let now = Utc::now();
    ArticleFeedItem {
        article: Article {
            id: Uuid::now_v7(),
            created: now,
            updated: now,
            user_id: Uuid::now_v7(),
            url: URL.to_string(),
            title: "Never gonna".to_string(),
            summary: "• one".to_string(),
            ascii_art: String::new(),
        },
        author_username: "mat".to_string(),
        body: BodyStatus::Unavailable,
    }
}

/// A fresh player drawn over a 200x50 terminal, as `ui::draw` leaves it.
fn drawn_player(now: Instant) -> Player {
    drawn_resuming_player(now, None)
}

/// A player drawn over a 200x50 terminal, resuming from `resume_from`.
fn drawn_resuming_player(now: Instant, resume_from: Option<WatchPosition>) -> Player {
    let player = Player::new(&video(), 7, now, resume_from);
    player.set_drawn(Player::layout(player.size(), Rect::new(0, 0, 200, 50)));
    player
}

fn at(position_s: u64, duration_s: Option<u64>) -> WatchPosition {
    WatchPosition {
        position_s,
        duration_s,
    }
}

#[test]
fn window_layout_is_a_capped_centered_box_at_two_sizes() {
    assert_eq!(
        Player::layout(WatchSize::Window, Rect::new(0, 0, 200, 50)),
        WatchLayout {
            // 90 percent of 200 is 180, capped at 160; 90 percent of 50 is 45.
            // The 5 spare rows split 3 above, 2 below.
            popup: Rect::new(20, 3, 160, 45),
            header: Rect::new(21, 4, 158, 2),
            body: Rect::new(21, 6, 158, 40),
            footer: Rect::new(21, 46, 158, 1),
            close: Rect::new(176, 3, 3, 1),
        }
    );
    assert_eq!(
        Player::layout(WatchSize::Window, Rect::new(0, 0, 100, 30)),
        WatchLayout {
            popup: Rect::new(5, 2, 90, 27),
            header: Rect::new(6, 3, 88, 2),
            body: Rect::new(6, 5, 88, 22),
            footer: Rect::new(6, 27, 88, 1),
            close: Rect::new(91, 2, 3, 1),
        }
    );
}

#[test]
fn full_layout_is_every_row_but_the_footer() {
    assert_eq!(
        Player::layout(WatchSize::Full, Rect::new(0, 0, 200, 50)),
        WatchLayout {
            popup: Rect::new(0, 0, 200, 50),
            header: Rect::default(),
            body: Rect::new(0, 0, 200, 49),
            footer: Rect::new(0, 49, 200, 1),
            close: Rect::default(),
        }
    );
    assert_eq!(
        Player::layout(WatchSize::Full, Rect::new(0, 0, 80, 24)),
        WatchLayout {
            popup: Rect::new(0, 0, 80, 24),
            header: Rect::default(),
            body: Rect::new(0, 0, 80, 23),
            footer: Rect::new(0, 23, 80, 1),
            close: Rect::default(),
        }
    );
}

/// A terminal too small for a body reserves nothing, so nothing plays.
#[test]
fn a_tiny_terminal_has_no_body_and_wants_no_play() {
    let player = Player::new(&video(), 1, Instant::now(), None);
    player.set_drawn(Player::layout(WatchSize::Window, Rect::new(0, 0, 10, 6)));
    assert!(player.drawn().body.is_empty());
    assert_eq!(player.wanted(), None);
}

/// Open, `f`, close: the CLI hears a play with the window's body, a place
/// with the full-screen body, then a stop, and nothing in between.
#[test]
fn sync_plays_places_and_stops_from_what_was_drawn() {
    let area = Rect::new(0, 0, 200, 50);
    let mut player = drawn_player(Instant::now());
    let mut live = None;

    assert_eq!(
        sync(&mut live, player.wanted()),
        vec![PairControlMessage::VideoPlay {
            play_id: 7,
            url: URL.to_string(),
            left: 21,
            top: 6,
            cols: 158,
            rows: 40,
            start_s: 0,
        }]
    );
    assert_eq!(
        sync(&mut live, player.wanted()),
        Vec::new(),
        "an unchanged draw sends nothing"
    );

    player.toggle_size();
    assert_eq!(player.size(), WatchSize::Full);
    player.set_drawn(Player::layout(player.size(), area));
    assert_eq!(
        sync(&mut live, player.wanted()),
        vec![PairControlMessage::VideoPlace {
            play_id: 7,
            left: 0,
            top: 0,
            cols: 200,
            rows: 49,
        }]
    );

    assert_eq!(
        sync(&mut live, None),
        vec![PairControlMessage::VideoStop { play_id: 7 }]
    );
    assert_eq!(live, None);
    assert_eq!(sync(&mut live, None), Vec::new());
}

/// A new play while the CLI still holds an older one stops the old first.
#[test]
fn sync_replaces_an_older_play() {
    let rect = Rect::new(1, 1, 10, 5);
    let mut live = Some(LivePlay { play_id: 3, rect });
    assert_eq!(
        sync(
            &mut live,
            Some(WantedPlay {
                play_id: 4,
                url: URL,
                rect,
                start_s: 1391,
            })
        ),
        vec![
            PairControlMessage::VideoStop { play_id: 3 },
            PairControlMessage::VideoPlay {
                play_id: 4,
                url: URL.to_string(),
                left: 1,
                top: 1,
                cols: 10,
                rows: 5,
                start_s: 1391,
            },
        ]
    );
    assert_eq!(live, Some(LivePlay { play_id: 4, rect }));
}

/// The reports a play goes through, each one's status and outcome, and the
/// footer's status text for it.
#[test]
fn reports_drive_the_status_and_report_each_outcome_once() {
    let now = Instant::now();
    let mut player = drawn_player(now);
    assert_eq!(player.status_text(now), "starting…");

    let steps = [
        (
            VideoReport::Starting,
            None,
            WatchStatus::Starting,
            "starting…",
        ),
        (
            VideoReport::Playing {
                position_s: 83,
                duration_s: Some(724),
            },
            Some(WatchOutcome::Played),
            WatchStatus::Playing(at(83, Some(724))),
            "▶ 1:23 / 12:04",
        ),
        (
            VideoReport::Paused {
                position_s: 84,
                duration_s: Some(724),
            },
            None,
            WatchStatus::Paused(at(84, Some(724))),
            "⏸ 1:24 / 12:04",
        ),
        (
            VideoReport::Playing {
                position_s: 3725,
                duration_s: None,
            },
            None,
            WatchStatus::Playing(at(3725, None)),
            "▶ 1:02:05",
        ),
        (
            VideoReport::Ended,
            Some(WatchOutcome::Ended),
            WatchStatus::Ended,
            "ended",
        ),
        // Final: a late report after the end changes nothing.
        (
            VideoReport::Playing {
                position_s: 1,
                duration_s: None,
            },
            None,
            WatchStatus::Ended,
            "ended",
        ),
    ];
    for (report, outcome, status, text) in steps {
        assert_eq!(player.apply(&report, now), outcome, "{report:?}");
        assert_eq!(player.status(), status, "{report:?}");
        assert_eq!(player.status_text(now), text, "{report:?}");
    }
    assert_eq!(
        player.wanted(),
        None,
        "an ended play wants nothing from the CLI"
    );
    assert_eq!(player.control(VideoControl::TogglePause), None);
}

#[test]
fn a_failed_play_shows_the_web_line_and_stops_relaying_keys() {
    let now = Instant::now();
    let mut player = drawn_player(now);
    assert_eq!(
        player.control(VideoControl::SeekForward),
        Some(PairControlMessage::VideoControl {
            play_id: 7,
            control: VideoControl::SeekForward,
        })
    );
    assert_eq!(player.fallback_line(), None);
    assert_eq!(
        player.apply(
            &VideoReport::Failed {
                message: "mpv exited with status 2".to_string()
            },
            now
        ),
        Some(WatchOutcome::Failed)
    );
    assert_eq!(player.status_text(now), "couldn't play");
    assert_eq!(
        player.fallback_line(),
        Some(format!("Watch on the web: {URL}"))
    );
    assert_eq!(player.control(VideoControl::SeekForward), None);
}

/// A play that is still starting is never lost on silence: the CLI says
/// nothing between its one `starting` and mpv's first position, and a
/// yt-dlp resolve can take longer than the timeout. Once a position came
/// in, ten seconds with no report loses the play, once; a report after
/// that is late (the stop already went out) and changes nothing.
#[test]
fn a_silent_cli_is_lost_after_ten_seconds_only_once_it_played() {
    let start = Instant::now();
    let mut player = drawn_player(start);
    assert_eq!(player.expire(start + Duration::from_secs(9)), None);
    assert_eq!(player.expire(start + Duration::from_secs(60)), None);
    assert_eq!(player.status_text(start), "starting…");
    assert!(player.wanted().is_some(), "a slow start is still wanted");

    let played = start + Duration::from_secs(61);
    player.apply(
        &VideoReport::Playing {
            position_s: 1,
            duration_s: Some(10),
        },
        played,
    );
    assert_eq!(player.expire(played + Duration::from_secs(9)), None);
    assert_eq!(
        player.expire(played + WATCH_LOST_AFTER),
        Some(WatchOutcome::Lost)
    );
    assert_eq!(player.status_text(played), "lost the CLI");
    assert_eq!(
        player.fallback_line(),
        Some(format!("Watch on the web: {URL}"))
    );
    assert_eq!(player.expire(played + Duration::from_secs(60)), None);
    assert_eq!(player.wanted(), None);

    let later = played + Duration::from_secs(61);
    assert_eq!(
        player.apply(
            &VideoReport::Paused {
                position_s: 5,
                duration_s: Some(10),
            },
            later,
        ),
        None
    );
    assert_eq!(player.status(), WatchStatus::Lost);
    assert_eq!(player.wanted(), None, "a lost play never asks for a replay");
}

#[test]
fn footer_names_the_size_f_goes_to() {
    let mut player = drawn_player(Instant::now());
    let keys = |player: &Player| {
        player
            .footer_hints()
            .into_iter()
            .map(|(key, hint)| format!("{key} {hint}"))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        keys(&player),
        [
            "Space pause",
            "←/→ seek 10s",
            "⇧←/→ 60s",
            "0 restart",
            "m mute",
            "f full"
        ]
    );
    player.toggle_size();
    assert_eq!(
        keys(&player),
        [
            "Space pause",
            "←/→ seek 10s",
            "⇧←/→ 60s",
            "0 restart",
            "m mute",
            "f window"
        ]
    );
}

/// A video left at 23:14 resumes three seconds before it: the play asks
/// the CLI for that start, the footer says so while it starts and for four
/// seconds once it plays, and then reads as any play.
#[test]
fn a_resumed_play_starts_before_the_saved_position_and_says_so() {
    let opened = Instant::now();
    let mut player = drawn_resuming_player(opened, Some(at(1394, Some(2910))));
    assert_eq!(player.status_text(opened), "resuming from 23:14…");
    assert_eq!(
        player.wanted().map(|wanted| wanted.start_s),
        Some(1391),
        "the rewind lands on the sentence that was playing"
    );

    let played = opened + Duration::from_secs(2);
    assert_eq!(
        player.apply(
            &VideoReport::Playing {
                position_s: 1391,
                duration_s: Some(2910),
            },
            played
        ),
        Some(WatchOutcome::Played)
    );
    assert_eq!(player.status_text(played), "▶ 23:11 / 48:30 · resumed");
    assert_eq!(
        player.status_text(played + WATCH_RESUME_NOTE_FOR),
        "▶ 23:11 / 48:30"
    );

    let tiny = Player::new(&video(), 2, opened, Some(at(2, Some(90))));
    assert_eq!(tiny.start_s(), 0, "a position inside the rewind is the top");
}

/// What a closing player leaves for the next watch, from every state it
/// can close in.
#[test]
fn closing_saves_partway_clears_at_the_end_and_skips_the_first_seconds() {
    let now = Instant::now();
    let playing = |position_s, duration_s| VideoReport::Playing {
        position_s,
        duration_s,
    };
    let cases: Vec<(&str, Option<WatchPosition>, Vec<VideoReport>, WatchMemory)> = vec![
        ("nothing came in", None, vec![], WatchMemory::Nothing),
        (
            "a fresh play stopped at once",
            None,
            vec![playing(9, Some(2910))],
            WatchMemory::Nothing,
        ),
        (
            "a resumed play taken back to the top and stopped",
            Some(at(1394, Some(2910))),
            vec![playing(4, Some(2910))],
            WatchMemory::Clear,
        ),
        (
            "partway through",
            None,
            vec![playing(10, Some(2910)), playing(1394, Some(2910))],
            WatchMemory::Save(at(1394, Some(2910))),
        ),
        (
            "paused partway through",
            None,
            vec![
                playing(500, Some(2910)),
                VideoReport::Paused {
                    position_s: 501,
                    duration_s: Some(2910),
                },
            ],
            WatchMemory::Save(at(501, Some(2910))),
        ),
        (
            "a live stream, no duration",
            None,
            vec![playing(700, None)],
            WatchMemory::Save(at(700, None)),
        ),
        (
            "the last half minute",
            None,
            vec![playing(2880, Some(2910))],
            WatchMemory::Clear,
        ),
        (
            "watched to the end",
            None,
            vec![playing(2900, Some(2910)), VideoReport::Ended],
            WatchMemory::Clear,
        ),
        (
            "the CLI failed mid-play",
            None,
            vec![
                playing(1394, Some(2910)),
                VideoReport::Failed {
                    message: "mpv exited".to_string(),
                },
            ],
            WatchMemory::Save(at(1394, Some(2910))),
        ),
    ];
    for (name, resume_from, reports, expected) in cases {
        let mut player = drawn_resuming_player(now, resume_from);
        for report in &reports {
            player.apply(report, now);
        }
        assert_eq!(player.memory(), expected, "{name}");
    }

    let mut lost = drawn_player(now);
    lost.apply(&playing(1394, Some(2910)), now);
    lost.expire(now + WATCH_LOST_AFTER);
    assert_eq!(
        lost.memory(),
        WatchMemory::Save(at(1394, Some(2910))),
        "a lost CLI keeps the last position"
    );
}

/// The position is saved every fifteen seconds of play, never inside the
/// first ten seconds, and opening counts as the first save.
#[test]
fn the_playing_position_is_checkpointed_every_fifteen_seconds() {
    let opened = Instant::now();
    let mut player = drawn_player(opened);
    assert_eq!(player.checkpoint(opened), None, "nothing reported yet");

    let mut saved = Vec::new();
    for second in 1..=40u64 {
        let now = opened + Duration::from_secs(second);
        player.apply(
            &VideoReport::Playing {
                position_s: second,
                duration_s: Some(2910),
            },
            now,
        );
        if let Some(position) = player.checkpoint(now) {
            saved.push((second, position));
        }
    }
    assert_eq!(
        saved,
        vec![(15, at(15, Some(2910))), (30, at(30, Some(2910))),]
    );

    let mut early = drawn_player(opened);
    let later = opened + Duration::from_secs(60);
    early.apply(
        &VideoReport::Playing {
            position_s: 5,
            duration_s: Some(2910),
        },
        later,
    );
    assert_eq!(
        early.checkpoint(later),
        None,
        "a position inside the first ten seconds is never saved"
    );
}

fn cli(capabilities: &[&str], blockers: &[VideoBlocker]) -> ClientAudioState {
    ClientAudioState {
        client_kind: ClientKind::Cli,
        ssh_mode: ClientSshMode::Native,
        platform: ClientPlatform::Linux,
        capabilities: capabilities.iter().map(|c| c.to_string()).collect(),
        video_blockers: blockers.to_vec(),
        muted: false,
        volume_percent: 30,
    }
}

fn inputs(cli: Option<ClientAudioState>) -> SetupInputs {
    SetupInputs {
        cli,
        protocol: Some(TerminalImageProtocol::Kitty),
        multiplexer_term: false,
        windows_terminal: false,
        images_off: false,
    }
}

fn rows(setup: &SetupBox) -> Vec<(Requirement, Mark)> {
    setup
        .rows()
        .iter()
        .map(|row| (row.requirement, row.mark))
        .collect()
}

/// Plain SSH, the common case: the CLI is the first miss, the tools
/// cannot be known, and the line is the real installer.
#[test]
fn setup_without_a_paired_cli_points_at_the_installer() {
    let setup = SetupBox::decide(URL, &inputs(None));
    assert_eq!(
        rows(&setup),
        vec![
            (Requirement::Cli, Mark::Fails),
            (Requirement::KittyTerminal, Mark::Holds),
            (Requirement::NoMultiplexer, Mark::Holds),
            (Requirement::Mpv, Mark::Unknown),
            (Requirement::YtDlp, Mark::Unknown),
        ]
    );
    assert_eq!(setup.install(), Some(InstallHint::CliInstaller));

    let windows = SetupBox::decide(
        URL,
        &SetupInputs {
            windows_terminal: true,
            protocol: Some(TerminalImageProtocol::Sixel),
            ..inputs(None)
        },
    );
    assert_eq!(windows.install(), Some(InstallHint::CliInstallerWindows));
    assert_eq!(
        windows.rows()[1].mark,
        Mark::Unknown,
        "without a CLI the server's detection only confirms, never rules out"
    );
}

/// Once a CLI reports, its word on the terminal is the one that counts:
/// WezTerm reads as iTerm2 to the server's image detection and still
/// plays, and a CLI in a terminal it cannot draw in fails the row under a
/// server that saw Kitty. A CLI not running natively fails its own row.
#[test]
fn a_reporting_cli_decides_the_terminal_row() {
    let wezterm = SetupBox::decide(
        URL,
        &SetupInputs {
            protocol: Some(TerminalImageProtocol::Iterm2),
            ..inputs(Some(cli(&["voice"], &[VideoBlocker::NoYtDlp])))
        },
    );
    assert_eq!(rows(&wezterm)[1], (Requirement::KittyTerminal, Mark::Holds));

    let not_kitty = SetupBox::decide(
        URL,
        &inputs(Some(cli(&["voice"], &[VideoBlocker::NotKitty]))),
    );
    assert_eq!(
        rows(&not_kitty)[1],
        (Requirement::KittyTerminal, Mark::Fails)
    );

    let openssh = SetupBox::decide(
        URL,
        &inputs(Some(ClientAudioState {
            ssh_mode: ClientSshMode::OpenSsh,
            ..cli(&["voice"], &[VideoBlocker::NotNative])
        })),
    );
    assert_eq!(rows(&openssh)[0], (Requirement::CliNative, Mark::Fails));
    assert_eq!(openssh.install(), None);
}

#[test]
fn setup_in_tmux_with_everything_installed_fails_only_the_multiplexer() {
    let setup = SetupBox::decide(
        URL,
        &SetupInputs {
            multiplexer_term: true,
            protocol: None,
            ..inputs(Some(cli(&["voice"], &[VideoBlocker::Multiplexer])))
        },
    );
    assert_eq!(
        rows(&setup),
        vec![
            (Requirement::Cli, Mark::Holds),
            // tmux hides the terminal from the server, and the CLI's word
            // on it is the one that counts.
            (Requirement::KittyTerminal, Mark::Holds),
            (Requirement::NoMultiplexer, Mark::Fails),
            (Requirement::Mpv, Mark::Holds),
            (Requirement::YtDlp, Mark::Holds),
        ]
    );
    assert_eq!(setup.install(), None);
}

#[test]
fn setup_on_kitty_missing_ytdlp_fails_one_row() {
    let setup = SetupBox::decide(
        URL,
        &inputs(Some(cli(&["voice"], &[VideoBlocker::NoYtDlp]))),
    );
    assert_eq!(
        rows(&setup),
        vec![
            (Requirement::Cli, Mark::Holds),
            (Requirement::KittyTerminal, Mark::Holds),
            (Requirement::NoMultiplexer, Mark::Holds),
            (Requirement::Mpv, Mark::Holds),
            (Requirement::YtDlp, Mark::Fails),
        ]
    );
    assert_eq!(setup.install(), Some(InstallHint::LinuxPackages));

    let mac = SetupBox::decide(
        URL,
        &inputs(Some(ClientAudioState {
            platform: ClientPlatform::Macos,
            ..cli(&[], &[VideoBlocker::MpvTooOld])
        })),
    );
    assert_eq!(rows(&mac)[3], (Requirement::MpvTooOld, Mark::Fails));
    assert_eq!(mac.install(), Some(InstallHint::Brew));
}

/// Everything holds but the Tweaks row is Off: the one case the box can be
/// green otherwise, so that row leads.
#[test]
fn setup_with_images_off_leads_with_the_tweak() {
    let setup = SetupBox::decide(
        URL,
        &SetupInputs {
            images_off: true,
            ..inputs(Some(cli(&["voice", "video"], &[])))
        },
    );
    assert_eq!(
        rows(&setup),
        vec![
            (Requirement::ImagesOn, Mark::Fails),
            (Requirement::Cli, Mark::Holds),
            (Requirement::KittyTerminal, Mark::Holds),
            (Requirement::NoMultiplexer, Mark::Holds),
            (Requirement::Mpv, Mark::Holds),
            (Requirement::YtDlp, Mark::Holds),
        ]
    );
    assert_eq!(setup.install(), None);
}

/// A CLI from before `video` reports neither the capability nor blockers:
/// it needs updating, and its tools cannot be known.
#[test]
fn setup_with_a_cli_that_predates_video_asks_for_an_update() {
    let setup = SetupBox::decide(URL, &inputs(Some(cli(&["voice"], &[]))));
    assert_eq!(
        rows(&setup),
        vec![
            (Requirement::CliUpdate, Mark::Fails),
            (Requirement::KittyTerminal, Mark::Holds),
            (Requirement::NoMultiplexer, Mark::Holds),
            (Requirement::Mpv, Mark::Unknown),
            (Requirement::YtDlp, Mark::Unknown),
        ]
    );
    assert_eq!(setup.install(), Some(InstallHint::CliInstaller));
}

#[test]
fn setup_box_renders_its_rows_and_lines() {
    let setup = SetupBox::decide(URL, &inputs(Some(cli(&["voice"], &[VideoBlocker::NoMpv]))));
    let text: Vec<String> = super::super::ui::setup_lines(&setup)
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>()
        })
        .collect();
    assert_eq!(
        text,
        vec![
            "Videos play right here when your setup has:".to_string(),
            String::new(),
            "✓ the late CLI               run late.sh through `late`, not plain ssh".to_string(),
            "✓ a Kitty-graphics terminal  kitty, Ghostty, WezTerm, Konsole, Rio".to_string(),
            "✓ no tmux or screen          the video is drawn straight to the terminal".to_string(),
            "✗ mpv 0.36 or newer          plays the video".to_string(),
            "✓ yt-dlp                     fetches it for mpv".to_string(),
            String::new(),
            "Install mpv and yt-dlp with your package manager (apt, dnf, pacman, nix)".to_string(),
            format!("Watch on the web: {URL}"),
            String::new(),
            "c copy link  Esc".to_string(),
        ]
    );
}
