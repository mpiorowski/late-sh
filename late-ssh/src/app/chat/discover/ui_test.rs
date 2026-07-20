use crate::app::chat::svc::DiscoverRoomItem;
use crate::app::chat::discover::ui::*;
use crate::app::chat::svc::PreviewMessage;
use chrono::Utc;
use ratatui::{Terminal, backend::TestBackend};
use uuid::Uuid;

fn discover_item(slug: &str, members: i64, messages: i64) -> DiscoverRoomItem {
    DiscoverRoomItem {
        room_id: Uuid::from_u128(1),
        slug: slug.to_string(),
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
        },
        96,
    );

    assert!(rendered.contains("No messages yet."));
}
