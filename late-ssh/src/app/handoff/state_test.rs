use std::time::Duration;

use uuid::Uuid;

use super::state::{HANDOFF_IDLE, HandoffCheck, ResumePoint};
use crate::app::common::primitives::Page;

const PAGES: [Page; 9] = [
    Page::Clubhouse,
    Page::Dashboard,
    Page::Arcade,
    Page::Games,
    Page::Artboard,
    Page::Profiles,
    Page::Leaderboard,
    Page::Calendars,
    Page::Zen,
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
