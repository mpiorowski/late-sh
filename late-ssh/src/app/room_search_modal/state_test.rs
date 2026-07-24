use super::*;
use chrono::Utc;

fn room(kind: &str, visibility: &str, slug: Option<&str>) -> ChatRoom {
    ChatRoom {
        id: Uuid::from_u128(1),
        created: Utc::now(),
        updated: Utc::now(),
        kind: kind.to_string(),
        visibility: visibility.to_string(),
        auto_join: false,
        permanent: false,
        slug: slug.map(str::to_string),
        language_code: None,
        dm_user_a: None,
        dm_user_b: None,
        title: None,
        about: None,
        rules: None,
        created_by: None,
    }
}

fn item(label: &str, meta: &str, unread_count: i64) -> RoomSearchItem {
    RoomSearchItem {
        slot: RoomSlot::Room(Uuid::from_u128(1)),
        label: label.to_string(),
        meta: meta.to_string(),
        unread_count,
        last_message_at: None,
        favorite: false,
    }
}

#[test]
fn query_ignores_room_prefixes() {
    assert_eq!(SearchQuery::parse("#lounge").text, "lounge");
    assert_eq!(SearchQuery::parse("@alice").text, "alice");
}

#[test]
fn bare_at_filters_to_dms() {
    assert_eq!(
        SearchQuery::parse("@"),
        SearchQuery {
            kind: SearchQueryKind::Dms,
            text: String::new()
        }
    );
}

#[test]
fn prefixed_queries_select_room_kind() {
    assert_eq!(SearchQuery::parse("@alice").kind, SearchQueryKind::Dms);
    assert_eq!(SearchQuery::parse("#lounge").kind, SearchQueryKind::Rooms);
    assert_eq!(SearchQuery::parse("lounge").kind, SearchQueryKind::All);
}

#[test]
fn bare_at_matches_all_dms() {
    let query = SearchQuery::parse("@");
    assert!(item_matches_query(
        &item("@alice", "direct message", 2),
        &query
    ));
    assert!(item_matches_query(
        &item("@bob", "direct message", 0),
        &query
    ));
    assert!(!item_matches_query(
        &item("#lounge", "core room", 3),
        &query
    ));
}

#[test]
fn named_at_matches_dms_by_name_or_meta() {
    let query = SearchQuery::parse("@ali");
    assert!(item_matches_query(
        &item("@alice", "direct message", 0),
        &query
    ));
    assert!(!item_matches_query(
        &item("#alice", "public room", 0),
        &query
    ));
    assert!(!item_matches_query(
        &item("@bob", "direct message", 0),
        &query
    ));
}

#[test]
fn bare_query_stays_in_rooms_mode() {
    assert_eq!(parse_modal_query("lounge"), ModalQuery::Rooms);
    assert_eq!(parse_modal_query("#rust"), ModalQuery::Rooms);
    assert_eq!(parse_modal_query("@alice"), ModalQuery::Rooms);
}

#[test]
fn question_mark_enters_message_mode() {
    assert_eq!(
        parse_modal_query("?deploy failed"),
        ModalQuery::Messages(MessageQuery {
            scope: None,
            text: "deploy failed".to_string(),
        })
    );
}

#[test]
fn message_mode_scopes_to_room_or_dm() {
    assert_eq!(
        parse_modal_query("?#rust lifetimes"),
        ModalQuery::Messages(MessageQuery {
            scope: Some(MessageScope::Room("rust".to_string())),
            text: "lifetimes".to_string(),
        })
    );
    assert_eq!(
        parse_modal_query("?@Alice that link"),
        ModalQuery::Messages(MessageQuery {
            scope: Some(MessageScope::Dm("alice".to_string())),
            text: "that link".to_string(),
        })
    );
}

#[test]
fn message_mode_mid_scope_token_has_empty_text() {
    assert_eq!(
        parse_modal_query("?#ru"),
        ModalQuery::Messages(MessageQuery {
            scope: Some(MessageScope::Room("ru".to_string())),
            text: String::new(),
        })
    );
}

#[test]
fn delete_word_left_stops_at_room_prefix() {
    let mut state = RoomSearchModalState {
        query: "#lounge chat".to_string(),
        ..RoomSearchModalState::default()
    };
    state.delete_word_left();
    assert_eq!(state.query, "#lounge ");
    state.delete_word_left();
    assert_eq!(state.query, "#");
}

#[test]
fn room_labels_prefix_rooms_and_dms() {
    let current = Uuid::from_u128(1);
    let peer = Uuid::from_u128(2);
    let mut usernames = std::collections::HashMap::new();
    usernames.insert(peer, "alice".to_string());

    let public = room("topic", "public", Some("rust"));
    assert_eq!(room_label(&public, current, &usernames), "#rust");

    let mut dm = room("dm", "dm", None);
    dm.dm_user_a = Some(current);
    dm.dm_user_b = Some(peer);
    assert_eq!(room_label(&dm, current, &usernames), "@alice");
}
