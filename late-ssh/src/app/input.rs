use super::{chat, dashboard, profile, state::App};
use crate::app::common::primitives::Screen;

#[derive(Clone, Copy)]
struct InputContext {
    screen: Screen,
    chat_composing: bool,
    chat_ac_active: bool,
    news_composing: bool,
    profile_composing: bool,
}

impl InputContext {
    fn from_app(app: &App) -> Self {
        Self {
            screen: app.screen,
            chat_composing: app.chat.is_composing(),
            chat_ac_active: app.chat.is_autocomplete_active(),
            news_composing: app.chat.news.composing(),
            profile_composing: app.profile_state.editing_username(),
        }
    }

    fn blocks_arrow_sequence(self) -> bool {
        let chat_screen = (self.screen == Screen::Dashboard || self.screen == Screen::Chat)
            && self.chat_composing;
        // Allow arrows through when autocomplete is active
        if chat_screen && self.chat_ac_active {
            return false;
        }
        chat_screen
            || (self.screen == Screen::Chat && self.news_composing)
            || (self.screen == Screen::Profile && self.profile_composing)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PasteTarget {
    None,
    ChatComposer,
    NewsComposer,
}

#[derive(Clone, Copy)]
struct DecodedInput {
    byte: u8,
    consumed: usize,
    arrow_key: Option<u8>,
    ctrl_arrow_key: Option<u8>,
    ctrl_backspace: bool,
    ctrl_delete: bool,
    scroll: Option<isize>,
    alt_enter: bool,
}

fn decode_input(data: &[u8], index: usize) -> DecodedInput {
    let byte = data[index];
    // Alt+Enter: ESC followed by CR or LF
    if byte == 0x1B && index + 1 < data.len() && matches!(data[index + 1], b'\r' | b'\n') {
        return DecodedInput {
            byte,
            consumed: 2,
            arrow_key: None,
            ctrl_arrow_key: None,
            ctrl_backspace: false,
            ctrl_delete: false,
            scroll: None,
            alt_enter: true,
        };
    }
    if let Some((consumed, key)) = parse_ctrl_arrow(data, index) {
        return DecodedInput {
            byte,
            consumed,
            arrow_key: None,
            ctrl_arrow_key: Some(key),
            ctrl_backspace: false,
            ctrl_delete: false,
            scroll: None,
            alt_enter: false,
        };
    }
    if let Some(consumed) = parse_ctrl_backspace(data, index) {
        return DecodedInput {
            byte,
            consumed,
            arrow_key: None,
            ctrl_arrow_key: None,
            ctrl_backspace: true,
            ctrl_delete: false,
            scroll: None,
            alt_enter: false,
        };
    }
    if let Some(consumed) = parse_ctrl_delete(data, index) {
        return DecodedInput {
            byte,
            consumed,
            arrow_key: None,
            ctrl_arrow_key: None,
            ctrl_backspace: false,
            ctrl_delete: true,
            scroll: None,
            alt_enter: false,
        };
    }
    if byte == 0x1B && index + 2 < data.len() && matches!(data[index + 1], b'[' | b'O') {
        // SGR mouse: ESC [ < button ; col ; row M/m
        if data[index + 1] == b'['
            && data[index + 2] == b'<'
            && let Some((consumed, scroll)) = parse_sgr_mouse(data, index + 3)
        {
            return DecodedInput {
                byte,
                consumed: 3 + consumed,
                arrow_key: None,
                ctrl_arrow_key: None,
                ctrl_backspace: false,
                ctrl_delete: false,
                scroll: Some(scroll.unwrap_or(0)),
                alt_enter: false,
            };
        }

        return DecodedInput {
            byte,
            consumed: 3,
            arrow_key: Some(data[index + 2]),
            ctrl_arrow_key: None,
            ctrl_backspace: false,
            ctrl_delete: false,
            scroll: None,
            alt_enter: false,
        };
    }

    // Alt+<printable key>: ESC followed by a printable byte that didn't match any
    // known sequence above. Consume both bytes so ESC doesn't exit composing mode
    // and the key doesn't leak through as a separate input (e.g. Alt+Q → quit).
    if byte == 0x1B && index + 1 < data.len() && (32..127).contains(&data[index + 1]) {
        return DecodedInput {
            byte,
            consumed: 2,
            arrow_key: None,
            ctrl_arrow_key: None,
            ctrl_backspace: false,
            ctrl_delete: false,
            scroll: None,
            alt_enter: false,
        };
    }

    DecodedInput {
        byte,
        consumed: 1,
        arrow_key: None,
        ctrl_arrow_key: None,
        ctrl_backspace: false,
        ctrl_delete: false,
        scroll: None,
        alt_enter: false,
    }
}

fn parse_ctrl_arrow(data: &[u8], index: usize) -> Option<(usize, u8)> {
    const SEQUENCES: [(&[u8], u8); 4] = [
        (b"\x1b[1;5C", b'C'),
        (b"\x1b[1;5D", b'D'),
        (b"\x1b[5C", b'C'),
        (b"\x1b[5D", b'D'),
    ];

    for (seq, key) in SEQUENCES {
        if data[index..].starts_with(seq) {
            return Some((seq.len(), key));
        }
    }

    None
}

fn parse_ctrl_delete(data: &[u8], index: usize) -> Option<usize> {
    const SEQUENCES: [&[u8]; 1] = [b"\x1b[3;5~"];

    for seq in SEQUENCES {
        if data[index..].starts_with(seq) {
            return Some(seq.len());
        }
    }

    None
}

fn parse_ctrl_backspace(data: &[u8], index: usize) -> Option<usize> {
    const SEQUENCES: [&[u8]; 4] = [b"\x1b[127;5u", b"\x1b[8;5~", b"\x1b\x7f", b"\x08"];

    for seq in SEQUENCES {
        if data[index..].starts_with(seq) {
            return Some(seq.len());
        }
    }

    None
}

fn is_likely_paste(data: &[u8]) -> bool {
    let printable = data
        .iter()
        .filter(|&&b| b >= 0x20 && b != 0x7f || b == b'\n' || b == b'\r' || b == b'\t')
        .count();
    printable > 8 && printable * 100 / data.len().max(1) > 80
}

fn parse_bracketed_paste(data: &[u8], index: usize) -> Option<(usize, &[u8])> {
    const START: &[u8] = b"\x1b[200~";
    const END: &[u8] = b"\x1b[201~";

    if !data[index..].starts_with(START) {
        return None;
    }

    let body_start = index + START.len();
    let rel_end = data[body_start..]
        .windows(END.len())
        .position(|window| window == END)?;
    let body_end = body_start + rel_end;
    let consumed = START.len() + rel_end + END.len();
    Some((consumed, &data[body_start..body_end]))
}

/// Parse SGR mouse parameters after `ESC [ <`.
/// Returns (bytes consumed, optional scroll direction).
fn parse_sgr_mouse(data: &[u8], start: usize) -> Option<(usize, Option<isize>)> {
    // Find the terminator M (press) or m (release)
    let mut end = start;
    while end < data.len() && data[end] != b'M' && data[end] != b'm' {
        end += 1;
    }
    if end >= data.len() {
        return None;
    }
    let consumed = end - start + 1;

    // Parse button number (first parameter before the first ';')
    let params = &data[start..end];
    let button_end = params
        .iter()
        .position(|&b| b == b';')
        .unwrap_or(params.len());
    let button: u16 = params[..button_end].iter().fold(0u16, |acc, &b| {
        acc.wrapping_mul(10).wrapping_add((b - b'0') as u16)
    });

    let scroll = match button {
        64 => Some(1isize),  // wheel up
        65 => Some(-1isize), // wheel down
        _ => None,
    };

    Some((consumed, scroll))
}

pub fn handle(app: &mut App, data: &[u8]) {
    let owned_input;
    let data = if app.pending_escape {
        app.pending_escape = false;
        owned_input = {
            let mut bytes = Vec::with_capacity(data.len() + 1);
            bytes.push(0x1B);
            bytes.extend_from_slice(data);
            bytes
        };
        owned_input.as_slice()
    } else {
        data
    };

    if app.show_splash {
        // Do not process input while splash screen is showing
        return;
    }

    if app.show_welcome && !data.is_empty() {
        app.show_welcome = false;
        return;
    }

    // Help overlay: scroll with j/k/arrows/mouse wheel, dismiss with ?/Esc/q
    if app.show_help && !data.is_empty() {
        let mut i = 0;
        while i < data.len() {
            // ESC sequences
            if data[i] == 0x1B && i + 1 < data.len() && data[i + 1] == b'[' {
                // Arrow keys: ESC [ A/B
                if i + 2 < data.len() {
                    match data[i + 2] {
                        b'B' => app.help_scroll = app.help_scroll.saturating_add(1),
                        b'A' => app.help_scroll = app.help_scroll.saturating_sub(1),
                        _ => {}
                    }
                    i += 3;
                    continue;
                }
            }
            // Lone ESC = close
            if data[i] == 0x1B {
                app.show_help = false;
                return;
            }
            match data[i] {
                b'?' | b'q' => {
                    app.show_help = false;
                    return;
                }
                b'j' => app.help_scroll = app.help_scroll.saturating_add(1),
                b'k' => app.help_scroll = app.help_scroll.saturating_sub(1),
                _ => {}
            }
            i += 1;
        }
        return;
    }

    // Web chat QR overlay: any key dismisses
    if app.show_web_chat_qr && !data.is_empty() {
        app.show_web_chat_qr = false;
        app.web_chat_qr_url = None;
        return;
    }

    // Heuristic: detect pastes from terminals that don't support bracketed
    // paste mode. A single keystroke produces 1 byte (or up to ~8 for escape
    // sequences). If we receive many printable bytes at once without bracketed
    // paste markers, it's almost certainly pasted text. Without this, each
    // byte is processed as a key — newlines submit messages mid-paste and
    // remaining chars become navigation commands, causing chaos.
    if !data.starts_with(b"\x1b[200~") && !data.starts_with(b"\x1b[<") && is_likely_paste(data) {
        handle_bracketed_paste(app, data);
        return;
    }

    let mut i = 0;
    while i < data.len() {
        if data[i] == 0x1B && i + 1 >= data.len() {
            // In modal/composing modes, process lone ESC immediately — a real
            // Esc keypress always arrives as a single 0x1B byte, while escape
            // sequences (arrow keys etc.) arrive as a complete multi-byte chunk.
            let ctx = InputContext::from_app(app);
            if ctx.chat_composing || ctx.news_composing || ctx.profile_composing {
                handle_modal_input(app, ctx, 0x1B);
                break;
            }
            if ctx.screen == Screen::Games && app.is_playing_game {
                dispatch_screen_key(app, ctx.screen, 0x1B);
                break;
            }
            if (ctx.screen == Screen::Chat || ctx.screen == Screen::Dashboard)
                && app.chat.selected_message_id.is_some()
            {
                app.chat.clear_message_selection();
                break;
            }
            app.pending_escape = true;
            break;
        }

        if let Some((consumed, pasted)) = parse_bracketed_paste(data, i) {
            handle_bracketed_paste(app, pasted);
            i += consumed;
            continue;
        }

        let input = decode_input(data, i);
        let ctx = InputContext::from_app(app);

        if input.alt_enter {
            if (ctx.screen == Screen::Dashboard || ctx.screen == Screen::Chat) && ctx.chat_composing
            {
                app.chat.composer_push('\n');
                app.chat.update_autocomplete();
            }
            i += input.consumed;
            continue;
        }

        if let Some(delta) = input.scroll {
            handle_scroll_for_screen(app, ctx.screen, delta);
            i += input.consumed;
            continue;
        }

        if input.ctrl_backspace
            && (ctx.screen == Screen::Chat || ctx.screen == Screen::Dashboard)
            && ctx.chat_composing
        {
            app.chat.composer_delete_word_left();
            app.chat.update_autocomplete();
            i += input.consumed;
            continue;
        }

        if input.ctrl_delete
            && (ctx.screen == Screen::Chat || ctx.screen == Screen::Dashboard)
            && ctx.chat_composing
        {
            app.chat.composer_delete_word_right();
            app.chat.update_autocomplete();
            i += input.consumed;
            continue;
        }

        if let Some(key) = input.ctrl_arrow_key
            && (ctx.screen == Screen::Chat || ctx.screen == Screen::Dashboard)
            && ctx.chat_composing
            && !ctx.chat_ac_active
        {
            if key == b'C' {
                app.chat.composer_cursor_word_right();
            } else {
                app.chat.composer_cursor_word_left();
            }
            i += input.consumed;
            continue;
        }

        if let Some(key) = input.arrow_key {
            // Route arrows to composer cursor when composing
            if (ctx.screen == Screen::Chat || ctx.screen == Screen::Dashboard)
                && ctx.chat_composing
                && !ctx.chat_ac_active
                && matches!(key, b'A' | b'B' | b'C' | b'D')
            {
                match key {
                    b'C' => app.chat.composer_cursor_right(),
                    b'D' => app.chat.composer_cursor_left(),
                    b'A' => app.chat.composer_cursor_up(),
                    b'B' => app.chat.composer_cursor_down(),
                    _ => {}
                }
                i += input.consumed;
                continue;
            }

            if ctx.blocks_arrow_sequence() {
                i += input.consumed;
                continue;
            }

            // Always consume a decoded arrow sequence. Some screens intentionally
            // return false for blocked moves; letting the raw ESC byte fall
            // through would incorrectly trigger global/game escape handling.
            let _ = handle_arrow_for_screen(app, ctx.screen, key);
            i += input.consumed;
            continue;
        }

        if handle_modal_input(app, ctx, input.byte) {
            i += input.consumed;
            continue;
        }

        if handle_global_key(app, ctx, input.byte) {
            app.chat.clear_message_selection();
            i += input.consumed;
            continue;
        }

        dispatch_screen_key(app, ctx.screen, input.byte);
        i += input.consumed;
    }
}

fn handle_bracketed_paste(app: &mut App, pasted: &[u8]) {
    let ctx = InputContext::from_app(app);
    match paste_target(ctx) {
        PasteTarget::ChatComposer => {
            insert_pasted_text(pasted, |ch| app.chat.composer_push(ch));
            app.chat.update_autocomplete();
        }
        PasteTarget::NewsComposer => {
            insert_pasted_text(pasted, |ch| app.chat.news.composer_push(ch));
        }
        PasteTarget::None => {}
    }
}

fn paste_target(ctx: InputContext) -> PasteTarget {
    if (ctx.screen == Screen::Dashboard || ctx.screen == Screen::Chat) && ctx.chat_composing {
        PasteTarget::ChatComposer
    } else if ctx.screen == Screen::Chat && ctx.news_composing {
        PasteTarget::NewsComposer
    } else {
        PasteTarget::None
    }
}

fn insert_pasted_text(pasted: &[u8], mut push: impl FnMut(char)) {
    // Strip any residual bracketed-paste markers. If a paste arrives split
    // across reads, the outer parser may miss the ESC[200~ / ESC[201~ envelope
    // and we end up seeing the markers inline. ESC itself gets filtered as a
    // control char below, but the literal `[200~` / `[201~` would otherwise
    // survive as printable text in the composer.
    let cleaned = strip_paste_markers(pasted);
    let normalized = String::from_utf8_lossy(&cleaned).replace("\r\n", "\n");
    let normalized = normalized.replace('\r', "\n");
    for ch in normalized.chars() {
        if ch == '\n' || (!ch.is_control() && ch != '\u{7f}') {
            push(ch);
        }
    }
}

fn strip_paste_markers(input: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(input.len());
    let mut i = 0;
    while i < input.len() {
        if input[i..].starts_with(b"\x1b[200~") || input[i..].starts_with(b"\x1b[201~") {
            i += 6;
            continue;
        }
        if input[i..].starts_with(b"[200~") || input[i..].starts_with(b"[201~") {
            i += 5;
            continue;
        }
        out.push(input[i]);
        i += 1;
    }
    out
}

/// Remove any bracketed-paste marker residue from a string. Used when a URL
/// is about to be copied to the clipboard, so stored data that was polluted
/// before the input-side fix still gets cleaned up at copy time.
pub fn sanitize_paste_markers(s: &str) -> String {
    String::from_utf8_lossy(&strip_paste_markers(s.as_bytes())).into_owned()
}

fn handle_scroll_for_screen(app: &mut App, screen: Screen, delta: isize) {
    match screen {
        Screen::Dashboard => {
            app.chat.select_dashboard_message(delta);
        }
        Screen::Chat => chat::input::handle_scroll(app, delta),
        _ => {}
    }
}

fn handle_arrow_for_screen(app: &mut App, screen: Screen, key: u8) -> bool {
    // Route arrows to autocomplete when active
    if (screen == Screen::Chat || screen == Screen::Dashboard)
        && app.chat.is_composing()
        && app.chat.is_autocomplete_active()
    {
        chat::input::handle_autocomplete_arrow(app, key);
        return true;
    }

    match screen {
        Screen::Chat => {
            let _ = chat::input::handle_arrow(app, key);
            true
        }
        Screen::Dashboard => dashboard::input::handle_arrow(app, key),
        Screen::Profile => profile::input::handle_arrow(app, key),
        Screen::Games => crate::app::games::input::handle_arrow(app, key),
    }
}

fn handle_modal_input(app: &mut App, ctx: InputContext, byte: u8) -> bool {
    if (ctx.screen == Screen::Dashboard || ctx.screen == Screen::Chat) && ctx.chat_composing {
        chat::input::handle_compose_input(app, byte);
        return true;
    }

    if ctx.screen == Screen::Chat && ctx.news_composing {
        chat::news::input::handle_composer_input(app, byte);
        return true;
    }

    if ctx.screen == Screen::Profile && ctx.profile_composing {
        profile::input::handle_composer_input(app, byte);
        return true;
    }

    false
}

fn handle_global_key(app: &mut App, ctx: InputContext, byte: u8) -> bool {
    // ? opens help unless composing text
    if byte == b'?' && !ctx.chat_composing && !ctx.news_composing && !ctx.profile_composing {
        app.show_help = true;
        app.help_scroll = 0;
        return true;
    }

    if ctx.screen == Screen::Games
        && app.is_playing_game
        && !matches!(byte, 0x03 | b'm' | b'M' | b'+' | b'=' | b'-' | b'_')
    {
        return false;
    }

    match byte {
        b'q' | b'Q' | 0x03 => {
            app.running = false;
            true
        }
        b'm' | b'M' => {
            let label = app
                .paired_client_state()
                .map(|state| match state.client_kind {
                    crate::session::ClientKind::Unknown => "client".to_string(),
                    _ => state.client_kind.label().to_string(),
                })
                .unwrap_or_else(|| "client".to_string());
            if app.toggle_paired_client_mute() {
                app.banner = Some(crate::app::common::primitives::Banner::success(&format!(
                    "Sent mute toggle to paired {label}"
                )));
            } else {
                app.banner = Some(crate::app::common::primitives::Banner::error(
                    "No paired client session",
                ));
            }
            true
        }
        b'+' | b'=' => {
            let label = app
                .paired_client_state()
                .map(|state| match state.client_kind {
                    crate::session::ClientKind::Unknown => "client".to_string(),
                    _ => state.client_kind.label().to_string(),
                })
                .unwrap_or_else(|| "client".to_string());
            if app.paired_client_volume_up() {
                app.banner = Some(crate::app::common::primitives::Banner::success(&format!(
                    "Sent volume up to paired {label}"
                )));
            } else {
                app.banner = Some(crate::app::common::primitives::Banner::error(
                    "No paired client session",
                ));
            }
            true
        }
        b'-' | b'_' => {
            let label = app
                .paired_client_state()
                .map(|state| match state.client_kind {
                    crate::session::ClientKind::Unknown => "client".to_string(),
                    _ => state.client_kind.label().to_string(),
                })
                .unwrap_or_else(|| "client".to_string());
            if app.paired_client_volume_down() {
                app.banner = Some(crate::app::common::primitives::Banner::success(&format!(
                    "Sent volume down to paired {label}"
                )));
            } else {
                app.banner = Some(crate::app::common::primitives::Banner::error(
                    "No paired client session",
                ));
            }
            true
        }
        b'x' | b'X' if !ctx.chat_composing && !ctx.news_composing && !ctx.profile_composing => {
            if app.bonsai_state.cut() {
                app.banner = Some(crate::app::common::primitives::Banner::success(
                    "Bonsai pruned!",
                ));
            } else if !app.bonsai_state.is_alive {
                app.banner = Some(crate::app::common::primitives::Banner::error(
                    "Can't prune a dead tree",
                ));
            } else {
                app.banner = Some(crate::app::common::primitives::Banner::error(
                    "Not enough growth to prune",
                ));
            }
            true
        }
        b'w' | b'W' if !ctx.chat_composing && !ctx.news_composing && !ctx.profile_composing => {
            if !app.bonsai_state.is_alive {
                app.bonsai_state.respawn();
                app.banner = Some(crate::app::common::primitives::Banner::success(
                    "New seed planted!",
                ));
            } else if app.bonsai_state.water() {
                app.banner = Some(crate::app::common::primitives::Banner::success(
                    "Bonsai watered!",
                ));
            } else {
                app.banner = Some(crate::app::common::primitives::Banner::success(
                    "Already watered today",
                ));
            }
            true
        }
        b's' | b'S' if !ctx.chat_composing && !ctx.news_composing && !ctx.profile_composing => {
            let snippet = app.bonsai_state.share_snippet();
            app.pending_clipboard = Some(snippet);
            app.banner = Some(crate::app::common::primitives::Banner::success(
                "Bonsai copied to clipboard!",
            ));
            true
        }
        b'1' => {
            app.screen = Screen::Dashboard;
            true
        }
        b'2' => {
            app.chat.request_list();
            app.chat.sync_selection();
            app.chat.mark_selected_room_read();
            app.screen = Screen::Chat;
            true
        }
        b'3' => {
            app.screen = Screen::Games;
            true
        }
        b'4' => {
            app.screen = Screen::Profile;
            true
        }
        b'\t' => {
            app.screen = ctx.screen.next();
            match app.screen {
                Screen::Dashboard => {}
                Screen::Chat => {
                    app.chat.request_list();
                    app.chat.sync_selection();
                    app.chat.mark_selected_room_read();
                }
                Screen::Profile => {}
                Screen::Games => {}
            }
            true
        }
        b'p' | b'P' => {
            app.pending_clipboard = Some(app.connect_url.clone());
            app.web_chat_qr_url = Some(app.connect_url.clone());
            app.show_web_chat_qr = true;
            true
        }
        _ => false,
    }
}

fn dispatch_screen_key(app: &mut App, screen: Screen, byte: u8) {
    match screen {
        Screen::Dashboard => {
            dashboard::input::handle_key(app, byte);
        }
        Screen::Chat => {
            chat::input::handle_byte(app, byte);
        }
        Screen::Profile => {
            profile::input::handle_byte(app, byte);
        }
        Screen::Games => {
            crate::app::games::input::handle_key(app, byte);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_input_reads_arrow_sequence() {
        let decoded = decode_input(b"\x1B[A", 0);
        assert_eq!(decoded.consumed, 3);
        assert_eq!(decoded.arrow_key, Some(b'A'));
    }

    #[test]
    fn decode_input_reads_ss3_arrow_sequence() {
        let decoded = decode_input(b"\x1BOD", 0);
        assert_eq!(decoded.consumed, 3);
        assert_eq!(decoded.arrow_key, Some(b'D'));
    }

    #[test]
    fn decode_input_reads_single_byte() {
        let decoded = decode_input(b"x", 0);
        assert_eq!(decoded.byte, b'x');
        assert_eq!(decoded.consumed, 1);
        assert_eq!(decoded.arrow_key, None);
        assert_eq!(decoded.ctrl_arrow_key, None);
        assert!(!decoded.ctrl_backspace);
        assert!(!decoded.ctrl_delete);
    }

    #[test]
    fn blocks_arrow_when_chat_is_composing_on_dashboard() {
        let ctx = InputContext {
            screen: Screen::Dashboard,
            chat_composing: true,
            chat_ac_active: false,
            news_composing: false,
            profile_composing: false,
        };
        assert!(ctx.blocks_arrow_sequence());
    }

    #[test]
    fn blocks_arrow_when_chat_is_composing_on_chat_screen() {
        let ctx = InputContext {
            screen: Screen::Chat,
            chat_composing: true,
            chat_ac_active: false,
            news_composing: false,
            profile_composing: false,
        };
        assert!(ctx.blocks_arrow_sequence());
    }

    #[test]
    fn allows_arrow_when_idle() {
        let ctx = InputContext {
            screen: Screen::Dashboard,
            chat_composing: false,
            chat_ac_active: false,
            news_composing: false,
            profile_composing: false,
        };
        assert!(!ctx.blocks_arrow_sequence());
    }

    #[test]
    fn decode_input_parses_sgr_scroll_up() {
        // ESC [ < 64 ; 10 ; 5 M  (wheel up → reversed to scroll down)
        let data = b"\x1b[<64;10;5M";
        let decoded = decode_input(data, 0);
        assert_eq!(decoded.scroll, Some(1));
        assert_eq!(decoded.consumed, data.len());
        assert_eq!(decoded.arrow_key, None);
    }

    #[test]
    fn decode_input_parses_sgr_scroll_down() {
        // ESC [ < 65 ; 10 ; 5 M  (wheel down → reversed to scroll up)
        let data = b"\x1b[<65;10;5M";
        let decoded = decode_input(data, 0);
        assert_eq!(decoded.scroll, Some(-1));
        assert_eq!(decoded.consumed, data.len());
    }

    #[test]
    fn decode_input_parses_sgr_click_consumed() {
        // ESC [ < 0 ; 10 ; 5 M  (left click — consumed as scroll 0)
        let data = b"\x1b[<0;10;5M";
        let decoded = decode_input(data, 0);
        assert_eq!(decoded.scroll, Some(0));
        assert_eq!(decoded.consumed, data.len());
    }

    #[test]
    fn decode_input_sgr_release_event() {
        // ESC [ < 64 ; 10 ; 5 m  (wheel up release → reversed)
        let data = b"\x1b[<64;10;5m";
        let decoded = decode_input(data, 0);
        assert_eq!(decoded.scroll, Some(1));
        assert_eq!(decoded.consumed, data.len());
    }

    // --- alt-enter ---

    #[test]
    fn decode_input_detects_alt_enter_cr() {
        let data = b"\x1b\r";
        let decoded = decode_input(data, 0);
        assert!(decoded.alt_enter);
        assert_eq!(decoded.consumed, 2);
        assert_eq!(decoded.arrow_key, None);
        assert_eq!(decoded.scroll, None);
    }

    #[test]
    fn decode_input_detects_alt_enter_lf() {
        let data = b"\x1b\n";
        let decoded = decode_input(data, 0);
        assert!(decoded.alt_enter);
        assert_eq!(decoded.consumed, 2);
    }

    #[test]
    fn decode_input_esc_alone_is_not_alt_enter() {
        let data = b"\x1b";
        let decoded = decode_input(data, 0);
        assert!(!decoded.alt_enter);
        assert_eq!(decoded.consumed, 1);
    }

    #[test]
    fn decode_input_esc_bracket_is_not_alt_enter() {
        // ESC [ A is an arrow key, not alt-enter
        let data = b"\x1b[A";
        let decoded = decode_input(data, 0);
        assert!(!decoded.alt_enter);
        assert_eq!(decoded.arrow_key, Some(b'A'));
    }

    #[test]
    fn decode_input_parses_ctrl_right_arrow() {
        let data = b"\x1b[1;5C";
        let decoded = decode_input(data, 0);
        assert_eq!(decoded.ctrl_arrow_key, Some(b'C'));
        assert_eq!(decoded.consumed, data.len());
        assert_eq!(decoded.arrow_key, None);
    }

    #[test]
    fn decode_input_parses_short_ctrl_left_arrow() {
        let data = b"\x1b[5D";
        let decoded = decode_input(data, 0);
        assert_eq!(decoded.ctrl_arrow_key, Some(b'D'));
        assert_eq!(decoded.consumed, data.len());
    }

    #[test]
    fn decode_input_parses_ctrl_delete() {
        let data = b"\x1b[3;5~";
        let decoded = decode_input(data, 0);
        assert!(decoded.ctrl_delete);
        assert_eq!(decoded.consumed, data.len());
        assert_eq!(decoded.arrow_key, None);
        assert_eq!(decoded.ctrl_arrow_key, None);
    }

    #[test]
    fn decode_input_parses_ctrl_backspace_csi_u() {
        let data = b"\x1b[127;5u";
        let decoded = decode_input(data, 0);
        assert!(decoded.ctrl_backspace);
        assert_eq!(decoded.consumed, data.len());
        assert_eq!(decoded.arrow_key, None);
        assert_eq!(decoded.ctrl_arrow_key, None);
    }

    #[test]
    fn decode_input_parses_ctrl_backspace_alt_del() {
        let data = b"\x1b\x7f";
        let decoded = decode_input(data, 0);
        assert!(decoded.ctrl_backspace);
        assert_eq!(decoded.consumed, data.len());
    }

    #[test]
    fn parse_bracketed_paste_extracts_body() {
        let data = b"\x1b[200~hello\nworld\x1b[201~";
        let (consumed, body) = parse_bracketed_paste(data, 0).expect("paste parsed");
        assert_eq!(consumed, data.len());
        assert_eq!(body, b"hello\nworld");
    }

    #[test]
    fn parse_bracketed_paste_requires_end_marker() {
        let data = b"\x1b[200~hello\nworld";
        assert!(parse_bracketed_paste(data, 0).is_none());
    }

    #[test]
    fn paste_target_prefers_chat_composer() {
        let ctx = InputContext {
            screen: Screen::Chat,
            chat_composing: true,
            chat_ac_active: false,
            news_composing: true,
            profile_composing: false,
        };
        assert_eq!(paste_target(ctx), PasteTarget::ChatComposer);
    }

    #[test]
    fn paste_target_routes_to_news_composer() {
        let ctx = InputContext {
            screen: Screen::Chat,
            chat_composing: false,
            chat_ac_active: false,
            news_composing: true,
            profile_composing: false,
        };
        assert_eq!(paste_target(ctx), PasteTarget::NewsComposer);
    }

    #[test]
    fn insert_pasted_text_normalizes_newlines_and_filters_controls() {
        let mut out = String::new();
        insert_pasted_text(b"hello\r\nworld\x00\rok\x7f", |ch| out.push(ch));
        assert_eq!(out, "hello\nworld\nok");
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

    // --- autocomplete arrow routing ---

    #[test]
    fn allows_arrow_when_autocomplete_active() {
        let ctx = InputContext {
            screen: Screen::Chat,
            chat_composing: true,
            chat_ac_active: true,
            news_composing: false,
            profile_composing: false,
        };
        assert!(!ctx.blocks_arrow_sequence());
    }

    #[test]
    fn blocks_arrow_when_composing_without_autocomplete() {
        let ctx = InputContext {
            screen: Screen::Chat,
            chat_composing: true,
            chat_ac_active: false,
            news_composing: false,
            profile_composing: false,
        };
        assert!(ctx.blocks_arrow_sequence());
    }
}
