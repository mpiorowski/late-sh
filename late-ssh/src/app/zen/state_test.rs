use ratatui::layout::Rect;
use uuid::Uuid;

use super::ChatRoomBinding;
use super::*;
use crate::app::zen::layout::{Gaps, tile_rects};

fn widths(root: &Node, area: Rect, gap: u16) -> Vec<u16> {
    tile_rects(root, area, Gaps::uniform(gap))
        .into_iter()
        .map(|(_, rect)| rect.width)
        .collect()
}

/// A stored layout as a page reads it, the stock `zen` layout its fallback.
fn from_json(value: Option<&serde_json::Value>) -> RiceLayout {
    RiceLayout::from_json_or(value, RiceLayout::default())
}

/// A page showing `rice`, with the focus moved to its first tile: the
/// edits below act on a known tile, whatever the layout opens on.
fn focused_on_first_tile(rice: RiceLayout) -> ZenState {
    let mut page = ZenState::new(rice, Some(RiceLayout::default()));
    page.focus = 0;
    page
}

fn heights(root: &Node, area: Rect, gap: u16) -> Vec<u16> {
    tile_rects(root, area, Gaps::uniform(gap))
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
            assert!(root.resize_leaf(0, Dir::Row, 1, area, Gaps::uniform(gap)));
            let grown = widths(&root, area, gap);
            assert_eq!(
                grown[0],
                before[0] + 1,
                "width {width} gap {gap}: the first tile grows one column"
            );
            assert_eq!(grown[1], before[1] - 1, "and the second gives it up");

            // Shrinking from the second tile's side lands back where it was.
            assert!(root.resize_leaf(1, Dir::Row, 1, area, Gaps::uniform(gap)));
            assert_eq!(widths(&root, area, gap), before);
        }
    }
}

#[test]
fn a_deep_column_split_moves_one_row_and_a_row_only_tree_has_no_height() {
    let area = Rect::new(0, 0, 160, 44);
    let mut root = RiceLayout::default().root;
    // Leaves run bonsai, aquarium, chat, clock, pet, lobby, live, music.
    // Live sits in a column inside a column inside the rail; only the
    // split directly above it moves, so the tiles above it and the left
    // column stay put, and music alone gives up the row.
    let before = heights(&root, area, 1);
    assert!(root.resize_leaf(6, Dir::Column, 1, area, Gaps::uniform(1)));
    let after = heights(&root, area, 1);
    assert_eq!(after[6], before[6] + 1, "live gains one row");
    for leaf in 0..6 {
        assert_eq!(after[leaf], before[leaf], "leaf {leaf} is untouched");
    }
    assert_eq!(after[7] + 1, before[7], "music gives up the row");

    let mut row_only = Node::split(
        Dir::Row,
        500,
        Node::leaf(TileKind::Bonsai),
        Node::leaf(TileKind::Chat),
    );
    assert!(
        !row_only.resize_leaf(0, Dir::Column, 1, area, Gaps::uniform(1)),
        "no column split above the tile: nothing to trade height with"
    );
}

#[test]
fn a_tile_shrinks_to_a_border_and_one_row_and_never_past_it() {
    let area = Rect::new(0, 0, 100, 40);
    let mut root = Node::split(
        Dir::Column,
        500,
        Node::leaf(TileKind::Clock),
        Node::leaf(TileKind::Chat),
    );
    for _ in 0..40 {
        assert!(root.resize_leaf(0, Dir::Column, -1, area, Gaps::uniform(0)));
    }
    assert_eq!(
        heights(&root, area, 0),
        vec![MIN_TILE_CELLS, 40 - MIN_TILE_CELLS],
        "the clock keeps its border and one row"
    );
    for _ in 0..80 {
        assert!(root.resize_leaf(0, Dir::Column, 1, area, Gaps::uniform(0)));
    }
    assert_eq!(
        heights(&root, area, 0),
        vec![40 - MIN_TILE_CELLS, MIN_TILE_CELLS],
        "and the chat below keeps the same when the clock grows"
    );

    // A split too small for two floors splits evenly instead of panicking.
    let tiny = Rect::new(0, 0, 100, 4);
    assert!(root.resize_leaf(0, Dir::Column, 1, tiny, Gaps::uniform(0)));
    assert_eq!(heights(&root, tiny, 0), vec![2, 2]);
}

#[test]
fn holding_split_stops_at_the_tile_cap_and_the_layout_still_round_trips() {
    // Every `S` splits the focused tile and moves focus into the new one, so
    // a held key nests the tree one level deeper per press. The stored JSON
    // is read back through serde_json, which refuses to parse past 128
    // nested levels; a layout that deep would lock the account out at
    // login. The cap keeps the tree far below that and the JSON readable.
    let mut zen = focused_on_first_tile(RiceLayout::default());
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
    // Nested inside the key's settings the way Home's layouts are stored,
    // and read back from text, it must still parse.
    let stored = serde_json::json!({
        "home_layouts": { "current": 0, "items": [{ "name": "held", "layout": zen.rice.to_json() }] }
    })
    .to_string();
    let stored: serde_json::Value =
        serde_json::from_str(&stored).expect("a layout built by holding S parses back");
    let read_back: RiceLayout =
        serde_json::from_value(stored["home_layouts"]["items"][0]["layout"].clone())
            .expect("and reads as a layout");
    assert_eq!(read_back, zen.rice);
}

#[test]
fn a_chat_tile_keeps_its_room_through_the_stored_json_and_old_layouts_still_read() {
    let mut zen = focused_on_first_tile(RiceLayout::default());
    let chat = zen
        .first_tile_of(TileKind::Chat)
        .expect("the default has a chat");
    let room = Uuid::now_v7();
    // The active chat tile takes the room: the focused one, else the
    // first chat on the page while the focus is on the bonsai.
    assert_ne!(zen.focus, chat);
    assert!(zen.bind_active_chat_room(ChatRoomBinding::Room(room)));
    assert_eq!(zen.active_chat_room(), Some(ChatRoomBinding::Room(room)));
    assert_eq!(zen.chat_tiles(), vec![(chat, ChatRoomBinding::Room(room))]);
    zen.focus = chat;

    let read_back = from_json(Some(&zen.rice.to_json()));
    assert_eq!(read_back, zen.rice, "the binding is part of the layout");

    // A layout saved before rooms were per tile has bare leaves: it parses,
    // and its chat shows the current room.
    let old = serde_json::json!({
        "root": { "node": "leaf", "kind": "chat" },
        "look": { "border": "rounded", "gap": 0, "titles": true }
    });
    let old = from_json(Some(&old));
    assert_eq!(old.root.leaf_rooms(), vec![ChatRoomBinding::Current]);

    // A stock layout holds News and binds a permanent room by slug, and
    // both ride the same `room` key a room id does.
    let street = RiceLayout::street();
    let bound: Vec<ChatRoomBinding> = street
        .root
        .leaf_rooms()
        .into_iter()
        .filter(|room| !room.is_current())
        .collect();
    assert_eq!(
        bound,
        vec![
            ChatRoomBinding::News,
            ChatRoomBinding::Permanent("announcements".to_string()),
        ]
    );
    assert_eq!(from_json(Some(&street.to_json())), street);

    // A layout from before Pulse names the presence tile, which is a tile
    // again (the online count, the clock, the friends): it reads as such,
    // not as an unreadable row that would reset the whole page.
    let retired = serde_json::json!({
        "root": { "node": "leaf", "kind": "presence" },
        "look": { "border": "rounded", "gap": 0, "titles": true }
    });
    assert_eq!(
        from_json(Some(&retired)).root.leaf_kinds(),
        vec![TileKind::Presence]
    );

    // Leaving chat forgets the room, so coming back lands on the current one.
    zen.open_kind_picker();
    zen.move_kind_picker(1);
    assert_eq!(zen.pick_kind(), KindPick::Changed);
    assert_eq!(zen.focused_kind(), Some(TileKind::Clock));
    zen.open_kind_picker();
    zen.move_kind_picker(-1);
    assert_eq!(zen.pick_kind(), KindPick::Changed);
    assert_eq!(zen.focused_kind(), Some(TileKind::Chat));
    assert_eq!(zen.active_chat_room(), Some(ChatRoomBinding::Current));
}

#[test]
fn the_page_holds_ten_chats_and_opens_on_the_first_one() {
    let mut zen = focused_on_first_tile(RiceLayout::default());
    // Split the bonsai again and again and pick Chat for each new tile in
    // the picker (the tile is set to Bonsai first, the row before Chat, so
    // one move down lands on it): Chat until the cap, then the row is
    // refused, the picker stays up, and the next row down is Clock.
    zen.focus = 0;
    for _ in 0..MAX_CHAT_TILES + 2 {
        assert!(zen.split_focused(true));
        zen.rice.root.set_kind(zen.focus, TileKind::Bonsai);
        zen.open_kind_picker();
        assert_eq!(zen.kind_picker_selection(), Some(TileKind::Bonsai));
        zen.move_kind_picker(1);
        assert_eq!(zen.kind_picker_selection(), Some(TileKind::Chat));
        match zen.pick_kind() {
            KindPick::Changed => assert!(zen.kind_picker.is_none(), "a pick closes the picker"),
            KindPick::Unchanged => panic!("a new tile picked as chat must change"),
            KindPick::ChatFull => {
                assert!(
                    zen.kind_picker.is_some(),
                    "a refused row keeps the picker up"
                );
                assert!(!zen.kind_allowed(TileKind::Chat));
                zen.move_kind_picker(1);
                assert_eq!(zen.pick_kind(), KindPick::Changed);
            }
        }
    }
    assert_eq!(
        zen.chat_tile_count(),
        MAX_CHAT_TILES,
        "chat is refused past the cap"
    );
    assert_eq!(zen.focused_kind(), Some(TileKind::Clock));
    // A tile that already is a chat can leave and come back: it counts
    // itself out of the cap.
    zen.focus = zen.first_tile_of(TileKind::Chat).expect("chats");
    assert!(zen.kind_allowed(TileKind::Chat));
    zen.open_kind_picker();
    zen.move_kind_picker(-1);
    assert_eq!(zen.pick_kind(), KindPick::Changed);
    assert_ne!(zen.focused_kind(), Some(TileKind::Chat));
    zen.open_kind_picker();
    zen.move_kind_picker(1);
    assert_eq!(zen.pick_kind(), KindPick::Changed);
    assert_eq!(zen.focused_kind(), Some(TileKind::Chat));
    assert_eq!(zen.chat_tile_count(), MAX_CHAT_TILES);
    // Picking the tile's own kind closes the picker and changes nothing.
    zen.open_kind_picker();
    assert_eq!(zen.pick_kind(), KindPick::Unchanged);
    assert!(zen.kind_picker.is_none());
    // The list wraps at both ends.
    zen.open_kind_picker();
    zen.move_kind_picker(-(TileKind::ALL.len() as isize));
    assert_eq!(zen.kind_picker_selection(), Some(TileKind::Chat));
    zen.close_kind_picker();

    // A page opens on its first chat tile, outside arrange mode.
    let mut fresh = ZenState::new(RiceLayout::default(), Some(RiceLayout::default()));
    assert_eq!(fresh.focused_kind(), Some(TileKind::Chat));
    assert_eq!(fresh.active_chat_index(), Some(0));
    assert!(!fresh.arrange);
    fresh.focus = 0;
    assert_eq!(fresh.focused_kind(), Some(TileKind::Bonsai));
    // With the bonsai focused the first chat tile is still the active one.
    assert_eq!(fresh.active_chat_index(), Some(0));
    // A reset lands on the chat tile too.
    fresh.reset();
    assert_eq!(fresh.focused_kind(), Some(TileKind::Chat));
}

#[test]
fn the_equalizer_shows_through_a_music_or_visualizer_tile() {
    // Leaves run bonsai, aquarium, chat, clock, pet, lobby, live, music.
    let zen = focused_on_first_tile(RiceLayout::default());
    assert!(zen.shows_equalizer(), "the default page has a music tile");

    let visualizer_only = focused_on_first_tile(RiceLayout {
        root: Node::leaf(TileKind::Visualizer),
        look: Look::default(),
    });
    assert!(visualizer_only.shows_equalizer());

    let no_eq = focused_on_first_tile(RiceLayout {
        root: Node::split(
            Dir::Row,
            500,
            Node::leaf(TileKind::Clock),
            Node::leaf(TileKind::Aquarium),
        ),
        look: Look::default(),
    });
    assert!(!no_eq.shows_equalizer());
    assert!(
        !no_eq.paints_on_half_edge(),
        "a clock and a reef hold no half-edge motion"
    );

    // The bonsai sways on the half edge without an equalizer.
    let bonsai_only = focused_on_first_tile(RiceLayout {
        root: Node::leaf(TileKind::Bonsai),
        look: Look::default(),
    });
    assert!(!bonsai_only.shows_equalizer());
    assert!(bonsai_only.paints_on_half_edge());
}

/// An ascii tile plays the screensaver's piece, the slow earthrise,
/// until `[` `]` pick another; the piece is stored with the layout, rides
/// along when the tile is split, and is forgotten when the tile becomes
/// something else, as a chat tile's room is.
#[test]
fn an_ascii_tile_keeps_its_piece_until_it_changes_kind() {
    use late_core::models::user::{AsciiPiece, AsciiSelection, Scene, SceneStyle, Screensaver};
    use rand::{SeedableRng, rngs::StdRng};
    use std::time::Instant;
    let now = Instant::now();
    let mut rng = StdRng::seed_from_u64(3);
    let earthrise = AsciiPiece {
        scene: Scene::Earthrise,
        style: SceneStyle::Dots,
    };
    let dawn = AsciiPiece {
        scene: Scene::AlpineDawn,
        style: SceneStyle::Pixels,
    };
    let mut zen = focused_on_first_tile(RiceLayout {
        root: Node::leaf(TileKind::Ascii),
        look: Look::default(),
    });
    assert_eq!(zen.focused_piece(), Some(earthrise));
    assert_eq!(
        Screensaver::DEFAULT,
        Screensaver::On(AsciiSelection::Piece(earthrise)),
        "the tile and the screensaver default to one piece"
    );
    assert!(zen.cycle_focused_piece(true, now, &mut rng));
    assert_eq!(
        zen.focused_piece(),
        Some(AsciiPiece {
            scene: Scene::Earthrise,
            style: SceneStyle::Pixels
        })
    );
    // Back past earthrise and the random modes, off the front of the list
    // onto its end.
    for _ in 0..4 {
        assert!(zen.cycle_focused_piece(false, now, &mut rng));
    }
    assert_eq!(zen.focused_piece(), Some(dawn), "it wraps");

    let stored = serde_json::to_string(&zen.rice).expect("serialize");
    assert!(
        stored.contains("\"piece\":\"alpine_dawn_pixels\""),
        "stored by key: {stored}"
    );
    let restored: RiceLayout = serde_json::from_str(&stored).expect("deserialize");
    assert_eq!(
        restored.root.selection_at(0),
        Some(AsciiSelection::Piece(dawn))
    );

    let aurora = AsciiPiece {
        scene: Scene::AuroraFjord,
        style: SceneStyle::Dots,
    };
    assert!(zen.set_focused_selection(AsciiSelection::Piece(aurora), now, &mut rng));
    assert_eq!(zen.focused_piece(), Some(aurora));
    assert!(zen.set_focused_selection(AsciiSelection::Piece(dawn), now, &mut rng));

    assert!(zen.split_focused(true));
    assert_eq!(
        zen.rice.root.selection_at(0),
        Some(AsciiSelection::Piece(dawn))
    );
    assert_eq!(zen.rice.root.selection_at(1), None, "the new tile is blank");
    assert!(
        !zen.cycle_focused_piece(true, now, &mut rng),
        "`[ ]` on a blank tile is nothing"
    );

    assert!(zen.rice.root.set_kind(0, TileKind::Clock));
    assert!(zen.rice.root.set_kind(0, TileKind::Ascii));
    assert_eq!(
        zen.rice.root.selection_at(0),
        Some(AsciiSelection::Piece(earthrise)),
        "coming back plays the default"
    );
}

/// Layouts stored before the ascii tile existed carry no `piece` key, and
/// read back unchanged.
#[test]
fn a_layout_without_pieces_reads_back_unchanged() {
    let stored = serde_json::to_string(&RiceLayout::default()).expect("serialize");
    assert!(!stored.contains("piece"));
    let restored: RiceLayout = serde_json::from_str(&stored).expect("deserialize");
    assert_eq!(restored, RiceLayout::default());
}

/// A piece key this build does not know (one dropped, like the donut, or a
/// deploy rolled back past it) costs that tile its pick, not the user the
/// whole layout: the tile plays the default and every other leaf reads as
/// stored.
#[test]
fn a_layout_with_an_unknown_piece_keeps_everything_but_the_pick() {
    use late_core::models::user::{AsciiPiece, AsciiSelection, Scene, SceneStyle};
    let room = Uuid::from_u128(7);
    let stored = serde_json::json!({
        "root": {
            "node": "split",
            "dir": "row",
            "share": 500,
            "first": { "node": "leaf", "kind": "ascii", "piece": "donut" },
            "second": {
                "node": "split",
                "dir": "column",
                "share": 400,
                "first": { "node": "leaf", "kind": "chat", "room": room },
                "second": { "node": "leaf", "kind": "ascii", "piece": "alpine_dawn_pixels" }
            }
        },
        "look": Look::default()
    });
    let restored = from_json(Some(&stored));
    assert_ne!(restored, RiceLayout::default(), "the layout survives");
    assert_eq!(restored.root.selection_at(0), Some(AsciiSelection::DEFAULT));
    assert_eq!(restored.root.kind_at(1), Some(TileKind::Chat));
    assert_eq!(restored.root.leaf_rooms()[1], ChatRoomBinding::Room(room));
    assert_eq!(
        restored.root.selection_at(2),
        Some(AsciiSelection::Piece(AsciiPiece {
            scene: Scene::AlpineDawn,
            style: SceneStyle::Pixels
        }))
    );
}

fn random_tiles() -> ZenState {
    use late_core::models::user::AsciiSelection;
    let mut root = Node::split(
        Dir::Row,
        500,
        Node::leaf(TileKind::Ascii),
        Node::leaf(TileKind::Ascii),
    );
    root.set_selection(0, AsciiSelection::Random);
    root.set_selection(1, AsciiSelection::Randomer);
    focused_on_first_tile(RiceLayout {
        root,
        look: Look::default(),
    })
}

#[test]
fn random_tiles_keep_runtime_separate_from_the_saved_layout() {
    use late_core::models::user::AsciiSelection;
    use rand::{SeedableRng, rngs::StdRng};
    use std::time::{Duration, Instant};
    let mut zen = random_tiles();
    let saved = zen.rice.to_json();
    assert_eq!(saved["root"]["first"]["piece"], "random");
    assert_eq!(saved["root"]["second"]["piece"], "randomer");
    assert_eq!(from_json(Some(&saved)), zen.rice);
    let now = Instant::now();
    let mut rng = StdRng::seed_from_u64(17);
    zen.enter(now, &mut rng);
    let initial = zen.playback.clone();
    assert!(zen.piece_label(0).unwrap().starts_with("random · "));
    assert!(zen.piece_label(1).unwrap().starts_with("randomer · "));
    zen.cycle_border();
    zen.cycle_gap();
    zen.flip_focused();
    zen.resize_focused(Dir::Column, 1, Rect::new(0, 0, 100, 40));
    // The Random tile holds; the Randomer tile rotates, and since every
    // tile is on screen that rotation is a frame worth paying.
    assert!(
        zen.sync_playback(now + Duration::from_secs(300), &mut rng),
        "a rotating piece on show dirties the frame"
    );
    assert_eq!(zen.playback[0], initial[0]);
    assert_ne!(
        zen.piece_at(1).unwrap().scene,
        initial[1].unwrap().piece().scene
    );
    assert_eq!(
        zen.pieces(),
        vec![zen.piece_at(0).unwrap(), zen.piece_at(1).unwrap()]
    );
    let rotated = zen.playback.clone();
    zen.focus = 1;
    assert!(!zen.set_focused_selection(
        AsciiSelection::Randomer,
        now + Duration::from_secs(300),
        &mut rng
    ));
    assert_eq!(zen.playback, rotated, "reselecting preserves playback");
    assert_eq!(
        zen.rice.root.selection_at(1),
        Some(AsciiSelection::Randomer)
    );
    // Runtime cannot leak into JSON, including when the layout is edited.
    let stored = zen.rice.to_json();
    assert_eq!(
        stored["root"]["first"]["piece"],
        saved["root"]["first"]["piece"]
    );
    assert_eq!(
        stored["root"]["second"]["piece"],
        saved["root"]["second"]["piece"]
    );
    assert_eq!(stored["root"]["second"].as_object().unwrap().len(), 3);
}

#[test]
fn random_tile_instances_follow_surviving_leaves_through_layout_edits() {
    use rand::{SeedableRng, rngs::StdRng};
    use std::time::Instant;
    let mut zen = random_tiles();
    zen.enter(Instant::now(), &mut StdRng::seed_from_u64(21));
    let original = zen.playback.clone();
    assert!(zen.split_focused(true));
    assert_eq!(zen.playback[0], original[0]);
    assert_eq!(zen.playback[2], original[1]);
    assert_eq!(zen.playback[1], None, "the new tile is blank");
    assert!(zen.close_focused());
    assert_eq!(zen.playback, original);
    zen.focus = 0;
    assert!(zen.close_focused());
    assert_eq!(zen.playback, vec![original[1]]);
    zen.open_kind_picker();
    zen.kind_picker = TileKind::ALL
        .iter()
        .position(|kind| *kind == TileKind::Clock);
    assert_eq!(zen.pick_kind(), KindPick::Changed);
    assert_eq!(zen.playback, vec![None]);
    zen.open_kind_picker();
    zen.kind_picker = TileKind::ALL
        .iter()
        .position(|kind| *kind == TileKind::Ascii);
    assert_eq!(zen.pick_kind(), KindPick::Changed);
    assert_eq!(
        zen.focused_piece(),
        Some(late_core::models::user::AsciiPiece::DEFAULT)
    );
    zen.reset();
    assert!(zen.playback.iter().all(Option::is_none));
}

#[test]
fn leaving_zen_ends_random_instances_and_returning_resolves_them_again() {
    use rand::{RngCore, SeedableRng, rngs::StdRng};
    use std::time::{Duration, Instant};
    let mut zen = random_tiles();
    let now = Instant::now();
    let mut rng = StdRng::seed_from_u64(22);
    zen.enter(now, &mut rng);
    let saved = zen.rice.to_json();
    zen.leave();
    assert!(zen.playback.iter().all(Option::is_none));
    let later = now + Duration::from_secs(3600);
    let mut idle_rng = rng.clone();
    assert!(!zen.sync_playback(later, &mut rng));
    assert_eq!(rng.next_u64(), idle_rng.next_u64());
    let mut idle_rng = rng.clone();
    zen.enter(later, &mut rng);
    assert_ne!(
        rng.next_u64(),
        idle_rng.next_u64(),
        "each tile draws afresh"
    );
    assert!(zen.playback.iter().all(Option::is_some));
    assert!(!zen.sync_playback(later + Duration::from_secs(299), &mut rng));
    assert!(zen.sync_playback(later + Duration::from_secs(300), &mut rng));
    assert_eq!(zen.rice.to_json(), saved);
}

#[test]
fn a_fixed_split_keeps_its_cells_at_every_width_and_a_share_still_reads_as_a_number() {
    // Home's rail: 23 cells beside a chat that takes the rest, at every
    // page width; a resize moves its edge by exactly one cell and the
    // edge stays put when the page grows.
    let mut root = Node::split_first(
        Dir::Row,
        23,
        Node::leaf(TileKind::Rooms),
        Node::leaf(TileKind::Chat),
    );
    for width in [60u16, 100, 200] {
        let area = Rect::new(0, 0, width, 30);
        assert_eq!(widths(&root, area, 1), vec![23, width - 24], "{width}");
    }
    let area = Rect::new(0, 0, 100, 30);
    assert!(root.resize_leaf(0, Dir::Row, 1, area, Gaps::uniform(1)));
    assert_eq!(widths(&root, area, 1), vec![24, 75]);
    assert_eq!(
        widths(&root, Rect::new(0, 0, 200, 30), 1),
        vec![24, 175],
        "fixed from the first child's side after the resize too"
    );

    // The sidebar is fixed from the other end: the chat takes the rest.
    let mut root = Node::split_second(
        Dir::Row,
        23,
        Node::leaf(TileKind::Chat),
        Node::leaf(TileKind::Music),
    );
    assert_eq!(widths(&root, area, 1), vec![76, 23]);
    assert!(root.resize_leaf(1, Dir::Row, 1, area, Gaps::uniform(1)));
    assert_eq!(widths(&root, area, 1), vec![75, 24]);
    assert_eq!(widths(&root, Rect::new(0, 0, 200, 30), 1), vec![175, 24]);

    // A per-mille share is still written as a bare number, so a layout
    // stored before fixed splits reads unchanged, and a fixed split reads
    // back as itself.
    let look = RiceLayout::default().look;
    let mille = RiceLayout {
        root: Node::split(
            Dir::Row,
            640,
            Node::leaf(TileKind::Chat),
            Node::leaf(TileKind::Clock),
        ),
        look: look.clone(),
    };
    let json = mille.to_json();
    assert_eq!(json["root"]["share"], serde_json::json!(640));
    assert_eq!(from_json(Some(&json)), mille);
    let fixed = RiceLayout { root, look };
    let json = fixed.to_json();
    assert_eq!(json["root"]["share"], serde_json::json!({ "second": 24 }));
    assert_eq!(from_json(Some(&json)), fixed);
}

#[test]
fn a_guest_lives_on_one_tile_and_toggling_moves_it_or_takes_it_off() {
    let mut root = Node::split(
        Dir::Column,
        500,
        Node::hosting(TileKind::Chat, &[Guest::Now]),
        Node::hosting(TileKind::Aquarium, &[Guest::Attention]),
    );
    assert_eq!(root.host_of(Guest::Now), Some(0));
    // Toggled on the reef, the strip moves there, beside the reef's
    // attention: one host per guest, any of the guests per tile, kept in
    // `Guest::ALL` order whichever came first.
    assert!(root.toggle_guest(1, Guest::Now));
    assert_eq!(root.host_of(Guest::Now), Some(1));
    assert_eq!(
        root.leaf_guests(),
        vec![vec![], vec![Guest::Now, Guest::Attention]]
    );
    // Toggled on its host, it goes; the other stays.
    assert!(root.toggle_guest(1, Guest::Now));
    assert_eq!(root.host_of(Guest::Now), None);
    assert_eq!(root.leaf_guests(), vec![vec![], vec![Guest::Attention]]);

    // Guests are part of the stored layout; a leaf without any writes
    // none, so layouts stored before guests stay as they were.
    let layout = RiceLayout {
        root,
        look: RiceLayout::default().look,
    };
    let json = layout.to_json();
    assert_eq!(from_json(Some(&json)), layout);
    assert!(
        json["root"]["first"].get("guests").is_none(),
        "a tile without guests writes none: {json}"
    );
}

#[test]
fn a_layout_saved_with_a_guest_list_keeps_the_guests_still_known() {
    // Layouts saved with a `guests` list, `live` and `banner` among them,
    // read as the guests this build knows.
    let stored = serde_json::json!({
        "root": {
            "node": "leaf",
            "kind": "aquarium",
            "guests": ["live", "banner", "now", "lobby"],
        },
        "look": RiceLayout::default().look,
    });
    let layout = from_json(Some(&stored));
    assert_eq!(
        layout.root.leaf_guests(),
        vec![vec![Guest::Now, Guest::Attention]]
    );
    // A tile that hosted the lobby, the guest attention grew out of, under
    // the one `guest` key of the builds that hosted one, hosts attention.
    let lobby_host = serde_json::json!({
        "root": { "node": "leaf", "kind": "chat", "guest": "lobby" },
        "look": RiceLayout::default().look,
    });
    assert_eq!(
        from_json(Some(&lobby_host)).root.leaf_guests(),
        vec![vec![Guest::Attention]]
    );
    // The inbox kind is gone: a layout that had one keeps its shape, the
    // slot reading as blank space.
    let had_inbox = serde_json::json!({
        "root": { "node": "leaf", "kind": "inbox" },
        "look": RiceLayout::default().look,
    });
    assert_eq!(
        from_json(Some(&had_inbox)).root.leaf_kinds(),
        vec![TileKind::Blank]
    );
    let only_gone = serde_json::json!({
        "root": { "node": "leaf", "kind": "chat", "guests": ["banner"] },
        "look": RiceLayout::default().look,
    });
    assert_eq!(
        from_json(Some(&only_gone)).root.leaf_guests(),
        vec![Vec::<Guest>::new()]
    );
}

#[test]
fn the_stock_home_is_the_rail_the_reef_over_the_chat_and_the_sidebar_column() {
    // The stock is fixed and the same for every account: the sidebar's
    // panels with the pet over the bonsai, and an unowned reef or pet
    // draws the shop's note in its tile.
    let home = RiceLayout::home();
    assert_eq!(
        home.root.leaf_kinds(),
        vec![
            TileKind::Rooms,
            TileKind::Aquarium,
            TileKind::Chat,
            TileKind::Presence,
            TileKind::Visualizer,
            TileKind::Music,
            TileKind::Lobby,
            TileKind::Live,
            TileKind::Pet,
            TileKind::Bonsai,
        ]
    );
    assert_eq!(
        home.root.leaf_guests()[1],
        vec![Guest::Now],
        "the reef hosts the strip, so the chat never loses a row to it"
    );
    assert_eq!(
        home.root.leaf_guests()[6],
        vec![Guest::Attention],
        "the lobby hosts attention: what waits on you takes the tables' tile"
    );
    assert_eq!(
        home.root.leaf_guests()[2],
        Vec::<Guest>::new(),
        "the board's day is the banner over the tree, not a guest"
    );
    let titles: Vec<TileTitle> = (0..home.root.leaf_count())
        .map(|ordinal| home.root.title_at(ordinal))
        .collect();
    assert_eq!(
        titles,
        vec![
            TileTitle::Hidden,
            TileTitle::Hidden,
            TileTitle::Named,
            TileTitle::Hidden,
            TileTitle::Named,
            TileTitle::Named,
            TileTitle::Named,
            TileTitle::Named,
            TileTitle::Named,
            TileTitle::Named,
        ],
        "only the rail, the reef and the presence rows take their title off"
    );

    let zen = ZenState::new(home.clone(), Some(home));
    assert_eq!(
        zen.focused_kind(),
        Some(TileKind::Chat),
        "Home opens on its chat"
    );
    assert_eq!(
        zen.active_chat_room(),
        Some(ChatRoomBinding::Current),
        "the stock chat is bound to no room: it shows the rail's"
    );
    assert!(!zen.arrange, "Home is furniture until Ctrl+E");
    assert!(!zen.active_chat_hosts(Guest::Now));
    assert!(zen.hosts(Guest::Now));
    assert!(!zen.kind_allowed(TileKind::Rooms), "one rail per page");

    // The stock zen layout has no rail, so any tile of it may become one.
    let zen_layout = ZenState::new(RiceLayout::default(), Some(RiceLayout::default()));
    assert!(zen_layout.kind_allowed(TileKind::Rooms));
    assert!(zen_layout.kind_allowed(TileKind::Presence));
}

#[test]
fn a_tile_owns_its_title_mode_on_home_and_the_mode_rides_the_layout() {
    let home = RiceLayout::home();
    let mut zen = ZenState::new(home.clone(), Some(home));
    let chat = zen.first_tile_of(TileKind::Chat).expect("a chat");
    let music = zen.first_tile_of(TileKind::Music).expect("a music tile");
    assert_eq!(zen.rice.root.title_at(chat), TileTitle::Named);

    // `t` walks the focused tile through the three and back, and stops
    // nowhere else.
    zen.focus = chat;
    assert!(zen.cycle_focused_title());
    assert_eq!(zen.rice.root.title_at(chat), TileTitle::Rule);
    assert!(zen.cycle_focused_title());
    assert_eq!(zen.rice.root.title_at(chat), TileTitle::Hidden);
    assert!(zen.cycle_focused_title());
    assert_eq!(zen.rice.root.title_at(chat), TileTitle::Named);
    assert_eq!(zen.rice.root.title_at(music), TileTitle::Named);

    // The layout stores each one and a reload keeps it; a new kind starts
    // over with its name.
    zen.focus = music;
    assert!(zen.cycle_focused_title());
    zen.focus = chat;
    assert!(zen.cycle_focused_title());
    assert!(zen.cycle_focused_title());
    let saved = zen.rice.to_json();
    assert_eq!(from_json(Some(&saved)), zen.rice);
    assert!(zen.rice.root.set_kind(music, TileKind::Clock));
    assert_eq!(zen.rice.root.title_at(music), TileTitle::Named);
}

/// A layout saved when a title was only on or off reads as the same page: on
/// is the name, off is no row, and a mode this build does not know keeps the
/// layout and shows the name.
#[test]
fn a_stored_title_reads_from_its_older_on_or_off() {
    let leaf = |title: serde_json::Value| -> TileTitle {
        let stored = serde_json::json!({
            "root": { "node": "leaf", "kind": "clock", "title": title },
            "look": { "border": "none", "gap": 0, "titles": true }
        });
        let layout = RiceLayout::from_json_or(
            Some(&stored),
            RiceLayout {
                root: Node::leaf(TileKind::Bonsai),
                look: Look {
                    border: BorderKind::None,
                    gap: 0,
                    titles: true,
                },
            },
        );
        assert_eq!(layout.root.leaf_kinds(), vec![TileKind::Clock]);
        layout.root.title_at(0)
    };
    assert_eq!(leaf(serde_json::json!(true)), TileTitle::Named);
    assert_eq!(leaf(serde_json::json!(false)), TileTitle::Hidden);
    assert_eq!(leaf(serde_json::json!("rule")), TileTitle::Rule);
    assert_eq!(leaf(serde_json::json!("hidden")), TileTitle::Hidden);
    assert_eq!(leaf(serde_json::json!("sideways")), TileTitle::Named);
}

/// Under the reef's minimum the tile shows a size warning and the fish stop,
/// so the stock reef clears it even in arrange mode, where the title takes a
/// row. A taller creature raises the minimum and fails here.
#[test]
fn the_stock_reef_fits_the_tallest_creature_with_its_title_up() {
    use crate::app::hub::aquarium::state::{AquariumState, RuntimeMode};

    let aquarium = AquariumState::default_for_area(Rect::new(0, 0, 120, 40)).expect("aquarium");
    let RuntimeMode::Reef(reef) = &aquarium.mode else {
        panic!("the default aquarium is a reef");
    };
    let home = RiceLayout::home();
    let rects = tile_rects(
        &home.root,
        Rect::new(0, 0, 200, 60),
        Gaps::uniform(home.look.gap as u16),
    );
    let (kind, rect) = rects[1];
    assert_eq!(kind, TileKind::Aquarium);
    let rows_under_the_title = rect.height - 1;
    assert!(
        rows_under_the_title >= reef.min_height,
        "{rows_under_the_title} rows under the title, the reef needs {}",
        reef.min_height
    );
}
