use std::time::Duration;

use uuid::Uuid;

use super::state::{HANDOFF_IDLE, HandoffCheck, ResumePage, ResumePoint};
use crate::app::common::primitives::Screen;

const PAGES: [ResumePage; 9] = [
    ResumePage::Clubhouse,
    ResumePage::Dashboard,
    ResumePage::Arcade,
    ResumePage::Games,
    ResumePage::Artboard,
    ResumePage::Profiles,
    ResumePage::Leaderboard,
    ResumePage::Calendars,
    ResumePage::Zen,
];

#[test]
fn every_resume_point_reads_back_as_written() {
    let room_id = Uuid::now_v7();
    for page in PAGES {
        for point in [
            ResumePoint {
                page,
                room_id: None,
            },
            ResumePoint {
                page,
                room_id: Some(room_id),
            },
        ] {
            assert_eq!(ResumePoint::parse(&point.encode()), Some(point));
        }
    }
}

#[test]
fn a_resume_point_this_server_did_not_write_is_refused() {
    let room_id = Uuid::now_v7();
    for raw in [
        String::new(),
        "v1".to_string(),
        "v2:dashboard".to_string(),
        "v1:nightcap".to_string(),
        "v1:dashboard:not-a-uuid".to_string(),
        format!("v1:dashboard:{room_id}:extra"),
        format!("v1:dashboard:{room_id}:"),
    ] {
        assert_eq!(ResumePoint::parse(&raw), None, "{raw:?}");
    }
}

#[test]
fn a_page_comes_back_as_itself_and_a_screen_off_the_cycle_as_its_page() {
    for page in PAGES {
        assert_eq!(ResumePage::from_screen(page.screen()), page);
    }
    for (screen, page) in [
        (Screen::Nethack, ResumePage::Games),
        (Screen::Lateania, ResumePage::Games),
        (Screen::Realm, ResumePage::Games),
        (Screen::City, ResumePage::Clubhouse),
        (Screen::Nightcap, ResumePage::Clubhouse),
        (Screen::HouseTable, ResumePage::Dashboard),
        (Screen::DailyMatch, ResumePage::Dashboard),
        (Screen::Scratchpad, ResumePage::Dashboard),
    ] {
        assert_eq!(ResumePage::from_screen(screen), page, "{screen:?}");
    }
}

fn idle() -> HandoffCheck {
    HandoffCheck {
        auto_restart: true,
        idle: HANDOFF_IDLE,
        in_voice: false,
        watching_video: false,
        composer_draft: false,
    }
}

#[test]
fn a_quiet_session_on_a_draining_pod_moves_at_five_minutes() {
    assert!(idle().ready());
    let just_short = HandoffCheck {
        idle: HANDOFF_IDLE - Duration::from_secs(1),
        ..idle()
    };
    assert!(!just_short.ready());
}

#[test]
fn the_setting_and_anything_a_person_does_without_typing_hold_it_in_place() {
    let held = [
        HandoffCheck {
            auto_restart: false,
            ..idle()
        },
        HandoffCheck {
            in_voice: true,
            ..idle()
        },
        HandoffCheck {
            watching_video: true,
            ..idle()
        },
        HandoffCheck {
            composer_draft: true,
            ..idle()
        },
    ];
    for check in held {
        assert!(!check.ready(), "{check:?}");
    }
}
