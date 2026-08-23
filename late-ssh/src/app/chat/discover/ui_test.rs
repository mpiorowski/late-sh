use crate::app::chat::discover::state::SortMode;
use crate::app::chat::discover::ui::*;
use crate::app::chat::svc::DiscoverRoomItem;
use crate::app::chat::svc::PreviewMessage;
use chrono::Utc;
use ratatui::layout::Rect;
use ratatui::{Terminal, backend::TestBackend};
use uuid::Uuid;

fn discover_item(slug: &str, members: i64, messages: i64) -> DiscoverRoomItem {
    DiscoverRoomItem {
        room_id: Uuid::from_u128(1),
        slug: slug.to_string(),
        topic: None,
        member_count: members,
        message_count: messages,
        last_message_at: Some(Utc::now()),
        recent: Vec::new(),
    }
}

fn with_recent(mut item: DiscoverRoomItem, recent: &[(&str, &str)]) -> DiscoverRoomItem {
    item.recent = recent
        .iter()
        .map(|(author, body)| PreviewMessage {
            author: author.to_string(),
            body: body.to_string(),
            created: Utc::now(),
        })
        .collect();
    item
}

fn render_discover(view: DiscoverListView<'_>) -> String {
    render_discover_at(view, 80)
}

fn render_discover_at(view: DiscoverListView<'_>, width: u16) -> String {
    let height = 10;
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("terminal");

    terminal
        .draw(|frame| draw_discover_list(frame, Rect::new(0, 0, width, height), &view))
        .expect("draw");

    let buffer = terminal.backend().buffer();
    let mut rendered = String::new();
    for y in 0..height {
        for x in 0..width {
            rendered.push_str(buffer[(x, y)].symbol());
        }
        rendered.push('\n');
    }
    rendered
}

#[test]
fn loading_state_does_not_claim_there_are_no_rooms() {
    let rendered = render_discover(DiscoverListView {
        items: Vec::new(),
        selected_index: 0,
        loading: true,
        filtering: false,
        query: "",
        sort: SortMode::default(),
    });

    assert!(rendered.contains("Loading rooms..."));
    assert!(!rendered.contains("No public rooms"));
}

#[test]
fn loaded_empty_state_explains_no_discoverable_rooms() {
    let rendered = render_discover(DiscoverListView {
        items: Vec::new(),
        selected_index: 0,
        loading: false,
        filtering: false,
        query: "",
        sort: SortMode::default(),
    });

    assert!(rendered.contains("No public rooms to discover right now."));
}

#[test]
fn empty_filter_result_names_the_query() {
    let rendered = render_discover(DiscoverListView {
        items: Vec::new(),
        selected_index: 0,
        loading: false,
        filtering: true,
        query: "zzz",
        sort: SortMode::default(),
    });

    assert!(rendered.contains("No rooms match \"zzz\"."));
}

#[test]
fn each_room_renders_name_then_stats_on_two_rows() {
    let a = discover_item("rust", 12, 3);
    let b = discover_item("python", 6, 1);
    let rendered = render_discover_at(
        DiscoverListView {
            items: vec![&a, &b],
            selected_index: 0,
            loading: false,
            filtering: false,
            query: "",
            sort: SortMode::default(),
        },
        70,
    );

    let lines: Vec<&str> = rendered.lines().collect();
    // Row one: name on its own line; row two: the stats underneath.
    assert!(lines[0].contains("#rust"));
    assert!(lines[1].contains("12 members"));
    assert!(lines[1].contains("3 messages"));
    // The next room begins two rows down.
    assert!(lines[2].contains("#python"));
}

#[test]
fn preview_shows_recent_messages_of_selected_room() {
    let a = with_recent(
        discover_item("rust", 12, 3),
        &[("alice", "hello rustaceans")],
    );
    let b = with_recent(
        discover_item("python", 6, 1),
        &[("bob", "pythonic greeting")],
    );
    let rendered = render_discover_at(
        DiscoverListView {
            items: vec![&a, &b],
            selected_index: 0,
            loading: false,
            filtering: false,
            query: "",
            sort: SortMode::default(),
        },
        96,
    );

    // The preview tracks the highlighted room (rust), not the other one.
    assert!(rendered.contains("alice"));
    assert!(rendered.contains("hello rustaceans"));
    assert!(!rendered.contains("pythonic greeting"));
}

#[test]
fn preview_follows_selection() {
    let a = with_recent(
        discover_item("rust", 12, 3),
        &[("alice", "hello rustaceans")],
    );
    let b = with_recent(
        discover_item("python", 6, 1),
        &[("bob", "pythonic greeting")],
    );
    let rendered = render_discover_at(
        DiscoverListView {
            items: vec![&a, &b],
            selected_index: 1,
            loading: false,
            filtering: false,
            query: "",
            sort: SortMode::default(),
        },
        96,
    );

    assert!(rendered.contains("pythonic greeting"));
    assert!(!rendered.contains("hello rustaceans"));
}

#[test]
fn preview_hidden_when_too_narrow() {
    let a = with_recent(
        discover_item("rust", 12, 3),
        &[("alice", "hello rustaceans")],
    );
    let rendered = render_discover_at(
        DiscoverListView {
            items: vec![&a],
            selected_index: 0,
            loading: false,
            filtering: false,
            query: "",
            sort: SortMode::default(),
        },
        60,
    );

    // No preview column: the message body never renders.
    assert!(!rendered.contains("hello rustaceans"));
    assert!(rendered.contains("#rust"));
}

#[test]
fn preview_handles_room_with_no_messages() {
    let a = discover_item("rust", 12, 3);
    let rendered = render_discover_at(
        DiscoverListView {
            items: vec![&a],
            selected_index: 0,
            loading: false,
            filtering: false,
            query: "",
            sort: SortMode::default(),
        },
        96,
    );

    assert!(rendered.contains("No messages yet."));
}

#[test]
fn discover_row_shows_the_topic_next_to_the_room_name() {
    let mut item = discover_item("books", 12, 40);
    item.topic = Some("what we are reading".to_string());
    let rendered = render_discover(DiscoverListView {
        items: vec![&item],
        selected_index: 0,
        query: "",
        filtering: false,
        loading: false,
        sort: SortMode::default(),
    });
    assert!(rendered.contains("#books"));
    assert!(
        rendered.contains("what we are reading"),
        "the topic is what tells someone whether to join:\n{rendered}"
    );
}

#[test]
fn discover_row_without_a_topic_is_unchanged() {
    let item = discover_item("books", 12, 40);
    let rendered = render_discover(DiscoverListView {
        items: vec![&item],
        selected_index: 0,
        query: "",
        filtering: false,
        loading: false,
        sort: SortMode::default(),
    });
    // Only the name row matters here: the stats row carries its own separators.
    let name_row = rendered.lines().next().unwrap_or_default();
    assert!(name_row.contains("#books"));
    assert!(
        !name_row.contains('\u{b7}'),
        "no topic, so no separator on the name row: {name_row}"
    );
}

#[test]
fn discover_preview_shows_what_the_room_is_about() {
    let mut item = discover_item("books", 12, 40);
    item.topic = Some("what we are reading this month".to_string());
    let rendered = render_discover(DiscoverListView {
        items: vec![&item],
        selected_index: 0,
        query: "",
        filtering: false,
        loading: false,
        sort: SortMode::default(),
    });
    // The row has space only for a clipped version, so the preview pane is where
    // the whole description is legible.
    let mut lines = rendered.lines();
    let row = lines.next().unwrap_or_default();
    assert!(
        row.contains("what we are reading"),
        "row shows the start: {row}"
    );
    assert!(
        rendered.contains("what we are reading this month"),
        "the preview shows all of it:\n{rendered}"
    );
}
