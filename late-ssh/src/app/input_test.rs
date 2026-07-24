use super::*;

#[test]
fn rect_contains_treats_edges_correctly() {
    let r = Rect {
        x: 5,
        y: 10,
        width: 3,
        height: 2,
    };
    // top-left corner is inside
    assert!(rect_contains(r, 5, 10));
    // bottom-right exclusive corner is outside
    assert!(!rect_contains(r, 8, 12));
    // last inside cell on each axis
    assert!(rect_contains(r, 7, 11));
    // just outside on each axis
    assert!(!rect_contains(r, 4, 10));
    assert!(!rect_contains(r, 5, 9));
    assert!(!rect_contains(r, 8, 11));
    assert!(!rect_contains(r, 7, 12));
}

#[test]
fn rect_contains_handles_overflow_safely() {
    let r = Rect {
        x: u16::MAX - 1,
        y: 0,
        width: 5,
        height: 1,
    };
    // saturating_add prevents wrap while keeping the right edge exclusive.
    assert!(rect_contains(r, u16::MAX - 1, 0));
    assert!(!rect_contains(r, u16::MAX, 0));
}

#[test]
fn composer_double_click_window_is_half_second() {
    assert_eq!(
        COMPOSER_DOUBLE_CLICK_WINDOW,
        std::time::Duration::from_millis(500)
    );
}

#[test]
fn profile_click_debounce_matches_chat_double_click_window() {
    assert_eq!(PROFILE_CLICK_DEBOUNCE, CHAT_CLICK_DOUBLE_WINDOW);
}

#[test]
fn blocks_arrow_when_chat_is_composing_on_dashboard() {
    let ctx = InputContext {
        screen: Screen::Dashboard,
        chat_composing: true,
        chat_ac_active: false,
        feeds_processing: false,
        news_composing: false,
        showcase_composing: false,
        work_composing: false,
        directory_tab: DirectoryTab::Profiles,
    };
    assert!(ctx.blocks_arrow_sequence());
}

#[test]
fn blocks_arrow_when_chat_is_composing_on_chat_screen() {
    let ctx = InputContext {
        screen: Screen::Dashboard,
        chat_composing: true,
        chat_ac_active: false,
        feeds_processing: false,
        news_composing: false,
        showcase_composing: false,
        work_composing: false,
        directory_tab: DirectoryTab::Profiles,
    };
    assert!(ctx.blocks_arrow_sequence());
}

#[test]
fn allows_arrow_when_idle() {
    let ctx = InputContext {
        screen: Screen::Dashboard,
        chat_composing: false,
        chat_ac_active: false,
        feeds_processing: false,
        news_composing: false,
        showcase_composing: false,
        work_composing: false,
        directory_tab: DirectoryTab::Profiles,
    };
    assert!(!ctx.blocks_arrow_sequence());
}

#[test]
fn compose_room_switch_allowed_on_chat_surfaces() {
    assert!(compose_room_switch_allowed(Screen::Dashboard));
    assert!(compose_room_switch_allowed(Screen::Dashboard));
    assert!(!compose_room_switch_allowed(Screen::Arcade));
}

#[test]
fn topbar_screen_hit_test_maps_screen_digits() {
    assert_eq!(topbar_screen_hit_test(12, 0), Some(Screen::Clubhouse));
    assert_eq!(topbar_screen_hit_test(14, 0), Some(Screen::Dashboard));
    assert_eq!(topbar_screen_hit_test(16, 0), Some(Screen::Arcade));
    assert_eq!(topbar_screen_hit_test(18, 0), Some(Screen::Games));
    assert_eq!(topbar_screen_hit_test(20, 0), Some(Screen::Artboard));
    assert_eq!(topbar_screen_hit_test(22, 0), Some(Screen::Pinstar));
    // The door games are no longer top-level tabs; the column past the last
    // digit and the gaps between digits map to nothing.
    assert_eq!(topbar_screen_hit_test(24, 0), None);
    assert_eq!(topbar_screen_hit_test(13, 0), None);
    assert_eq!(topbar_screen_hit_test(12, 1), None);
}

#[test]
fn vt_parser_reads_arrow_sequence() {
    let mut parser = VtInputParser::default();
    assert_eq!(parser.feed(b"\x1b[A"), vec![ParsedInput::Arrow(b'A')]);
}

#[test]
fn vt_parser_reads_ss3_arrow_sequence() {
    let mut parser = VtInputParser::default();
    assert_eq!(parser.feed(b"\x1bOD"), vec![ParsedInput::Arrow(b'D')]);
}

#[test]
fn vt_parser_reads_backtab_sequence() {
    let mut parser = VtInputParser::default();
    assert_eq!(parser.feed(b"\x1b[Z"), vec![ParsedInput::BackTab]);
}

#[test]
fn lone_escape_then_key_is_swallowed_as_alt_chord() {
    // A bare ESC leaves the parser wedged mid-escape (no event yet), so the
    // following byte merges into an `ESC 1` = Alt+1 chord that esc_dispatch
    // drops. This is why dismissing the splash with Escape used to eat the
    // first `1` — the splash path must `reset()` to avoid it.
    let mut parser = VtInputParser::default();
    assert_eq!(parser.feed(b"\x1b"), vec![]);
    assert_eq!(parser.feed(b"1"), vec![]);
}

#[test]
fn reset_after_lone_escape_lets_the_next_key_through() {
    let mut parser = VtInputParser::default();
    assert_eq!(parser.feed(b"\x1b"), vec![]);
    parser.reset();
    assert_eq!(parser.feed(b"1"), vec![ParsedInput::Char('1')]);
}

#[test]
fn vt_parser_reads_da1_reply() {
    let mut parser = VtInputParser::default();
    assert_eq!(
        parser.feed(b"\x1b[?62;4;22c"),
        vec![ParsedInput::DeviceAttributes(vec![62, 4, 22])]
    );
}

#[test]
fn vt_parser_ignores_da2_reply() {
    let mut parser = VtInputParser::default();
    // Secondary Device Attributes uses the `>` marker; only DA1 (`?`)
    // should surface as DeviceAttributes.
    assert_eq!(parser.feed(b"\x1b[>1;10;0c"), vec![]);
}

#[test]
fn vt_parser_reads_multiple_probe_replies_in_one_chunk() {
    let mut parser = VtInputParser::default();
    // Terminals answer the startup probes back-to-back, so XTVERSION and
    // DA1 replies routinely share one data chunk over a low-latency link.
    // Both events must surface; the splash handler applies every one.
    assert_eq!(
        parser.feed(b"\x1bP>|XTerm(370)\x1b\\\x1b[?62;4;22c"),
        vec![
            ParsedInput::TerminalVersion("XTerm(370)".to_string()),
            ParsedInput::DeviceAttributes(vec![62, 4, 22]),
        ]
    );
}

#[test]
fn vt_parser_parses_scroll_events() {
    let mut parser = VtInputParser::default();
    assert_eq!(
        parser.feed(b"\x1b[<64;10;5M"),
        vec![ParsedInput::Mouse(MouseEvent {
            kind: MouseEventKind::ScrollUp,
            button: None,
            x: 10,
            y: 5,
            modifiers: MouseModifiers::default(),
        })]
    );
    assert_eq!(
        parser.feed(b"\x1b[<65;10;5m"),
        vec![ParsedInput::Mouse(MouseEvent {
            kind: MouseEventKind::ScrollDown,
            button: None,
            x: 10,
            y: 5,
            modifiers: MouseModifiers::default(),
        })]
    );
}

#[test]
fn vt_parser_parses_horizontal_scroll_events() {
    let mut parser = VtInputParser::default();
    assert_eq!(
        parser.feed(b"\x1b[<66;8;3M"),
        vec![ParsedInput::Mouse(MouseEvent {
            kind: MouseEventKind::ScrollLeft,
            button: None,
            x: 8,
            y: 3,
            modifiers: MouseModifiers::default(),
        })]
    );
    assert_eq!(
        parser.feed(b"\x1b[<67;8;3M"),
        vec![ParsedInput::Mouse(MouseEvent {
            kind: MouseEventKind::ScrollRight,
            button: None,
            x: 8,
            y: 3,
            modifiers: MouseModifiers::default(),
        })]
    );
}

#[test]
fn vt_parser_parses_ctrl_sequences() {
    let mut parser = VtInputParser::default();
    assert_eq!(
        parser.feed(b"\x1b[1;5C"),
        vec![ParsedInput::CtrlArrow(b'C')]
    );
    assert_eq!(parser.feed(b"\x1b[5D"), vec![ParsedInput::CtrlArrow(b'D')]);
    // Alt+Arrow (xterm modifier 3). Kitty emits this for Option-Arrow /
    // Alt-Arrow in its default mode; consumers alias it to word-jump.
    assert_eq!(parser.feed(b"\x1b[1;3D"), vec![ParsedInput::AltArrow(b'D')]);
    assert_eq!(parser.feed(b"\x1b[1;3C"), vec![ParsedInput::AltArrow(b'C')]);
    // Unmodified Arrow falls through unchanged.
    assert_eq!(parser.feed(b"\x1b[D"), vec![ParsedInput::Arrow(b'D')]);
    assert_eq!(parser.feed(b"\x1b[3~"), vec![ParsedInput::Delete]);
    assert_eq!(parser.feed(b"\x1b[3;5~"), vec![ParsedInput::CtrlDelete]);
    assert_eq!(
        parser.feed(b"\x1b[127;5u"),
        vec![ParsedInput::CtrlBackspace]
    );
    assert_eq!(parser.feed(b"\x1b[8;5u"), vec![ParsedInput::CtrlBackspace]);
    assert_eq!(parser.feed(b"\x1b[8;5~"), vec![ParsedInput::CtrlBackspace]);
    assert_eq!(parser.feed(b"\x1b[47;5u"), vec![ParsedInput::Byte(0x1F)]);
    // Raw ^H (0x08) and DEL (0x7F) stay plain bytes; the composer maps ^H to
    // word-delete itself. The CSI-u forms above are the explicit Ctrl+BS.
    assert_eq!(parser.feed(b"\x08"), vec![ParsedInput::Byte(0x08)]);
    assert_eq!(parser.feed(b"\x7f"), vec![ParsedInput::Byte(0x7f)]);
}

#[test]
fn vt_parser_keeps_split_arrow_state_across_reads() {
    let mut parser = VtInputParser::default();
    assert!(parser.feed(b"\x1b[").is_empty());
    assert_eq!(parser.feed(b"A"), vec![ParsedInput::Arrow(b'A')]);
}

#[test]
fn vt_parser_consumes_alt_printable_without_emitting_bytes() {
    let mut parser = VtInputParser::default();
    assert!(parser.feed(b"\x1bq").is_empty());
}

#[test]
fn vt_parser_emits_alt_c_for_explicit_clipboard_chord() {
    let mut parser = VtInputParser::default();
    assert_eq!(parser.feed(b"\x1bc"), vec![ParsedInput::AltC]);
}

#[test]
fn vt_parser_emits_alt_a_for_explicit_aquarium_chord() {
    let mut parser = VtInputParser::default();
    assert_eq!(parser.feed(b"\x1ba"), vec![ParsedInput::AltA]);
    assert_eq!(parser.feed(b"\x1bA"), vec![ParsedInput::AltA]);
}

#[test]
fn vt_parser_reset_clears_pending_escape_state() {
    let mut parser = VtInputParser::default();
    assert!(parser.feed(b"\x1b").is_empty());
    parser.reset();
    assert_eq!(parser.feed(b"j"), vec![ParsedInput::Char('j')]);
}

#[test]
fn vt_parser_keeps_split_bracketed_paste_state_across_reads() {
    let mut parser = VtInputParser::default();
    assert!(parser.feed(b"\x1b[200~hello").is_empty());
    assert_eq!(
        parser.feed(b"\nworld\x1b[201~"),
        vec![ParsedInput::Paste(b"hello\nworld".to_vec())]
    );
}

#[test]
fn paste_target_prefers_chat_composer() {
    let ctx = InputContext {
        screen: Screen::Dashboard,
        chat_composing: true,
        chat_ac_active: false,
        feeds_processing: false,
        news_composing: true,
        showcase_composing: false,
        work_composing: false,
        directory_tab: DirectoryTab::Profiles,
    };
    assert_eq!(paste_target(ctx), PasteTarget::ChatComposer);
}

#[test]
fn paste_target_routes_to_news_composer() {
    let ctx = InputContext {
        screen: Screen::Dashboard,
        chat_composing: false,
        chat_ac_active: false,
        feeds_processing: false,
        news_composing: true,
        showcase_composing: false,
        work_composing: false,
        directory_tab: DirectoryTab::Profiles,
    };
    assert_eq!(paste_target(ctx), PasteTarget::NewsComposer);
}

#[test]
fn paste_target_routes_to_showcase_composer() {
    let ctx = InputContext {
        screen: Screen::Dashboard,
        chat_composing: false,
        chat_ac_active: false,
        feeds_processing: false,
        news_composing: false,
        showcase_composing: true,
        work_composing: false,
        directory_tab: DirectoryTab::Profiles,
    };
    assert_eq!(paste_target(ctx), PasteTarget::ShowcaseComposer);
}

#[test]
fn insert_pasted_text_normalizes_newlines_and_filters_controls() {
    let mut out = String::new();
    insert_pasted_text(b"hello\r\nworld\x00\rok\x7f", |ch| out.push(ch));
    assert_eq!(out, "hello\nworld\nok");
}

#[test]
fn split_alt_enter_returns_plain_bytes_when_no_trigger() {
    let chunks = split_escaped_input(b"hello");
    assert_eq!(chunks, vec![EscapedInputChunk::Bytes(b"hello")]);
}

#[test]
fn split_escaped_input_splits_on_inline_escape_cr() {
    let chunks = split_escaped_input(b"ab\x1b\rcd");
    assert_eq!(
        chunks,
        vec![
            EscapedInputChunk::Bytes(b"ab"),
            EscapedInputChunk::Event(ParsedInput::AltEnter),
            EscapedInputChunk::Bytes(b"cd"),
        ]
    );
}

#[test]
fn split_escaped_input_handles_escape_lf_variant() {
    let chunks = split_escaped_input(b"\x1b\n");
    assert_eq!(
        chunks,
        vec![EscapedInputChunk::Event(ParsedInput::AltEnter)]
    );
}

#[test]
fn split_escaped_input_handles_escape_backspace_variants() {
    let chunks = split_escaped_input(b"\x1b\x08\x1b\x7fx");
    assert_eq!(
        chunks,
        vec![
            EscapedInputChunk::Event(ParsedInput::CtrlBackspace),
            EscapedInputChunk::Event(ParsedInput::CtrlBackspace),
            EscapedInputChunk::Bytes(b"x"),
        ]
    );
}

#[test]
fn split_escaped_input_handles_consecutive_triggers() {
    let chunks = split_escaped_input(b"\x1b\r\x1b\nx");
    assert_eq!(
        chunks,
        vec![
            EscapedInputChunk::Event(ParsedInput::AltEnter),
            EscapedInputChunk::Event(ParsedInput::AltEnter),
            EscapedInputChunk::Bytes(b"x"),
        ]
    );
}

#[test]
fn split_escaped_input_leaves_trailing_lone_escape_for_pending_logic() {
    // A bare ESC at the end of the buffer is left in the byte stream so
    // handle()'s trailing-ESC bookkeeping can set pending_escape.
    let chunks = split_escaped_input(b"ab\x1b");
    assert_eq!(chunks, vec![EscapedInputChunk::Bytes(b"ab\x1b")]);
}

#[test]
fn vt_parser_parses_page_keys_numeric_form() {
    let mut parser = VtInputParser::default();
    assert_eq!(parser.feed(b"\x1b[5~"), vec![ParsedInput::PageUp]);
    assert_eq!(parser.feed(b"\x1b[6~"), vec![ParsedInput::PageDown]);
    assert_eq!(parser.feed(b"\x1b[4~"), vec![ParsedInput::End]);
    assert_eq!(parser.feed(b"\x1b[8~"), vec![ParsedInput::End]);
}

#[test]
fn vt_parser_parses_end_bare_form() {
    let mut parser = VtInputParser::default();
    assert_eq!(parser.feed(b"\x1b[F"), vec![ParsedInput::End]);
}

#[test]
fn vt_parser_parses_end_ss3_form() {
    let mut parser = VtInputParser::default();
    assert_eq!(parser.feed(b"\x1bOF"), vec![ParsedInput::End]);
}

#[test]
fn vt_parser_parses_home_forms() {
    let mut parser = VtInputParser::default();
    assert_eq!(parser.feed(b"\x1b[1~"), vec![ParsedInput::Home]);
    assert_eq!(parser.feed(b"\x1b[7~"), vec![ParsedInput::Home]);
    assert_eq!(parser.feed(b"\x1b[H"), vec![ParsedInput::Home]);
    assert_eq!(parser.feed(b"\x1bOH"), vec![ParsedInput::Home]);
}

#[test]
fn vt_parser_parses_modified_arrow_variants() {
    let mut parser = VtInputParser::default();
    assert_eq!(
        parser.feed(b"\x1b[1;2A"),
        vec![ParsedInput::ShiftArrow(b'A')]
    );
    assert_eq!(parser.feed(b"\x1b[2A"), vec![ParsedInput::ShiftArrow(b'A')]);
    assert_eq!(parser.feed(b"\x1b[1;3B"), vec![ParsedInput::AltArrow(b'B')]);
    assert_eq!(parser.feed(b"\x1b[3C"), vec![ParsedInput::AltArrow(b'C')]);
    assert_eq!(
        parser.feed(b"\x1b[1;6D"),
        vec![ParsedInput::CtrlShiftArrow(b'D')]
    );
    assert_eq!(
        parser.feed(b"\x1b[6A"),
        vec![ParsedInput::CtrlShiftArrow(b'A')]
    );
}

#[test]
fn vt_parser_parses_mouse_press_and_release() {
    let mut parser = VtInputParser::default();
    assert_eq!(
        parser.feed(b"\x1b[<0;10;5M"),
        vec![ParsedInput::Mouse(MouseEvent {
            kind: MouseEventKind::Down,
            button: Some(MouseButton::Left),
            x: 10,
            y: 5,
            modifiers: MouseModifiers::default(),
        })]
    );
    assert_eq!(
        parser.feed(b"\x1b[<0;10;5m"),
        vec![ParsedInput::Mouse(MouseEvent {
            kind: MouseEventKind::Up,
            button: Some(MouseButton::Left),
            x: 10,
            y: 5,
            modifiers: MouseModifiers::default(),
        })]
    );
    assert_eq!(
        parser.feed(b"\x1b[<2;10;5M"),
        vec![ParsedInput::Mouse(MouseEvent {
            kind: MouseEventKind::Down,
            button: Some(MouseButton::Right),
            x: 10,
            y: 5,
            modifiers: MouseModifiers::default(),
        })]
    );
}

#[test]
fn vt_parser_parses_mouse_drag_and_move() {
    let mut parser = VtInputParser::default();
    // Left-button drag: base button 0 + motion bit 32 = 32.
    assert_eq!(
        parser.feed(b"\x1b[<32;4;6M"),
        vec![ParsedInput::Mouse(MouseEvent {
            kind: MouseEventKind::Drag,
            button: Some(MouseButton::Left),
            x: 4,
            y: 6,
            modifiers: MouseModifiers::default(),
        })]
    );
    // Hover / motion without a button: low bits = 3, plus motion bit 32 = 35.
    assert_eq!(
        parser.feed(b"\x1b[<35;4;6M"),
        vec![ParsedInput::Mouse(MouseEvent {
            kind: MouseEventKind::Moved,
            button: None,
            x: 4,
            y: 6,
            modifiers: MouseModifiers::default(),
        })]
    );
}

#[test]
fn vt_parser_parses_mouse_modifier_bits() {
    let mut parser = VtInputParser::default();
    // Left press with Shift (bit 4): 0 | 4 = 4.
    assert_eq!(
        parser.feed(b"\x1b[<4;1;1M"),
        vec![ParsedInput::Mouse(MouseEvent {
            kind: MouseEventKind::Down,
            button: Some(MouseButton::Left),
            x: 1,
            y: 1,
            modifiers: MouseModifiers {
                shift: true,
                alt: false,
                ctrl: false
            },
        })]
    );
    // Left press with Ctrl+Alt (bits 16|8 = 24): 0 | 24 = 24.
    assert_eq!(
        parser.feed(b"\x1b[<24;2;3M"),
        vec![ParsedInput::Mouse(MouseEvent {
            kind: MouseEventKind::Down,
            button: Some(MouseButton::Left),
            x: 2,
            y: 3,
            modifiers: MouseModifiers {
                shift: false,
                alt: true,
                ctrl: true
            },
        })]
    );
}

#[test]
fn vt_parser_emits_char_for_printable_non_ascii() {
    let mut parser = VtInputParser::default();
    assert_eq!(parser.feed("т".as_bytes()), vec![ParsedInput::Char('т')]);
    assert_eq!(parser.feed("漢".as_bytes()), vec![ParsedInput::Char('漢')]);
    assert_eq!(parser.feed("ł".as_bytes()), vec![ParsedInput::Char('ł')]);
}

#[test]
fn vt_parser_emits_char_for_ascii_printable() {
    let mut parser = VtInputParser::default();
    assert_eq!(parser.feed(b"a"), vec![ParsedInput::Char('a')]);
    assert_eq!(parser.feed(b" "), vec![ParsedInput::Char(' ')]);
    assert_eq!(parser.feed(b"~"), vec![ParsedInput::Char('~')]);
}

#[test]
fn vt_parser_emits_one_char_per_codepoint_for_full_word() {
    let mut parser = VtInputParser::default();
    assert_eq!(
        parser.feed("тест".as_bytes()),
        vec![
            ParsedInput::Char('т'),
            ParsedInput::Char('е'),
            ParsedInput::Char('с'),
            ParsedInput::Char('т'),
        ]
    );
}

#[test]
fn vt_parser_preserves_ascii_controls_as_bytes() {
    let mut parser = VtInputParser::default();
    assert_eq!(parser.feed(b"\r"), vec![ParsedInput::Byte(b'\r')]);
    assert_eq!(parser.feed(b"\n"), vec![ParsedInput::Byte(b'\n')]);
    assert_eq!(parser.feed(b"\x15"), vec![ParsedInput::Byte(0x15)]);
    assert_eq!(parser.feed(b"\x7f"), vec![ParsedInput::Byte(0x7f)]);
}

#[test]
fn vt_parser_preserves_del_when_adjacent_to_printable_bytes() {
    let mut parser = VtInputParser::default();
    assert_eq!(
        parser.feed(b"\x7f!"),
        vec![ParsedInput::Byte(0x7f), ParsedInput::Char('!')]
    );
}

#[test]
fn vt_parser_interleaves_ascii_and_non_ascii() {
    let mut parser = VtInputParser::default();
    assert_eq!(
        parser.feed("café".as_bytes()),
        vec![
            ParsedInput::Char('c'),
            ParsedInput::Char('a'),
            ParsedInput::Char('f'),
            ParsedInput::Char('é'),
        ]
    );
}

#[test]
fn insert_pasted_text_strips_bracketed_paste_markers() {
    let mut out = String::new();
    insert_pasted_text(b"\x1b[200~https://example.com\x1b[201~", |ch| out.push(ch));
    assert_eq!(out, "https://example.com");

    // Literal residue (ESC already stripped by an earlier stage).
    let mut out = String::new();
    insert_pasted_text(b"[200~https://example.com[201~", |ch| out.push(ch));
    assert_eq!(out, "https://example.com");
}

#[test]
fn sanitize_paste_markers_cleans_stored_urls() {
    assert_eq!(
        sanitize_paste_markers("[200~https://example.com[201~"),
        "https://example.com"
    );
    assert_eq!(
        sanitize_paste_markers("\x1b[200~https://example.com\x1b[201~"),
        "https://example.com"
    );
    assert_eq!(
        sanitize_paste_markers("https://example.com"),
        "https://example.com"
    );
}

#[test]
fn room_section_suffixes_map_plain_keys_to_sections() {
    assert_eq!(room_section_suffix(b'f'), Some(RoomSection::Favorites));
    assert_eq!(room_section_suffix(b'o'), Some(RoomSection::Core));
    assert_eq!(room_section_suffix(b'c'), Some(RoomSection::Channels));
    assert_eq!(room_section_suffix(b'u'), Some(RoomSection::Updates));
    assert_eq!(room_section_suffix(b'd'), Some(RoomSection::Dms));
    assert_eq!(room_section_suffix(b'x'), None);
}

// --- autocomplete arrow routing ---

#[test]
fn allows_arrow_when_autocomplete_active() {
    let ctx = InputContext {
        screen: Screen::Dashboard,
        chat_composing: true,
        chat_ac_active: true,
        feeds_processing: false,
        news_composing: false,
        showcase_composing: false,
        work_composing: false,
        directory_tab: DirectoryTab::Profiles,
    };
    assert!(!ctx.blocks_arrow_sequence());
}

#[test]
fn blocks_arrow_when_composing_without_autocomplete() {
    let ctx = InputContext {
        screen: Screen::Dashboard,
        chat_composing: true,
        chat_ac_active: false,
        feeds_processing: false,
        news_composing: false,
        showcase_composing: false,
        work_composing: false,
        directory_tab: DirectoryTab::Profiles,
    };
    assert!(ctx.blocks_arrow_sequence());
}

#[test]
fn overlay_input_action_accepts_printable_chars_and_arrows() {
    assert_eq!(
        overlay_input_action(&ParsedInput::Char('j')),
        Some(OverlayInputAction::Scroll(1))
    );
    assert_eq!(
        overlay_input_action(&ParsedInput::Char('k')),
        Some(OverlayInputAction::Scroll(-1))
    );
    assert_eq!(
        overlay_input_action(&ParsedInput::Char('q')),
        Some(OverlayInputAction::Close)
    );
    assert_eq!(
        overlay_input_action(&ParsedInput::Arrow(b'B')),
        Some(OverlayInputAction::Scroll(1))
    );
    assert_eq!(
        overlay_input_action(&ParsedInput::Arrow(b'A')),
        Some(OverlayInputAction::Scroll(-1))
    );
}

// ── Chat-scroll click classification ────────────────────────

use crate::app::chat::ui::HeaderSegment;

fn header_hit(message_id: Uuid, segments: Vec<HeaderSegment>) -> ChatRowHit {
    ChatRowHit {
        message_id: Some(message_id),
        kind: ChatRowKind::Header(segments),
    }
}

#[test]
fn classify_chat_hit_routes_username_column_to_profile() {
    let mid = Uuid::now_v7();
    let hit = header_hit(
        mid,
        vec![HeaderSegment {
            start_col: 1,
            end_col: 6,
            target: HeaderTarget::Profile,
        }],
    );
    assert_eq!(
        classify_chat_hit(&hit, 3),
        Some(ChatClickKind::ProfileOf { message_id: mid })
    );
}

#[test]
fn classify_chat_hit_routes_store_badge_column_to_shop() {
    let mid = Uuid::now_v7();
    let hit = header_hit(
        mid,
        vec![
            HeaderSegment {
                start_col: 1,
                end_col: 6,
                target: HeaderTarget::Profile,
            },
            HeaderSegment {
                start_col: 8,
                end_col: 10,
                target: HeaderTarget::StoreBadge,
            },
        ],
    );
    assert_eq!(classify_chat_hit(&hit, 9), Some(ChatClickKind::StoreBadge));
}

#[test]
fn classify_chat_hit_routes_store_flag_column_to_flags_shop() {
    let mid = Uuid::now_v7();
    let hit = header_hit(
        mid,
        vec![
            HeaderSegment {
                start_col: 1,
                end_col: 6,
                target: HeaderTarget::Profile,
            },
            HeaderSegment {
                start_col: 8,
                end_col: 10,
                target: HeaderTarget::StoreFlag,
            },
        ],
    );
    assert_eq!(classify_chat_hit(&hit, 9), Some(ChatClickKind::StoreFlag));
}

#[test]
fn classify_chat_hit_falls_through_gap_between_segments_to_body() {
    let mid = Uuid::now_v7();
    let hit = header_hit(
        mid,
        vec![
            HeaderSegment {
                start_col: 1,
                end_col: 6,
                target: HeaderTarget::Profile,
            },
            HeaderSegment {
                start_col: 8,
                end_col: 10,
                target: HeaderTarget::StoreBadge,
            },
        ],
    );
    // Column 7 is the separator space — no segment owns it.
    assert_eq!(
        classify_chat_hit(&hit, 7),
        Some(ChatClickKind::BodySelect { message_id: mid })
    );
}

#[test]
fn classify_chat_hit_body_and_image_use_message_id() {
    let mid = Uuid::now_v7();
    let body = ChatRowHit {
        message_id: Some(mid),
        kind: ChatRowKind::Body,
    };
    let image = ChatRowHit {
        message_id: Some(mid),
        kind: ChatRowKind::Image,
    };
    assert_eq!(
        classify_chat_hit(&body, 0),
        Some(ChatClickKind::BodySelect { message_id: mid })
    );
    assert_eq!(
        classify_chat_hit(&image, 0),
        Some(ChatClickKind::Image { message_id: mid })
    );
}

#[test]
fn classify_chat_hit_blank_or_missing_message_yields_none() {
    let blank = ChatRowHit {
        message_id: None,
        kind: ChatRowKind::None,
    };
    let orphan_body = ChatRowHit {
        message_id: None,
        kind: ChatRowKind::Body,
    };
    assert!(classify_chat_hit(&blank, 0).is_none());
    assert!(classify_chat_hit(&orphan_body, 0).is_none());
}

#[test]
fn chat_click_kind_double_click_followup_only_for_body_and_profile() {
    let mid = Uuid::now_v7();
    assert!(ChatClickKind::BodySelect { message_id: mid }.has_double_click_followup());
    assert!(ChatClickKind::ProfileOf { message_id: mid }.has_double_click_followup());
    assert!(!ChatClickKind::StoreBadge.has_double_click_followup());
    assert!(!ChatClickKind::StoreFlag.has_double_click_followup());
    assert!(!ChatClickKind::Image { message_id: mid }.has_double_click_followup());
}
