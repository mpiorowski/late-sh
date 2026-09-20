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
        topic: None,
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

/// A live stream is pickable before the viewer ever joined its room (the
/// pick joins lazily, like the rail); a pending one is not listed at all.
#[tokio::test]
async fn picker_lists_live_streams_only() {
    use late_core::test_utils::create_test_user;

    let test_db = crate::test_helpers::new_test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let viewer = create_test_user(&test_db.db, "picker_viewer").await;
    let live_owner = create_test_user(&test_db.db, "onair_owner").await;
    let pending_owner = create_test_user(&test_db.db, "pending_owner").await;
    let live_room = ChatRoom::get_or_create_stream_room(&client, "onair_owner", live_owner.id)
        .await
        .expect("live stream room");
    let pending_room =
        ChatRoom::get_or_create_stream_room(&client, "pending_owner", pending_owner.id)
            .await
            .expect("pending stream room");
    let stream = |user_id: Uuid, username: &str, room_id: Uuid, live: bool| {
        crate::app::stream::registry::LiveStreamView {
            user_id,
            username: username.to_string(),
            title: "show".to_string(),
            room_id,
            voice_channel_id: Uuid::now_v7(),
            stream_id: username.to_string(),
            live,
            watching: 0,
            watch_url: String::new(),
        }
    };

    let mut app = crate::test_helpers::make_app(test_db.db.clone(), viewer.id, "picker-streams");
    app.chat.set_live_streams(vec![
        stream(live_owner.id, "onair_owner", live_room.id, true),
        stream(pending_owner.id, "pending_owner", pending_room.id, false),
    ]);

    let slots: Vec<RoomSlot> = filtered_items(&app.chat, viewer.id, PickerScope::AllSlots, "")
        .iter()
        .map(|item| item.slot)
        .collect();
    assert!(
        slots.contains(&RoomSlot::Room(live_room.id)),
        "live stream missing from {slots:?}"
    );
    assert!(
        !slots.contains(&RoomSlot::Room(pending_room.id)),
        "pending stream listed in {slots:?}"
    );

    let hits: Vec<RoomSlot> = filtered_items(&app.chat, viewer.id, PickerScope::AllSlots, "#onair")
        .iter()
        .map(|item| item.slot)
        .collect();
    assert_eq!(hits, vec![RoomSlot::Room(live_room.id)]);
}
