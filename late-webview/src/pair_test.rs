use super::*;

fn snapshot(id: &str, video_id: &str, started_at_ms: Option<i64>) -> QueueItemSnapshot {
    QueueItemSnapshot {
        id: id.to_string(),
        video_id: video_id.to_string(),
        started_at_ms,
        duration_ms: Some(180_000),
        is_stream: false,
    }
}

fn load(sync: &mut InitialYoutubeSync, item_id: &str, video_id: &str) -> LoadVideoDecision {
    sync.handle_load_at(
        item_id.to_string(),
        video_id.to_string(),
        false,
        Some(25_500),
    )
}

fn dispatched(decision: LoadVideoDecision) -> LoadVideoCommand {
    match decision {
        LoadVideoDecision::Dispatch(command) => command,
        LoadVideoDecision::Buffered => panic!("expected load_video dispatch"),
    }
}

fn assert_buffered(decision: LoadVideoDecision) {
    match decision {
        LoadVideoDecision::Buffered => {}
        LoadVideoDecision::Dispatch(command) => {
            panic!("expected buffered load_video, got {command:?}")
        }
    }
}

#[test]
fn initial_sync_uses_snapshot_once_for_first_matching_load() {
    let mut sync = InitialYoutubeSync::new();
    assert!(
        sync.observe_queue_update_at(Some(snapshot("item-1", "video-1", Some(10_000))), 25_500)
            .is_none()
    );

    assert_eq!(
        dispatched(load(&mut sync, "item-1", "video-1")),
        LoadVideoCommand {
            item_id: "item-1".to_string(),
            video_id: "video-1".to_string(),
            is_stream: false,
            start_seconds: Some(15),
        }
    );
    assert_eq!(
        dispatched(load(&mut sync, "item-1", "video-1")),
        LoadVideoCommand {
            item_id: "item-1".to_string(),
            video_id: "video-1".to_string(),
            is_stream: false,
            start_seconds: None,
        }
    );
}

#[test]
fn initial_sync_buffers_load_until_snapshot_arrives() {
    let mut sync = InitialYoutubeSync::new();
    assert_buffered(load(&mut sync, "item-1", "video-1"));

    assert_eq!(
        sync.observe_queue_update_at(Some(snapshot("item-1", "video-1", Some(10_000))), 25_500)
            .unwrap(),
        LoadVideoCommand {
            item_id: "item-1".to_string(),
            video_id: "video-1".to_string(),
            is_stream: false,
            start_seconds: Some(15),
        }
    );
}

#[test]
fn initial_sync_does_not_arm_later_track_switches() {
    let mut sync = InitialYoutubeSync::new();
    sync.observe_queue_update_at(Some(snapshot("item-1", "video-1", Some(10_000))), 25_000);

    assert_eq!(
        dispatched(sync.handle_load_at(
            "item-1".to_string(),
            "video-1".to_string(),
            false,
            Some(25_000),
        ))
        .start_seconds,
        Some(15)
    );

    sync.observe_queue_update_at(Some(snapshot("item-2", "video-2", Some(30_000))), 45_000);
    assert_eq!(
        dispatched(sync.handle_load_at(
            "item-2".to_string(),
            "video-2".to_string(),
            false,
            Some(45_000),
        ))
        .start_seconds,
        None
    );
}

#[test]
fn initial_sync_dispatches_buffered_load_without_seek_if_it_does_not_match_snapshot() {
    let mut sync = InitialYoutubeSync::new();
    assert_buffered(sync.handle_load_at(
        "fallback".to_string(),
        "fallback-video".to_string(),
        true,
        Some(25_000),
    ));

    assert_eq!(
        sync.observe_queue_update_at(Some(snapshot("item-1", "video-1", Some(10_000))), 25_000)
            .unwrap(),
        LoadVideoCommand {
            item_id: "fallback".to_string(),
            video_id: "fallback-video".to_string(),
            is_stream: true,
            start_seconds: None,
        }
    );
}

#[test]
fn initial_sync_disables_when_initial_snapshot_has_no_current_track() {
    let mut sync = InitialYoutubeSync::new();
    assert!(sync.observe_queue_update_at(None, 20_000).is_none());
    sync.observe_queue_update_at(Some(snapshot("item-1", "video-1", Some(10_000))), 30_000);

    assert_eq!(
        dispatched(sync.handle_load_at(
            "item-1".to_string(),
            "video-1".to_string(),
            false,
            Some(30_000),
        ))
        .start_seconds,
        None
    );
}

fn sync_item(id: &str, video_id: &str, started_at_ms: i64, is_stream: bool) -> InitialSyncItem {
    InitialSyncItem {
        item_id: id.to_string(),
        video_id: video_id.to_string(),
        started_at_ms,
        duration_ms: Some(180_000),
        is_stream,
    }
}

#[test]
fn resume_command_seeks_to_live_server_position() {
    let item = CurrentItem {
        item_id: "item-1".to_string(),
        video_id: "video-1".to_string(),
    };
    let snap = sync_item("item-1", "video-1", 10_000, false);
    let command = resume_command_at(Some(&item), Some(&snap), Some(40_000)).unwrap();
    assert_eq!(command.start_seconds, Some(30));
    assert_eq!(command.item_id, "item-1");
}

#[test]
fn resume_command_loads_from_start_when_snapshot_is_for_another_item() {
    let item = CurrentItem {
        item_id: "item-2".to_string(),
        video_id: "video-2".to_string(),
    };
    let snap = sync_item("item-1", "video-1", 10_000, false);
    let command = resume_command_at(Some(&item), Some(&snap), Some(40_000)).unwrap();
    assert_eq!(command.start_seconds, None);
    assert_eq!(command.item_id, "item-2");
}

#[test]
fn resume_command_does_not_seek_streams() {
    let item = CurrentItem {
        item_id: "live".to_string(),
        video_id: "live-video".to_string(),
    };
    let snap = sync_item("live", "live-video", 10_000, true);
    let command = resume_command_at(Some(&item), Some(&snap), Some(40_000)).unwrap();
    assert_eq!(command.start_seconds, None);
    assert!(command.is_stream);
}

#[test]
fn resume_command_without_current_item_is_noop() {
    assert!(resume_command_at(None, None, Some(40_000)).is_none());
}

#[test]
fn initial_audio_settings_seed_from_parent_values() {
    let settings = initial_audio_settings(Some("1"), Some("55"));
    assert!(settings.muted);
    assert_eq!(settings.volume_percent, 55);

    let settings = initial_audio_settings(Some("0"), Some("0"));
    assert!(!settings.muted);
    assert_eq!(settings.volume_percent, 0);
}

#[test]
fn initial_audio_settings_fall_back_to_defaults_on_missing_or_invalid_values() {
    let defaults = AudioSettings::default();

    let settings = initial_audio_settings(None, None);
    assert_eq!(settings.muted, defaults.muted);
    assert_eq!(settings.volume_percent, defaults.volume_percent);

    let settings = initial_audio_settings(Some("yes"), Some("150"));
    assert_eq!(settings.muted, defaults.muted);
    assert_eq!(settings.volume_percent, defaults.volume_percent);

    let settings = initial_audio_settings(Some("1"), Some("banana"));
    assert!(settings.muted);
    assert_eq!(settings.volume_percent, defaults.volume_percent);
}
