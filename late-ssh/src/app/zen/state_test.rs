use ratatui::layout::Rect;
use uuid::Uuid;

use super::*;
use crate::app::zen::layout::tile_rects;

fn widths(root: &Node, area: Rect, gap: u16) -> Vec<u16> {
    tile_rects(root, area, gap, None)
        .into_iter()
        .map(|(_, rect)| rect.width)
        .collect()
}

fn heights(root: &Node, area: Rect, gap: u16) -> Vec<u16> {
    tile_rects(root, area, gap, None)
        .into_iter()
        .map(|(_, rect)| rect.height)
        .collect()
}

#[test]
fn one_press_moves_a_row_split_by_exactly_one_column_at_every_width() {
    for width in 40..=320u16 {
        for gap in 0..=1u16 {
            let area = Rect::new(0, 0, width, 30);
            let mut root = Node::split(
                Dir::Row,
                640,
                Node::leaf(TileKind::Bonsai),
                Node::leaf(TileKind::Chat),
            );
            let before = widths(&root, area, gap);
            assert!(root.resize_leaf(0, Dir::Row, 1, area, gap));
            let grown = widths(&root, area, gap);
            assert_eq!(
                grown[0],
                before[0] + 1,
                "width {width} gap {gap}: the first tile grows one column"
            );
            assert_eq!(grown[1], before[1] - 1, "and the second gives it up");

            // Shrinking from the second tile's side lands back where it was.
            assert!(root.resize_leaf(1, Dir::Row, 1, area, gap));
            assert_eq!(widths(&root, area, gap), before);
        }
    }
}

#[test]
fn a_deep_column_split_moves_one_row_and_a_row_only_tree_has_no_height() {
    let area = Rect::new(0, 0, 160, 44);
    let mut root = RiceLayout::default().root;
    // Leaves run bonsai, chat, clock, music, lobby, pet, aquarium. Music sits in
    // a column inside a column inside the rail; only the split directly
    // above it moves, so the clock above and the left column stay put.
    let before = heights(&root, area, 1);
    assert!(root.resize_leaf(3, Dir::Column, 1, area, 1));
    let after = heights(&root, area, 1);
    assert_eq!(after[3], before[3] + 1, "music gains one row");
    assert_eq!(after[0], before[0], "the bonsai column is untouched");
    assert_eq!(after[1], before[1]);
    assert_eq!(after[2], before[2], "the clock is untouched");
    assert_eq!(
        after[4] + after[5] + after[6] + 1,
        before[4] + before[5] + before[6],
        "lobby, pet, and the reef give up one row between them"
    );

    let mut row_only = Node::split(
        Dir::Row,
        500,
        Node::leaf(TileKind::Bonsai),
        Node::leaf(TileKind::Chat),
    );
    assert!(
        !row_only.resize_leaf(0, Dir::Column, 1, area, 1),
        "no column split above the tile: nothing to trade height with"
    );
}

#[test]
fn shares_stay_inside_their_bounds() {
    let area = Rect::new(0, 0, 100, 30);
    let mut root = Node::split(
        Dir::Row,
        MAX_SHARE,
        Node::leaf(TileKind::Bonsai),
        Node::leaf(TileKind::Chat),
    );
    let before = widths(&root, area, 0);
    assert!(root.resize_leaf(0, Dir::Row, 1, area, 0));
    assert_eq!(
        widths(&root, area, 0),
        before,
        "already at the widest share"
    );
    let Node::Split { share, .. } = root else {
        unreachable!()
    };
    assert_eq!(share, MAX_SHARE);
}

#[test]
fn holding_split_stops_at_the_tile_cap_and_the_layout_still_round_trips() {
    // Every `S` splits the focused tile and moves focus into the new one, so
    // a held key nests the tree one level deeper per press. The stored JSON
    // is read back through serde_json, which refuses to parse past 128
    // nested levels; a layout that deep would lock the account out at
    // login. The cap keeps the tree far below that and the JSON readable.
    let mut zen = ZenState::new(RiceLayout::default());
    let mut accepted = 0;
    for _ in 0..400 {
        if zen.split_focused(true) {
            accepted += 1;
        }
    }
    assert_eq!(zen.leaf_count(), MAX_TILES, "splits stop at the cap");
    assert_eq!(
        accepted + RiceLayout::default().root.leaf_count(),
        MAX_TILES,
        "every press past the cap is refused"
    );
    // Nested inside the settings blob (one more level) it must still parse.
    let stored = serde_json::json!({ "zen_layout": zen.rice.to_json() });
    let read_back: RiceLayout = serde_json::from_value(stored["zen_layout"].clone())
        .expect("a layout built by holding S parses back");
    assert_eq!(read_back, zen.rice);
}

#[test]
fn a_chat_tile_keeps_its_room_through_the_stored_json_and_old_layouts_still_read() {
    let mut zen = ZenState::new(RiceLayout::default());
    let chat = zen.first_tile_of(TileKind::Chat).expect("the default has a chat");
    let room = Uuid::now_v7();
    // Only a focused chat tile takes a room.
    assert!(!zen.bind_focused_chat_room(Some(room)), "bonsai has no room");
    zen.focus = chat;
    assert!(zen.bind_focused_chat_room(Some(room)));
    assert_eq!(zen.focused_chat_room(), Some(Some(room)));
    assert_eq!(zen.chat_tiles(), vec![(chat, Some(room))]);

    let read_back = RiceLayout::from_json(Some(&zen.rice.to_json()));
    assert_eq!(read_back, zen.rice, "the binding is part of the layout");

    // A layout saved before rooms were per tile has bare leaves: it parses,
    // and its chat shows the current room.
    let old = serde_json::json!({
        "root": { "node": "leaf", "kind": "chat" },
        "look": { "border": "rounded", "gap": 0, "titles": true }
    });
    let old = RiceLayout::from_json(Some(&old));
    assert_eq!(old.root.leaf_rooms(), vec![None]);

    // Leaving chat forgets the room, so coming back lands on the current one.
    zen.cycle_focused_kind(true);
    zen.cycle_focused_kind(false);
    assert_eq!(zen.focused_chat_room(), Some(None));
}

#[test]
fn the_page_holds_ten_chats_and_the_first_opening_lands_on_the_first_one() {
    let mut zen = ZenState::new(RiceLayout::default());
    // Split the bonsai again and again; each new tile is set to Pet, whose
    // next kind is Chat, and cycled forward: Chat until the cap, then the
    // cycle skips to Music.
    zen.focus = 0;
    for _ in 0..MAX_CHAT_TILES + 2 {
        assert!(zen.split_focused(true));
        zen.rice.root.set_kind(zen.focus, TileKind::Pet);
        zen.cycle_focused_kind(true);
    }
    assert_eq!(zen.chat_tile_count(), MAX_CHAT_TILES, "chat is skipped past the cap");
    assert_eq!(zen.focused_kind(), Some(TileKind::Music));
    // A tile that already is a chat can leave and come back.
    zen.focus = zen.first_tile_of(TileKind::Chat).expect("chats");
    zen.cycle_focused_kind(false);
    assert_ne!(zen.focused_kind(), Some(TileKind::Chat));
    zen.cycle_focused_kind(true);
    assert_eq!(zen.focused_kind(), Some(TileKind::Chat));
    assert_eq!(zen.chat_tile_count(), MAX_CHAT_TILES);

    // The first opening focuses the first chat tile; later ones keep focus.
    let mut fresh = ZenState::new(RiceLayout::default());
    fresh.note_opened();
    assert_eq!(fresh.focused_kind(), Some(TileKind::Chat));
    assert_eq!(fresh.active_chat_index(), Some(0));
    fresh.focus = 0;
    fresh.note_opened();
    assert_eq!(fresh.focused_kind(), Some(TileKind::Bonsai));
    // With the bonsai focused the first chat tile is still the active one.
    assert_eq!(fresh.active_chat_index(), Some(0));
}
