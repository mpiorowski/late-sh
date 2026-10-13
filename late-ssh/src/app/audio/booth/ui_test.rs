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

fn current_item(video_id: &str) -> QueueItemView {
    QueueItemView {
        id: Uuid::nil(),
        video_id: video_id.to_string(),
        title: Some("Current Track".to_string()),
        channel: Some("Channel".to_string()),
        duration_ms: Some(125_000),
        started_at_ms: None,
        is_stream: false,
        submitter: "mat".to_string(),
        submitter_id: Uuid::nil(),
        vote_score: 0,
        unskippable: false,
        queued_at: chrono::DateTime::<chrono::Utc>::UNIX_EPOCH,
        thumbnail: None,
    }
}

fn render(current: Option<QueueItemView>) -> String {
    use ratatui::{Terminal, backend::TestBackend};

    let snapshot = QueueSnapshot {
        audio_mode: AudioMode::Youtube,
        current,
        queue: Vec::new(),
        history: Vec::new(),
        skip_progress: None,
    };
    let mut state = BoothModalState::default();
    state.open(true);
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).expect("terminal");
    terminal
        .draw(|frame| draw(frame, frame.area(), &state, &snapshot, true, false))
        .expect("draw");
    let buffer = terminal.backend().buffer();
    (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
                .trim_end()
                .to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn now_playing_shows_the_watch_link_under_the_track_and_the_footer_names_copy() {
    let screen = render(Some(current_item("aaaaaaaaaaa")));

    let track_row = screen
        .lines()
        .position(|line| line.contains("▶ Current Track"))
        .expect("current track row");
    let link_row = screen.lines().nth(track_row + 1).expect("link row");
    assert_eq!(
        link_row.trim_matches(|c: char| c == '│' || c.is_whitespace()),
        "https://www.youtube.com/watch?v=aaaaaaaaaaa",
        "{screen}"
    );
    assert!(screen.contains("^y copy  Esc close"), "{screen}");
    assert!(!screen.contains("Esc/q"), "{screen}");
}

#[test]
fn fallback_stream_has_no_link_row() {
    let screen = render(None);

    assert!(screen.contains("fallback stream · YouTube"), "{screen}");
    assert!(!screen.contains("youtube.com/watch"), "{screen}");
}
