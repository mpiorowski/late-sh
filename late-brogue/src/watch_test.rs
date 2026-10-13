use super::*;

/// A watcher's side of the wire, minimal: apply reset and diff frames to a
/// parser the way late-ssh's spectate client does.
fn apply_frames(watcher: &mut vt100::Parser, mut bytes: &[u8]) {
    while !bytes.is_empty() {
        let tag = bytes[0];
        let len = u32::from_be_bytes(bytes[1..5].try_into().unwrap()) as usize;
        let payload = &bytes[5..5 + len];
        bytes = &bytes[5 + len..];
        match tag {
            FRAME_RESET => {
                let cols = u16::from_be_bytes([payload[0], payload[1]]);
                let rows = u16::from_be_bytes([payload[2], payload[3]]);
                *watcher = vt100::Parser::new(rows, cols, 0);
                watcher.process(&payload[4..]);
            }
            FRAME_DIFF => watcher.process(payload),
            other => panic!("unknown frame tag {other}"),
        }
    }
}

/// Everything a watcher renders: size, cells with their attributes, cursor.
fn visible_state(screen: &vt100::Screen) -> (Vec<u8>, (u16, u16), (u16, u16), bool) {
    (
        screen.contents_formatted(),
        screen.size(),
        screen.cursor_position(),
        screen.hide_cursor(),
    )
}

#[test]
fn parse_request_reads_list_and_game() {
    assert_eq!(parse_request("list"), Some(WatchRequest::List));
    assert_eq!(
        parse_request("game:alice_99"),
        Some(WatchRequest::Game("alice_99".to_string()))
    );
}

#[test]
fn parse_request_refuses_names_the_registry_cannot_hold() {
    assert_eq!(parse_request("game:"), None);
    assert_eq!(parse_request("game:../alice"), None);
    assert_eq!(parse_request("game:alice bob"), None);
    assert_eq!(parse_request(&format!("game:{}", "a".repeat(31))), None);
    assert_eq!(parse_request("alice"), None);
    assert_eq!(parse_request(""), None);
}

#[test]
fn encode_list_writes_one_line_per_game_then_end() {
    let block = encode_list(&[
        ListedGame {
            playname: "alice".to_string(),
            started_unix: 100,
            watchers: 2,
            status: "Depth 4".to_string(),
        },
        ListedGame {
            playname: "bob".to_string(),
            started_unix: 200,
            watchers: 0,
            status: String::new(),
        },
    ]);
    assert_eq!(
        String::from_utf8(block).unwrap(),
        "game\talice\t100\t2\tDepth 4\ngame\tbob\t200\t0\t\nend\n"
    );
    assert_eq!(encode_list(&[]), b"end\n");
}

/// A watcher that joins mid-game and then follows diffs ends up with exactly
/// the player's screen, through the terminal state a raw replay would lose:
/// the alternate screen, a scroll region scrolled after the join, colors,
/// a hidden cursor, and a window change.
#[test]
fn watcher_frames_reproduce_the_players_screen() {
    let mut host = vt100::Parser::new(24, 80, 0);
    let mut watcher = vt100::Parser::new(1, 1, 0);
    let mut prev: Option<vt100::Screen> = None;
    let mut step = |host: &mut vt100::Parser, bytes: &[u8]| {
        host.process(bytes);
        let screen = host.screen().clone();
        if let Some(frame) = screen_frame(prev.as_ref(), &screen) {
            apply_frames(&mut watcher, &frame);
        }
        prev = Some(screen);
        assert_eq!(
            visible_state(watcher.screen()),
            visible_state(host.screen())
        );
    };

    // Before the watcher joins: alternate screen, a message scroll region,
    // a colored map line.
    host.process(b"\x1b[?1049h\x1b[2J\x1b[H\x1b[1;31m@\x1b[0m....g....>\x1b[20;24r");
    // Join (reset frame), then lines scrolled inside the region.
    step(&mut host, b"");
    step(&mut host, b"\x1b[24;1Hyou hit the goblin\n");
    step(&mut host, b"you kill the goblin!\n\x1b[?25l");
    // The player resizes: a reset frame at the new size.
    step(&mut host, b"");
    host.screen_mut().set_size(30, 100);
    step(
        &mut host,
        b"\x1b[2J\x1b[1;1H\x1b[33m$\x1b[0m redraw at 100x30",
    );
}

#[test]
fn an_unchanged_screen_sends_no_frame() {
    let mut host = vt100::Parser::new(24, 80, 0);
    host.process(b"hello");
    let first = host.screen().clone();
    assert!(screen_frame(None, &first).is_some());
    assert_eq!(screen_frame(Some(&first), &host.screen().clone()), None);
}

#[test]
fn registry_lists_a_game_until_its_handle_drops() {
    let registry = LiveRegistry::new();
    let handle = registry.register("alice", 80, 24);
    let mut ended = registry.get("alice").unwrap().ended.subscribe();
    assert_eq!(
        registry
            .list()
            .iter()
            .map(|g| g.playname.as_str())
            .collect::<Vec<_>>(),
        vec!["alice"]
    );

    drop(handle);
    assert!(registry.list().is_empty());
    assert!(*ended.borrow_and_update(), "watchers must see the game end");
}

#[test]
fn a_relaunch_survives_the_old_sessions_teardown() {
    let registry = LiveRegistry::new();
    let old = registry.register("alice", 80, 24);
    let new = registry.register("alice", 80, 24);
    drop(old);
    assert_eq!(registry.list().len(), 1, "the newer game stays listed");
    new.feed(b"still here");
    assert!(
        registry
            .get("alice")
            .unwrap()
            .snapshot()
            .contents()
            .contains("still here")
    );
}

#[test]
fn watchers_are_counted_while_they_watch() {
    let registry = LiveRegistry::new();
    let _handle = registry.register("alice", 80, 24);
    let game = registry.get("alice").unwrap();
    let guard = WatcherGuard::new(registry.clone(), game);
    assert_eq!(registry.list()[0].watchers, 1);
    drop(guard);
    assert_eq!(registry.list()[0].watchers, 0);
}

#[test]
fn hud_status_reads_the_depth_off_the_sidebar() {
    let sidebar = "Rogue                \nHealth               \n\
                   Str: 12  Armor: 3    \n  -- Depth: 4 --       ";
    assert_eq!(hud_status(sidebar), Some("Depth 4".to_string()));
    assert_eq!(
        hud_status("  -- Depth: 26 --   "),
        Some("Depth 26".to_string())
    );
}

#[test]
fn hud_status_is_none_while_the_sidebar_is_covered() {
    assert_eq!(hud_status(""), None);
    assert_eq!(hud_status("a) a dagger\nb) 3 rations of food"), None);
    assert_eq!(hud_status("  -- Depth: --"), None);
}

/// The roster keeps the last status it read while a menu covers the sidebar.
#[test]
fn the_roster_keeps_a_status_while_the_sidebar_is_covered() {
    let registry = LiveRegistry::new();
    let handle = registry.register("alice", 100, 34);
    assert_eq!(registry.list()[0].status, "");

    handle.feed(b"\x1b[2J\x1b[34;1H  -- Depth: 1 --");
    assert_eq!(registry.list()[0].status, "Depth 1");

    handle.feed(b"\x1b[2J\x1b[1;1Ha) a dagger");
    assert_eq!(registry.list()[0].status, "Depth 1");

    handle.feed(b"\x1b[2J\x1b[34;1H  -- Depth: 2 --");
    assert_eq!(registry.list()[0].status, "Depth 2");
}

/// brogue's truecolor renderer moves the cursor with HVP, which vt100 drops;
/// the mirror positions it like CUP, also when a read splits the sequence.
#[test]
fn the_mirror_positions_hvp_like_cup() {
    let registry = LiveRegistry::new();
    let handle = registry.register("alice", 100, 34);
    handle.feed(b"\x1b[3;5");
    handle.feed(b"f@");

    let screen = registry.get("alice").unwrap().snapshot();
    assert_eq!(screen.cell(2, 4).unwrap().contents(), "@");
}

/// The mirror holds brogue's announced grid, never the slack of a bigger
/// window around it, and clamps the grid to a window smaller than it.
#[test]
fn the_mirror_holds_the_announced_grid_clamped_to_the_window() {
    let registry = LiveRegistry::new();
    let handle = registry.register("alice", 160, 50);
    let size = || registry.get("alice").unwrap().snapshot().size();
    assert_eq!(size(), (50, 160), "the window until brogue announces");

    handle.feed(b"\x1b[8;34;100t");
    assert_eq!(size(), (34, 100));

    handle.resize(200, 60);
    assert_eq!(size(), (34, 100), "a bigger window keeps the grid");

    handle.resize(90, 30);
    assert_eq!(size(), (30, 90), "a smaller window clips it");
}
