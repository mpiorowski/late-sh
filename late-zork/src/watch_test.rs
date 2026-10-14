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
    assert_eq!(parse_request(&format!("game:{}", "a".repeat(21))), None);
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
            status: "Zork I \u{b7} West of House".to_string(),
        },
        ListedGame {
            playname: "bob".to_string(),
            started_unix: 200,
            watchers: 0,
            status: "Zork III".to_string(),
        },
    ]);
    assert_eq!(
        String::from_utf8(block).unwrap(),
        "game\talice\t100\t2\tZork I \u{b7} West of House\ngame\tbob\t200\t0\tZork III\nend\n"
    );
    assert_eq!(encode_list(&[]), b"end\n");
}

/// A watcher that joins mid-game and then follows diffs ends up with exactly
/// the player's screen: a reverse-video status bar, a scroll region scrolled
/// after the join, and a window change.
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

    // Before the watcher joins: the status bar and a story scroll region.
    host.process(b"\x1b[2J\x1b[H\x1b[7m West of House     Score: 0   Moves: 1\x1b[0m\x1b[2;24r");
    step(&mut host, b"");
    step(
        &mut host,
        b"\x1b[24;1H>open mailbox\r\nOpening the mailbox reveals a leaflet.\r\n",
    );
    step(&mut host, b">");
    // The player resizes: a reset frame at the new size.
    host.screen_mut().set_size(30, 100);
    step(&mut host, b"\x1b[2J\x1b[1;1H\x1b[7m North of House\x1b[0m");
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
    let handle = registry.register("alice", Edition::Zork1, 80, 24);
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

/// An edition switch starts the next child while the old bridge is still
/// tearing down: the newer game stays listed.
#[test]
fn a_switch_survives_the_old_games_teardown() {
    let registry = LiveRegistry::new();
    let old = registry.register("alice", Edition::Zork1, 80, 24);
    let new = registry.register("alice", Edition::Zork2, 80, 24);
    drop(old);
    assert_eq!(registry.list().len(), 1, "the newer game stays listed");
    assert_eq!(registry.list()[0].status, "Zork II");
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
    let _handle = registry.register("alice", Edition::Zork1, 80, 24);
    let game = registry.get("alice").unwrap();
    let guard = WatcherGuard::new(registry.clone(), game);
    assert_eq!(registry.list()[0].watchers, 1);
    drop(guard);
    assert_eq!(registry.list()[0].watchers, 0);
}

#[test]
fn hud_status_reads_the_room_off_the_status_bar() {
    assert_eq!(
        hud_status(" West of House                       Score: 0        Moves: 1"),
        Some("West of House".to_string())
    );
    // The narrow form a small terminal gets, and a room long enough to push
    // the score to one space away.
    assert_eq!(
        hud_status(" Round Room   S: 35  M: 120"),
        Some("Round Room".to_string())
    );
    assert_eq!(
        hud_status(" Engravings Cave Score: 10 Moves: 9"),
        Some("Engravings Cave".to_string())
    );
    // A timed story's clock.
    assert_eq!(
        hud_status(" Shore            Time:  9:05 am"),
        Some("Shore".to_string())
    );
}

#[test]
fn hud_status_is_none_without_the_status_bar() {
    assert_eq!(hud_status(""), None);
    assert_eq!(hud_status("ZORK I: The Great Underground Empire"), None);
    assert_eq!(hud_status("     Score: 0   Moves: 1"), None);
    assert_eq!(
        hud_status(&format!(
            " {} Score: 0 Moves: 1",
            "x".repeat(PLACE_MAX_CHARS + 1)
        )),
        None
    );
}

/// The roster shows the edition from the start, adds the room once the
/// status bar is drawn, and keeps it while the bar is off screen.
#[test]
fn the_roster_keeps_a_room_while_the_status_bar_is_gone() {
    let registry = LiveRegistry::new();
    let handle = registry.register("alice", Edition::Zork2, 80, 24);
    assert_eq!(registry.list()[0].status, "Zork II");

    handle.feed(b"\x1b[2J\x1b[1;1H Inside the Barrow    Score: 0   Moves: 0");
    assert_eq!(
        registry.list()[0].status,
        "Zork II \u{b7} Inside the Barrow"
    );

    handle.feed(b"\x1b[2J\x1b[1;1HSave description:");
    assert_eq!(
        registry.list()[0].status,
        "Zork II \u{b7} Inside the Barrow"
    );

    handle.feed(b"\x1b[2J\x1b[1;1H Narrow Tunnel    Score: 0   Moves: 1");
    assert_eq!(registry.list()[0].status, "Zork II \u{b7} Narrow Tunnel");
}
