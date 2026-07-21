use super::*;
use uuid::Uuid;

fn history_item(video_id: &str) -> HistoryItemView {
    HistoryItemView {
        id: Uuid::nil(),
        video_id: video_id.to_string(),
        title: Some("Current Track".to_string()),
        channel: Some("Channel".to_string()),
        duration_ms: Some(125_000),
        is_stream: false,
        play_count: 2,
        last_played_at_ms: 0,
        vote_score: 4,
    }
}

fn line_text(line: &Line<'_>) -> String {
    line.spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect()
}

#[test]
fn history_line_marks_current_track() {
    let line = history_line(&history_item("abc123"), false, true, 80);

    assert!(line_text(&line).starts_with(" ▶ Current Track"));
}

#[test]
fn selected_history_line_keeps_cursor_when_not_current() {
    let line = history_line(&history_item("abc123"), true, false, 80);

    assert!(line_text(&line).starts_with(" › Current Track"));
}
