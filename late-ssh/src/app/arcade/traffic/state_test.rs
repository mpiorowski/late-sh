use super::*;
use crate::app::arcade::traffic::tracks::DEFAULT_TRACK;

#[test]
fn picker_starts_with_zero_score() {
    let s = State::new();
    assert_eq!(s.screen, TrafficScreen::Picker);
    assert_eq!(s.best_score, 0);
}

#[test]
fn start_track_moves_to_racing() {
    let mut s = State::new();
    s.start_track(DEFAULT_TRACK);
    assert_eq!(s.screen, TrafficScreen::Racing);
    assert_eq!(s.current_stage_idx, 0);
    assert_eq!(
        s.player_lane_idx,
        DEFAULT_TRACK.stages[0].road.lanes.player_start_idx()
    );
}

#[test]
fn move_left_then_right_returns_to_origin() {
    let mut s = State::new();
    s.start_track(DEFAULT_TRACK);
    let start = s.player_lane_idx;
    s.move_left();
    assert!(s.player_lane_idx < start);
    s.move_right();
    assert_eq!(s.player_lane_idx, start);
}

#[test]
fn cannot_drive_above_lane_max_for_long() {
    let mut s = State::new();
    s.start_track(DEFAULT_TRACK);
    let lane = s.current_lane_cfg().unwrap();
    s.player_speed_kmh = lane.own_max_speed + 50.0;
    for _ in 0..30 {
        s.tick();
    }
    let lane = s.current_lane_cfg().unwrap();
    assert!(s.player_speed_kmh <= lane.own_max_speed + 1.0);
}
