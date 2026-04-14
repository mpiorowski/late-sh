use super::{chat, dashboard, profile, state::App};
use crate::app::common::primitives::Screen;
use std::{mem, time::Duration};
use vte::{Params, Parser, Perform};

const PENDING_ESCAPE_FLUSH_DELAY: Duration = Duration::from_millis(40);

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

#[derive(Clone, Debug, Eq, PartialEq)]
enum ParsedInput {
    Byte(u8),
    Arrow(u8),
    CtrlArrow(u8),
    CtrlBackspace,
    CtrlDelete,
    Scroll(isize),
    AltEnter,
    Paste(Vec<u8>),
    PageUp,
    PageDown,
    End,
}

/// Walk `data` and split it on inline `ESC` + `CR`/`LF` pairs (Alt+Enter).
///
/// vte routes C0 control bytes through `execute` while the parser is in
/// escape state, which means `esc_dispatch` never sees `\r` or `\n` as the
/// final byte of an `ESC <byte>` sequence. Without this pre-scan, Alt+Enter
/// would be emitted as a plain Enter keypress and submit the composer.
#[derive(Debug, Eq, PartialEq)]
enum AltEnterChunk<'a> {
    Bytes(&'a [u8]),
    AltEnter,
}

fn split_alt_enter(data: &[u8]) -> Vec<AltEnterChunk<'_>> {
    let mut out = Vec::new();
    let mut seg_start = 0;
    let mut i = 0;
    while i + 1 < data.len() {
        if data[i] == 0x1B && matches!(data[i + 1], b'\r' | b'\n') {
            if i > seg_start {
                out.push(AltEnterChunk::Bytes(&data[seg_start..i]));
            }
            out.push(AltEnterChunk::AltEnter);
            i += 2;
            seg_start = i;
        } else {
            i += 1;
        }
    }
    if seg_start < data.len() {
        out.push(AltEnterChunk::Bytes(&data[seg_start..]));
    }
    out
}

pub(crate) struct VtInputParser {
    parser: Parser,
    collector: VtCollector,
}

impl Default for VtInputParser {
    fn default() -> Self {
        Self {
            parser: Parser::new(),
            collector: VtCollector::default(),
        }
    }
}

impl VtInputParser {
    fn feed(&mut self, data: &[u8]) -> Vec<ParsedInput> {
        self.parser.advance(&mut self.collector, data);
        mem::take(&mut self.collector.events)
    }

    fn reset(&mut self) {
        self.parser = Parser::new();
        self.collector.ss3_pending = false;
    }
}

#[derive(Default)]
struct VtCollector {
    events: Vec<ParsedInput>,
    paste: Option<Vec<u8>>,
    ss3_pending: bool,
}

impl VtCollector {
    fn push_byte(&mut self, byte: u8) {
        if let Some(paste) = &mut self.paste {
            paste.push(byte);
        } else {
            self.events.push(ParsedInput::Byte(byte));
        }
    }

    fn push_char(&mut self, ch: char) {
        let mut buf = [0; 4];
        let bytes = ch.encode_utf8(&mut buf).as_bytes();
        if let Some(paste) = &mut self.paste {
            paste.extend_from_slice(bytes);
        } else {
            for &byte in bytes {
                self.events.push(ParsedInput::Byte(byte));
            }
        }
    }

    fn finish_paste(&mut self) {
        if let Some(paste) = self.paste.take() {
            self.events.push(ParsedInput::Paste(paste));
        }
    }
}

impl Perform for VtCollector {
    fn print(&mut self, c: char) {
        if self.ss3_pending {
            self.ss3_pending = false;
            match c {
                'A' | 'B' | 'C' | 'D' => {
                    self.events.push(ParsedInput::Arrow(c as u8));
                    return;
                }
                'F' => {
                    self.events.push(ParsedInput::End);
                    return;
                }
                _ => {}
            }
        }

        self.push_char(c);
    }

    fn execute(&mut self, byte: u8) {
        self.push_byte(byte);
    }

    fn hook(&mut self, _: &Params, _: &[u8], _: bool, _: char) {}

    fn put(&mut self, _: u8) {}

    fn unhook(&mut self) {}

    fn osc_dispatch(&mut self, _: &[&[u8]], _: bool) {}

    fn csi_dispatch(&mut self, params: &Params, intermediates: &[u8], ignore: bool, action: char) {
        if ignore {
            return;
        }

        let params: Vec<u16> = params
            .iter()
            .map(|param| param.first().copied().unwrap_or(0))
            .collect();
        let p0 = params.first().copied();
        let p1 = params.get(1).copied();

        match action {
            '~' if p0 == Some(200) => {
                self.paste.get_or_insert_with(Vec::new);
            }
            '~' if p0 == Some(201) => {
                self.finish_paste();
            }
            'A' | 'B' | 'C' | 'D' => {
                let key = action as u8;
                if p1 == Some(5) || (p0 == Some(5) && p1.is_none()) {
                    self.events.push(ParsedInput::CtrlArrow(key));
                } else {
                    self.events.push(ParsedInput::Arrow(key));
                }
            }
            '~' if p0 == Some(3) && p1 == Some(5) => {
                self.events.push(ParsedInput::CtrlDelete);
            }
            '~' if p0 == Some(8) && p1 == Some(5) => {
                self.events.push(ParsedInput::CtrlBackspace);
            }
            // PageUp / PageDown / End (numeric form: CSI n ~). rxvt/linux
            // console encode End as 4~; xterm uses 8~. Home is intentionally
            // not bound — jumping to the oldest message in a long-lived room
            // is rarely useful and the `End` / PageUp pair covers the real
            // "scroll to a specific position" need.
            '~' if p0 == Some(5) => self.events.push(ParsedInput::PageUp),
            '~' if p0 == Some(6) => self.events.push(ParsedInput::PageDown),
            '~' if p0 == Some(4) || p0 == Some(8) => self.events.push(ParsedInput::End),
            // xterm bare form: CSI F (no params, no intermediates).
            'F' if intermediates.is_empty() && p0.unwrap_or(0) <= 1 => {
                self.events.push(ParsedInput::End);
            }
            // Kitty keyboard protocol: some terminals report Backspace as
            // codepoint 127, others as 8 (BS). Accept both for Ctrl+Backspace.
            'u' if (p0 == Some(127) || p0 == Some(8)) && p1 == Some(5) => {
                self.events.push(ParsedInput::CtrlBackspace);
            }
            'M' | 'm' if intermediates == [b'<'] && params.len() >= 3 => {
                let scroll = match p0.unwrap_or_default() {
                    64 => 1,
                    65 => -1,
                    _ => return,
                };
                self.events.push(ParsedInput::Scroll(scroll));
            }
            _ => {}
        }
    }

    fn esc_dispatch(&mut self, intermediates: &[u8], ignore: bool, byte: u8) {
        if ignore {
            return;
        }

        if intermediates.is_empty() && byte == b'O' {
            self.ss3_pending = true;
        }

        // Alt+printable falls through and is intentionally ignored, so ESC does
        // not cancel a composer and the printable byte does not leak separately.
        // Alt+Enter (ESC + CR/LF) is NOT dispatched here: vte executes C0
        // control bytes via `execute` while staying in escape state, so it
        // never reaches esc_dispatch. It's pre-scanned in `handle()` instead.
    }
}

pub fn flush_pending_escape(app: &mut App) {
    if !app.pending_escape {
        return;
    }

    let Some(started_at) = app.pending_escape_started_at else {
        return;
    };

    if started_at.elapsed() < PENDING_ESCAPE_FLUSH_DELAY {
        return;
    }

    app.pending_escape = false;
    app.pending_escape_started_at = None;
    app.vt_input.reset();
    dispatch_escape(app);
}

pub fn handle(app: &mut App, data: &[u8]) {
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

    // Split-across-reads Alt+Enter: previous read ended with a lone ESC and
    // this one begins with CR/LF. vte would execute the CR/LF as a plain
    // Enter while still sitting in escape state, submitting the composer
    // instead of inserting a newline. Intercept here before anything else.
    let mut start = 0;
    if app.pending_escape && matches!(data.first(), Some(b'\r') | Some(b'\n')) {
        app.pending_escape = false;
        app.pending_escape_started_at = None;
        app.vt_input.reset();
        handle_parsed_input(app, ParsedInput::AltEnter);
        start = 1;
    }

    if app.pending_escape
        && let Some(started_at) = app.pending_escape_started_at
        && started_at.elapsed() >= PENDING_ESCAPE_FLUSH_DELAY
    {
        app.pending_escape = false;
        app.pending_escape_started_at = None;
        app.vt_input.reset();
        dispatch_escape(app);
    }

    // Inline Alt+Enter: pre-scan and split on ESC+CR/LF pairs. Each segment
    // is fed to vte independently and an AltEnter event is emitted at each
    // split point. See `split_alt_enter` for why this can't live in the
    // `Perform` impl.
    for chunk in split_alt_enter(&data[start..]) {
        match chunk {
            AltEnterChunk::Bytes(bytes) => handle_vt_segment(app, bytes),
            AltEnterChunk::AltEnter => handle_parsed_input(app, ParsedInput::AltEnter),
        }
    }

    if data.last() == Some(&0x1B) {
        app.pending_escape = true;
        app.pending_escape_started_at = Some(std::time::Instant::now());
    } else {
        app.pending_escape = false;
        app.pending_escape_started_at = None;
    }
}

fn handle_vt_segment(app: &mut App, data: &[u8]) {
    if data.is_empty() {
        return;
    }

    let events = app.vt_input.feed(data);
    for event in events {
        handle_parsed_input(app, event);
    }
}

fn handle_overlay_input(app: &mut App, event: &ParsedInput) {
    match event {
        ParsedInput::Byte(b'q' | b'Q') => app.chat.close_overlay(),
        ParsedInput::Byte(b'j' | b'J') => app.chat.scroll_overlay(1),
        ParsedInput::Byte(b'k' | b'K') => app.chat.scroll_overlay(-1),
        ParsedInput::Arrow(b'B') => app.chat.scroll_overlay(1),
        ParsedInput::Arrow(b'A') => app.chat.scroll_overlay(-1),
        _ => {}
    }
}

fn handle_parsed_input(app: &mut App, event: ParsedInput) {
    let ctx = InputContext::from_app(app);

    if (ctx.screen == Screen::Chat || ctx.screen == Screen::Dashboard) && app.chat.has_overlay() {
        handle_overlay_input(app, &event);
        return;
    }

    match event {
        ParsedInput::Paste(pasted) => handle_bracketed_paste(app, &pasted),
        ParsedInput::AltEnter => {
            if (ctx.screen == Screen::Dashboard || ctx.screen == Screen::Chat) && ctx.chat_composing
            {
                app.chat.composer_push('\n');
                app.chat.update_autocomplete();
            }
        }
        ParsedInput::Scroll(delta) => handle_scroll_for_screen(app, ctx.screen, delta),
        // Page keys mirror Ctrl-U / Ctrl-D. Signs follow the existing scheme:
        // positive = toward older/top, negative = toward newer/bottom. See
        // `app.chat.select_message` — its `delta` is in MESSAGES, not rows,
        // and chat messages wrap to ~3 rows each, so we divide terminal
        // height by 6 to get something that feels like half a visible page.
        ParsedInput::PageUp => {
            let step = (app.size.1 / 6).max(1) as isize;
            handle_scroll_for_screen(app, ctx.screen, step);
        }
        ParsedInput::PageDown => {
            let step = (app.size.1 / 6).max(1) as isize;
            handle_scroll_for_screen(app, ctx.screen, -step);
        }
        ParsedInput::End => handle_scroll_for_screen(app, ctx.screen, isize::MIN),
        ParsedInput::CtrlBackspace
            if (ctx.screen == Screen::Chat || ctx.screen == Screen::Dashboard)
                && ctx.chat_composing =>
        {
            app.chat.composer_delete_word_left();
            app.chat.update_autocomplete();
        }
        // Many terminals encode Ctrl+Backspace as raw BS (^H / 0x08) rather
        // than a distinct escape sequence. Treat that as delete-word-left in
        // the chat composer; plain Backspace continues to come through as DEL.
        ParsedInput::Byte(0x08)
            if (ctx.screen == Screen::Chat || ctx.screen == Screen::Dashboard)
                && ctx.chat_composing =>
        {
            app.chat.composer_delete_word_left();
            app.chat.update_autocomplete();
        }
        ParsedInput::CtrlDelete
            if (ctx.screen == Screen::Chat || ctx.screen == Screen::Dashboard)
                && ctx.chat_composing =>
        {
            app.chat.composer_delete_word_right();
            app.chat.update_autocomplete();
        }
        ParsedInput::CtrlArrow(key)
            if (ctx.screen == Screen::Chat || ctx.screen == Screen::Dashboard)
                && ctx.chat_composing
                && !ctx.chat_ac_active =>
        {
            if key == b'C' {
                app.chat.composer_cursor_word_right();
            } else {
                app.chat.composer_cursor_word_left();
            }
        }
        ParsedInput::CtrlArrow(_) | ParsedInput::CtrlBackspace | ParsedInput::CtrlDelete => {}
        ParsedInput::Arrow(key) => {
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
                return;
            }

            if ctx.blocks_arrow_sequence() {
                return;
            }

            let _ = handle_arrow_for_screen(app, ctx.screen, key);
        }
        // Ctrl+J sends bare LF (0x0A). In the chat composer we alias it to
        // Alt+Enter so users have a one-handed way to insert a newline
        // without reaching for Alt. Plain Enter stays as bare CR (0x0D),
        // which still submits. News composer keeps its submit-on-LF
        // behavior since it only ever holds a single URL.
        ParsedInput::Byte(b'\n')
            if (ctx.screen == Screen::Dashboard || ctx.screen == Screen::Chat)
                && ctx.chat_composing =>
        {
            app.chat.composer_push('\n');
            app.chat.update_autocomplete();
        }
        ParsedInput::Byte(byte) => {
            if handle_modal_input(app, ctx, byte) {
                return;
            }

            if handle_global_key(app, ctx, byte) {
                app.chat.clear_message_selection();
                return;
            }

            dispatch_screen_key(app, ctx.screen, byte);
        }
    }
}

fn dispatch_escape(app: &mut App) {
    let ctx = InputContext::from_app(app);
    if handle_modal_input(app, ctx, 0x1B) {
        return;
    }
    if (ctx.screen == Screen::Chat || ctx.screen == Screen::Dashboard) && app.chat.has_overlay() {
        app.chat.close_overlay();
        return;
    }
    if ctx.screen == Screen::Games && app.is_playing_game {
        dispatch_screen_key(app, ctx.screen, 0x1B);
        return;
    }
    if (ctx.screen == Screen::Chat || ctx.screen == Screen::Dashboard)
        && app.chat.selected_message_id.is_some()
    {
        app.chat.clear_message_selection();
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
        Screen::Dashboard => app.chat.select_dashboard_message(delta),
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
    fn vt_parser_parses_scroll_events() {
        let mut parser = VtInputParser::default();
        assert_eq!(parser.feed(b"\x1b[<64;10;5M"), vec![ParsedInput::Scroll(1)]);
        assert_eq!(
            parser.feed(b"\x1b[<65;10;5m"),
            vec![ParsedInput::Scroll(-1)]
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
        assert_eq!(parser.feed(b"\x1b[3;5~"), vec![ParsedInput::CtrlDelete]);
        assert_eq!(
            parser.feed(b"\x1b[127;5u"),
            vec![ParsedInput::CtrlBackspace]
        );
        assert_eq!(parser.feed(b"\x1b[8;5u"), vec![ParsedInput::CtrlBackspace]);
        assert_eq!(parser.feed(b"\x1b[8;5~"), vec![ParsedInput::CtrlBackspace]);
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
    fn vt_parser_reset_clears_pending_escape_state() {
        let mut parser = VtInputParser::default();
        assert!(parser.feed(b"\x1b").is_empty());
        parser.reset();
        assert_eq!(parser.feed(b"j"), vec![ParsedInput::Byte(b'j')]);
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
    fn split_alt_enter_returns_plain_bytes_when_no_trigger() {
        let chunks = split_alt_enter(b"hello");
        assert_eq!(chunks, vec![AltEnterChunk::Bytes(b"hello")]);
    }

    #[test]
    fn split_alt_enter_splits_on_inline_escape_cr() {
        let chunks = split_alt_enter(b"ab\x1b\rcd");
        assert_eq!(
            chunks,
            vec![
                AltEnterChunk::Bytes(b"ab"),
                AltEnterChunk::AltEnter,
                AltEnterChunk::Bytes(b"cd"),
            ]
        );
    }

    #[test]
    fn split_alt_enter_handles_escape_lf_variant() {
        let chunks = split_alt_enter(b"\x1b\n");
        assert_eq!(chunks, vec![AltEnterChunk::AltEnter]);
    }

    #[test]
    fn split_alt_enter_handles_consecutive_triggers() {
        let chunks = split_alt_enter(b"\x1b\r\x1b\nx");
        assert_eq!(
            chunks,
            vec![
                AltEnterChunk::AltEnter,
                AltEnterChunk::AltEnter,
                AltEnterChunk::Bytes(b"x"),
            ]
        );
    }

    #[test]
    fn split_alt_enter_leaves_trailing_lone_escape_for_pending_logic() {
        // A bare ESC at the end of the buffer is left in the byte stream so
        // handle()'s trailing-ESC bookkeeping can set pending_escape.
        let chunks = split_alt_enter(b"ab\x1b");
        assert_eq!(chunks, vec![AltEnterChunk::Bytes(b"ab\x1b")]);
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
    fn vt_parser_emits_utf8_bytes_for_printable_non_ascii() {
        let mut parser = VtInputParser::default();
        assert_eq!(
            parser.feed("ł".as_bytes()),
            "ł".as_bytes()
                .iter()
                .copied()
                .map(ParsedInput::Byte)
                .collect::<Vec<_>>()
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
