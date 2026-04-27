use super::{
    chat, dashboard, help_modal, icon_picker, profile_modal, quit_confirm, settings_modal,
    state::App,
};
use crate::app::common::primitives::Screen;
use crate::app::common::readline::ctrl_byte_to_input;
use ratatui::{
    layout::{Constraint, Layout, Rect},
    widgets::{Block, Borders},
};
use std::{mem, time::Duration};
use vte::{Params, Parser, Perform};

const PENDING_ESCAPE_FLUSH_DELAY: Duration = Duration::from_millis(40);

#[derive(Clone, Copy)]
struct InputContext {
    screen: Screen,
    chat_composing: bool,
    chat_ac_active: bool,
    news_composing: bool,
    showcase_composing: bool,
}

impl InputContext {
    fn from_app(app: &App) -> Self {
        Self {
            screen: app.screen,
            chat_composing: app.chat.is_composing(),
            chat_ac_active: app.chat.is_autocomplete_active(),
            news_composing: app.chat.news.composing(),
            showcase_composing: app.chat.showcase.composing(),
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
            || (self.screen == Screen::Chat && (self.news_composing || self.showcase_composing))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PasteTarget {
    None,
    ChatComposer,
    NewsComposer,
    ShowcaseComposer,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ParsedInput {
    Char(char),
    Byte(u8),
    Arrow(u8),
    CtrlArrow(u8),
    ShiftArrow(u8),
    /// Arrow with the Alt/Meta modifier (xterm `CSI 1;3 {A|B|C|D}`).
    /// Most terminals emit this for Option-Arrow on macOS or Alt-Arrow on
    /// Linux; kitty does in its default (non-kitty-keyboard) mode. Consumers
    /// treat `AltArrow` and `CtrlArrow` identically for word-jump bindings.
    AltArrow(u8),
    CtrlShiftArrow(u8),
    Delete,
    CtrlBackspace,
    CtrlDelete,
    Mouse(MouseEvent),
    BackTab,
    // Alt+Enter inserts a newline. `ESC`-prefixed control chords that would
    // otherwise wedge vte are pre-scanned before the parser sees them.
    AltEnter,
    // Alt+S submits without closing the composer. Picked over Ctrl+Enter
    // because tmux collapses Ctrl-modified Enter to bare `\r` unless the
    // kitty keyboard protocol is forwarded, which it isn't by default.
    AltS,
    AltC,
    Paste(Vec<u8>),
    PageUp,
    PageDown,
    End,
    Home,
    FocusGained,
    FocusLost,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MouseButton {
    Left,
    Middle,
    Right,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MouseModifiers {
    pub shift: bool,
    pub alt: bool,
    pub ctrl: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MouseEventKind {
    Down,
    Up,
    Drag,
    Moved,
    ScrollUp,
    ScrollDown,
    ScrollLeft,
    ScrollRight,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MouseEvent {
    pub kind: MouseEventKind,
    pub button: Option<MouseButton>,
    pub x: u16,
    pub y: u16,
    pub modifiers: MouseModifiers,
}

/// vte keeps pending escape state when `ESC` is followed by control bytes
/// such as `CR`, `LF`, or `BS`, so pre-scan those chords before feeding the
/// parser. This keeps Alt+Enter and Alt+Backspace from wedging subsequent
/// input when the chord is split across reads.
#[derive(Debug, Eq, PartialEq)]
enum EscapedInputChunk<'a> {
    Bytes(&'a [u8]),
    Event(ParsedInput),
}

fn escaped_input_event(byte: u8) -> Option<ParsedInput> {
    match byte {
        b'\r' | b'\n' => Some(ParsedInput::AltEnter),
        0x08 | 0x7F => Some(ParsedInput::CtrlBackspace),
        _ => None,
    }
}

fn split_escaped_input(data: &[u8]) -> Vec<EscapedInputChunk<'_>> {
    let mut out = Vec::new();
    let mut seg_start = 0;
    let mut i = 0;
    while i + 1 < data.len() {
        if data[i] == 0x1B
            && let Some(event) = escaped_input_event(data[i + 1])
        {
            if i > seg_start {
                out.push(EscapedInputChunk::Bytes(&data[seg_start..i]));
            }
            out.push(EscapedInputChunk::Event(event));
            i += 2;
            seg_start = i;
        } else {
            i += 1;
        }
    }
    if seg_start < data.len() {
        out.push(EscapedInputChunk::Bytes(&data[seg_start..]));
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
        if let Some(paste) = &mut self.paste {
            let mut buf = [0; 4];
            paste.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
        } else if ch.is_ascii_control() {
            // vte routes DEL (0x7F) through `print`, not `execute`. Keep it
            // on the control-byte path so Backspace in composers still works.
            self.events.push(ParsedInput::Byte(ch as u8));
        } else {
            self.events.push(ParsedInput::Char(ch));
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
                'H' => {
                    self.events.push(ParsedInput::Home);
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
                // xterm modifier param encoding: 2=Shift, 3=Alt, 4=Shift+Alt,
                // 5=Ctrl, 6=Ctrl+Shift, 7=Ctrl+Alt, 8=Ctrl+Shift+Alt. Some
                // terminals drop the leading "1;" (e.g. CSI 2 A instead of
                // CSI 1;2 A), so accept either placement.
                let modifier = match (p0, p1) {
                    (_, Some(m)) => Some(m),
                    (Some(m), None) if m > 1 => Some(m),
                    _ => None,
                };
                match modifier {
                    Some(2) => self.events.push(ParsedInput::ShiftArrow(key)),
                    Some(3) => self.events.push(ParsedInput::AltArrow(key)),
                    Some(5) => self.events.push(ParsedInput::CtrlArrow(key)),
                    Some(6) => self.events.push(ParsedInput::CtrlShiftArrow(key)),
                    _ => self.events.push(ParsedInput::Arrow(key)),
                }
            }
            '~' if p0 == Some(3) && p1 == Some(5) => {
                self.events.push(ParsedInput::CtrlDelete);
            }
            '~' if p0 == Some(3) => {
                self.events.push(ParsedInput::Delete);
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
            // Home: numeric forms `CSI 1~` / `CSI 7~` and bare `CSI H`.
            '~' if p0 == Some(1) || p0 == Some(7) => {
                self.events.push(ParsedInput::Home);
            }
            'H' if intermediates.is_empty() && p0.unwrap_or(0) <= 1 => {
                self.events.push(ParsedInput::Home);
            }
            // Kitty keyboard protocol: some terminals report Backspace as
            // codepoint 127, others as 8 (BS). Accept both for Ctrl+Backspace.
            'u' if (p0 == Some(127) || p0 == Some(8)) && p1 == Some(5) => {
                self.events.push(ParsedInput::CtrlBackspace);
            }
            // Shift+Tab: xterm `CSI Z`.
            'Z' if intermediates.is_empty() => {
                self.events.push(ParsedInput::BackTab);
            }
            'I' if intermediates.is_empty() => {
                self.events.push(ParsedInput::FocusGained);
            }
            'O' if intermediates.is_empty() => {
                self.events.push(ParsedInput::FocusLost);
            }
            'M' | 'm' if intermediates == [b'<'] && params.len() >= 3 => {
                let raw = p0.unwrap_or_default();
                let x = params.get(1).copied().unwrap_or(0);
                let y = params.get(2).copied().unwrap_or(0);
                let modifiers = MouseModifiers {
                    shift: raw & 4 != 0,
                    alt: raw & 8 != 0,
                    ctrl: raw & 16 != 0,
                };
                // SGR mouse encodes wheel directions in bit 6 plus the low
                // button bits: 64..67 => up/down/left/right.
                if raw & 64 != 0 {
                    let kind = match raw & 0b0100_0011 {
                        64 => MouseEventKind::ScrollUp,
                        65 => MouseEventKind::ScrollDown,
                        66 => MouseEventKind::ScrollLeft,
                        67 => MouseEventKind::ScrollRight,
                        _ => return,
                    };
                    self.events.push(ParsedInput::Mouse(MouseEvent {
                        kind,
                        button: None,
                        x,
                        y,
                        modifiers,
                    }));
                } else {
                    let motion = raw & 32 != 0;
                    let low = raw & 0b11;
                    // Low bits 0..=2 identify the button; 3 means "no button"
                    // (only meaningful with the motion bit set — mouse move
                    // without any button held, reported by ?1003h).
                    let button = match low {
                        0 => Some(MouseButton::Left),
                        1 => Some(MouseButton::Middle),
                        2 => Some(MouseButton::Right),
                        _ => None,
                    };
                    let kind = if motion {
                        if button.is_some() {
                            MouseEventKind::Drag
                        } else {
                            MouseEventKind::Moved
                        }
                    } else if action == 'M' {
                        MouseEventKind::Down
                    } else {
                        MouseEventKind::Up
                    };
                    self.events.push(ParsedInput::Mouse(MouseEvent {
                        kind,
                        button,
                        x,
                        y,
                        modifiers,
                    }));
                }
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
            return;
        }

        // Explicit Alt+printable chords we route across the app. Everything
        // else falls through and is intentionally swallowed as a lone Alt
        // modifier rather than leaking ESC + byte separately.
        if intermediates.is_empty() {
            match byte {
                b's' | b'S' => self.events.push(ParsedInput::AltS),
                b'c' | b'C' => self.events.push(ParsedInput::AltC),
                _ => {}
            }
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
        // Escape skips the rest of the intro animation
        if data.contains(&0x1B) {
            app.show_splash = false;
        }
        return;
    }

    // Web chat QR overlay: any key dismisses
    if app.show_web_chat_qr && !data.is_empty() {
        app.show_web_chat_qr = false;
        app.web_chat_qr_url = None;
        return;
    }

    // Split-across-reads `ESC` chords: previous read ended with a lone ESC
    // and this one begins with a control byte that should be treated as an
    // Alt chord instead of feeding a wedged parser.
    let mut start = 0;
    if app.pending_escape
        && let Some(event) = data.first().and_then(|byte| escaped_input_event(*byte))
    {
        app.pending_escape = false;
        app.pending_escape_started_at = None;
        app.vt_input.reset();
        handle_parsed_input(app, event);
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

    // Inline `ESC` control chords: pre-scan and split on the sequences that
    // would otherwise leave vte mid-escape. Each segment is fed to vte
    // independently and recognized chords are emitted directly.
    for chunk in split_escaped_input(&data[start..]) {
        match chunk {
            EscapedInputChunk::Bytes(bytes) => handle_vt_segment(app, bytes),
            EscapedInputChunk::Event(event) => handle_parsed_input(app, event),
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
    match overlay_input_action(event) {
        Some(OverlayInputAction::Close) => app.chat.close_overlay(),
        Some(OverlayInputAction::Scroll(delta)) => app.chat.scroll_overlay(delta),
        None => {}
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum OverlayInputAction {
    Close,
    Scroll(i16),
}

fn overlay_input_action(event: &ParsedInput) -> Option<OverlayInputAction> {
    match event {
        ParsedInput::Byte(b'q' | b'Q') | ParsedInput::Char('q' | 'Q') => {
            Some(OverlayInputAction::Close)
        }
        ParsedInput::Byte(b'j' | b'J') | ParsedInput::Char('j' | 'J') => {
            Some(OverlayInputAction::Scroll(1))
        }
        ParsedInput::Byte(b'k' | b'K') | ParsedInput::Char('k' | 'K') => {
            Some(OverlayInputAction::Scroll(-1))
        }
        ParsedInput::Arrow(b'B') => Some(OverlayInputAction::Scroll(1)),
        ParsedInput::Arrow(b'A') => Some(OverlayInputAction::Scroll(-1)),
        _ => None,
    }
}

fn handle_parsed_input(app: &mut App, event: ParsedInput) {
    if app.show_quit_confirm {
        quit_confirm::input::handle_input(app, event);
        return;
    }

    // Ctrl+O is a plain C0 control byte (0x0F) across terminals/tmux, so
    // treat it as the global "open settings" chord before any local routing.
    if matches!(event, ParsedInput::Byte(0x0F)) {
        open_settings_modal_globally(app);
        return;
    }

    // The quit confirm is topmost. Otherwise the existing modal stack owns input.
    if app.show_help {
        help_modal::input::handle_input(app, event);
        return;
    }

    if app.show_settings {
        settings_modal::input::handle_input(app, event);
        return;
    }

    if app.show_profile_modal {
        profile_modal::input::handle_input(app, event);
        return;
    }

    if app.show_bonsai_modal {
        crate::app::bonsai::modal_input::handle_input(app, event);
        return;
    }

    // Picker intercepts all input when open (ESC is handled via dispatch_escape).
    if app.icon_picker_open {
        handle_icon_picker_input(app, event);
        return;
    }

    let ctx = InputContext::from_app(app);

    if handle_dedicated_screen_input(app, ctx, &event) {
        return;
    }

    if (ctx.screen == Screen::Chat || ctx.screen == Screen::Dashboard) && app.chat.has_overlay() {
        handle_overlay_input(app, &event);
        return;
    }

    // Screen-specific rich event handlers get first crack at
    // Mouse/Home/modified-arrow events before the generic dispatch below.
    if ctx.screen == Screen::Games
        && app.is_playing_game
        && crate::app::games::input::handle_event(app, &event)
    {
        return;
    }
    if ctx.screen == Screen::Artboard && crate::app::artboard::page::handle_event(app, &event) {
        return;
    }
    if ctx.screen == Screen::Rooms && crate::app::rooms::input::handle_event(app, &event) {
        return;
    }

    match event {
        ParsedInput::FocusGained | ParsedInput::FocusLost => {}
        ParsedInput::Paste(pasted) => handle_bracketed_paste(app, &pasted),
        ParsedInput::AltEnter => {
            if (ctx.screen == Screen::Dashboard || ctx.screen == Screen::Chat) && ctx.chat_composing
            {
                app.chat.composer_push('\n');
                app.chat.update_autocomplete();
            } else if ctx.screen == Screen::Chat && ctx.showcase_composing {
                app.chat.showcase.field_newline();
            }
        }
        ParsedInput::AltS => {
            if (ctx.screen == Screen::Dashboard || ctx.screen == Screen::Chat) && ctx.chat_composing
            {
                let from_dashboard = ctx.screen == Screen::Dashboard;
                if let Some(b) = app.chat.submit_composer(true, from_dashboard) {
                    app.banner = Some(b);
                }
                chat::input::handle_post_submit_requests(app);
            }
        }
        ParsedInput::AltC => {}
        // Mouse events feed global hit tests first, then vertical wheel
        // fallback for screens that scroll outside richer local handlers.
        ParsedInput::Mouse(mouse) => {
            if handle_mouse_click(app, ctx.screen, mouse) {
                return;
            }
            if handle_notifications_hud_click(app, mouse) {
                return;
            }
            if let Some(delta) = mouse_scroll_delta(mouse) {
                handle_scroll_for_screen(app, ctx.screen, delta);
            }
        }
        ParsedInput::BackTab => {
            if ctx.screen == Screen::Chat && app.chat.room_jump_active {
                return;
            }
            if ctx.screen == Screen::Chat && ctx.showcase_composing {
                app.chat.showcase.cycle_field(false);
                return;
            }
            if (ctx.screen == Screen::Dashboard || ctx.screen == Screen::Chat) && ctx.chat_composing
            {
                return;
            }
            if ctx.screen == Screen::Chat && (ctx.news_composing || ctx.showcase_composing) {
                return;
            }
            if ctx.screen == Screen::Games && app.is_playing_game {
                return;
            }
            if artboard_blocks_global_page_switch(app, ctx.screen) {
                return;
            }
            reset_composers_for_page_change(app);
            app.set_screen(ctx.screen.prev());
            app.chat.clear_message_selection();
        }
        // Page keys mirror Ctrl-U / Ctrl-D. Signs follow the existing scheme:
        // positive = toward older/top, negative = toward newer/bottom. See
        // `app.chat.select_message` — its `delta` is in MESSAGES, not rows,
        // and chat messages wrap to ~3 rows each, so we divide terminal
        // height by 6 to get something that feels like half a visible page.
        ParsedInput::PageUp => {
            if ctx.screen == Screen::Chat && app.chat.room_jump_active {
                return;
            }
            let step = (app.size.1 / 6).max(1) as isize;
            handle_scroll_for_screen(app, ctx.screen, step);
        }
        ParsedInput::PageDown => {
            if ctx.screen == Screen::Chat && app.chat.room_jump_active {
                return;
            }
            let step = (app.size.1 / 6).max(1) as isize;
            handle_scroll_for_screen(app, ctx.screen, -step);
        }
        ParsedInput::End => {
            if ctx.screen == Screen::Chat && app.chat.room_jump_active {
                return;
            }
            handle_scroll_for_screen(app, ctx.screen, isize::MIN)
        }
        ParsedInput::Delete
            if (ctx.screen == Screen::Chat || ctx.screen == Screen::Dashboard)
                && ctx.chat_composing =>
        {
            app.chat.composer_delete_right();
            app.chat.update_autocomplete();
        }
        ParsedInput::Delete if ctx.screen == Screen::Chat && ctx.news_composing => {
            app.chat.news.composer_delete_right();
        }
        ParsedInput::CtrlBackspace
            if (ctx.screen == Screen::Chat || ctx.screen == Screen::Dashboard)
                && ctx.chat_composing =>
        {
            app.chat.composer_delete_word_left();
            app.chat.update_autocomplete();
        }
        ParsedInput::CtrlBackspace if ctx.screen == Screen::Chat && ctx.news_composing => {
            app.chat.news.composer_delete_word_left();
        }
        ParsedInput::Byte(0x17)
            if (ctx.screen == Screen::Chat || ctx.screen == Screen::Dashboard)
                && ctx.chat_composing =>
        {
            app.chat.composer_delete_word_left();
            app.chat.update_autocomplete();
        }
        ParsedInput::Byte(0x17) if ctx.screen == Screen::Chat && ctx.news_composing => {
            app.chat.news.composer_delete_word_left();
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
        ParsedInput::Byte(0x08) if ctx.screen == Screen::Chat && ctx.news_composing => {
            app.chat.news.composer_delete_word_left();
        }
        ParsedInput::CtrlDelete
            if (ctx.screen == Screen::Chat || ctx.screen == Screen::Dashboard)
                && ctx.chat_composing =>
        {
            app.chat.composer_delete_word_right();
            app.chat.update_autocomplete();
        }
        ParsedInput::CtrlDelete if ctx.screen == Screen::Chat && ctx.news_composing => {
            app.chat.news.composer_delete_word_right();
        }
        ParsedInput::CtrlArrow(key) | ParsedInput::AltArrow(key)
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
        ParsedInput::CtrlArrow(key) | ParsedInput::AltArrow(key)
            if ctx.screen == Screen::Chat && ctx.news_composing =>
        {
            if key == b'C' {
                app.chat.news.composer_cursor_word_right();
            } else if key == b'D' {
                app.chat.news.composer_cursor_word_left();
            }
        }
        ParsedInput::CtrlArrow(key) | ParsedInput::AltArrow(key)
            if ctx.screen == Screen::Chat && ctx.showcase_composing =>
        {
            let _ = chat::showcase::input::handle_arrow(app, key);
        }
        ParsedInput::Delete
        | ParsedInput::CtrlArrow(_)
        | ParsedInput::AltArrow(_)
        | ParsedInput::CtrlBackspace
        | ParsedInput::CtrlDelete => {}
        // Modified arrows are only bound on screens that opt in via the early
        // `handle_event` hook. Everywhere else they're inert.
        ParsedInput::ShiftArrow(_) | ParsedInput::CtrlShiftArrow(_) | ParsedInput::Home => {}
        ParsedInput::Arrow(key) => {
            if ctx.screen == Screen::Chat && app.chat.room_jump_active {
                let _ = chat::input::handle_arrow(app, key);
                return;
            }
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
            if ctx.screen == Screen::Chat && ctx.news_composing {
                match key {
                    b'C' => app.chat.news.composer_cursor_right(),
                    b'D' => app.chat.news.composer_cursor_left(),
                    _ => {}
                }
                return;
            }
            if ctx.screen == Screen::Chat && ctx.showcase_composing {
                let _ = chat::showcase::input::handle_arrow(app, key);
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
        // 0x1D (Ctrl+] / Ctrl+5 / raw GS) opens the chat icon picker on
        // chat-bearing screens, but active Artboard editing owns this
        // keystroke as the glyph-picker open key — let it fall through
        // to the byte dispatch below.
        ParsedInput::Byte(0x1D)
            if !((ctx.screen == Screen::Games && app.is_playing_game)
                || (ctx.screen == Screen::Artboard && app.artboard_interacting)) =>
        {
            try_open_icon_picker(app)
        }
        ParsedInput::Byte(byte) => handle_byte_event(app, ctx, byte),
        ParsedInput::Char(ch) => {
            if route_char_to_composer(app, ctx, ch) {
                return;
            }
            // Hotkey dispatchers are byte-oriented; non-ASCII can't match.
            if ch.is_ascii() {
                handle_byte_event(app, ctx, ch as u8);
            }
        }
    }
}

fn handle_dedicated_screen_input(app: &mut App, ctx: InputContext, event: &ParsedInput) -> bool {
    if ctx.screen == Screen::Games && app.is_playing_game {
        match event {
            ParsedInput::Byte(byte) => {
                crate::app::games::input::handle_key(app, *byte);
            }
            ParsedInput::Char(ch) if ch.is_ascii() => {
                crate::app::games::input::handle_key(app, *ch as u8);
            }
            ParsedInput::Arrow(key) => {
                crate::app::games::input::handle_arrow(app, *key);
            }
            _ => {}
        }
        return true;
    }

    if ctx.screen == Screen::Rooms && app.rooms_active_room.is_some() {
        let _ = crate::app::rooms::input::handle_event(app, event);
        return true;
    }

    false
}

fn route_char_to_composer(app: &mut App, ctx: InputContext, ch: char) -> bool {
    if (ctx.screen == Screen::Chat || ctx.screen == Screen::Dashboard) && ctx.chat_composing {
        chat::input::handle_compose_char(app, ch);
        return true;
    }
    if ctx.screen == Screen::Chat && ctx.showcase_composing {
        app.chat.showcase.field_insert_char(ch);
        return true;
    }
    false
}

fn handle_byte_event(app: &mut App, ctx: InputContext, byte: u8) {
    if ctx.screen == Screen::Chat && app.chat.room_jump_active {
        let _ = chat::input::handle_byte(app, byte);
        return;
    }

    if handle_modal_input(app, ctx, byte) {
        return;
    }

    if handle_global_key(app, ctx, byte) {
        app.chat.clear_message_selection();
        return;
    }

    dispatch_screen_key(app, ctx.screen, byte);
}

fn dispatch_escape(app: &mut App) {
    if app.show_quit_confirm {
        quit_confirm::input::handle_escape(app);
        return;
    }
    if app.show_help {
        help_modal::input::handle_escape(app);
        return;
    }
    if app.show_settings {
        settings_modal::input::handle_escape(app);
        return;
    }
    if app.show_profile_modal {
        profile_modal::input::handle_escape(app);
        return;
    }
    if app.show_bonsai_modal {
        crate::app::bonsai::modal_input::handle_escape(app);
        return;
    }
    if app.icon_picker_open {
        app.icon_picker_open = false;
        return;
    }
    let ctx = InputContext::from_app(app);
    if (ctx.screen == Screen::Chat || ctx.screen == Screen::Dashboard) && app.chat.room_jump_active
    {
        app.chat.cancel_room_jump();
        return;
    }
    if handle_modal_input(app, ctx, 0x1B) {
        return;
    }
    if (ctx.screen == Screen::Chat || ctx.screen == Screen::Dashboard)
        && app.chat.is_reaction_leader_active()
    {
        app.chat.cancel_reaction_leader();
        return;
    }
    if (ctx.screen == Screen::Chat || ctx.screen == Screen::Dashboard) && app.chat.has_overlay() {
        app.chat.close_overlay();
        return;
    }
    if ctx.screen == Screen::Artboard {
        let Some(state) = app.dartboard_state.as_ref() else {
            return;
        };
        if state.is_snapshot_browser_open() {
            dispatch_screen_key(app, ctx.screen, 0x1B);
            return;
        }
        if state.is_glyph_picker_open() || state.is_help_open() {
            dispatch_screen_key(app, ctx.screen, 0x1B);
            return;
        }
        if app.artboard_interacting {
            if crate::app::artboard::page::handle_key(app, 0x1B) {
                return;
            }
            app.deactivate_artboard_interaction();
            return;
        }
    }
    if ctx.screen == Screen::Games && app.is_playing_game {
        dispatch_screen_key(app, ctx.screen, 0x1B);
        return;
    }
    if ctx.screen == Screen::Rooms {
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
        PasteTarget::ShowcaseComposer => {
            insert_pasted_text(pasted, |ch| app.chat.showcase.field_insert_char(ch));
        }
        PasteTarget::None => {}
    }
}

fn paste_target(ctx: InputContext) -> PasteTarget {
    if (ctx.screen == Screen::Dashboard || ctx.screen == Screen::Chat) && ctx.chat_composing {
        PasteTarget::ChatComposer
    } else if ctx.screen == Screen::Chat && ctx.news_composing {
        PasteTarget::NewsComposer
    } else if ctx.screen == Screen::Chat && ctx.showcase_composing {
        PasteTarget::ShowcaseComposer
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
            if let Some(room_id) = app.dashboard_active_room_id() {
                chat::input::handle_scroll_in_room(app, room_id, delta);
            }
        }
        Screen::Chat => chat::input::handle_scroll(app, delta),
        Screen::Artboard => {}
        _ => {}
    }
}

fn handle_mouse_click(app: &mut App, screen: Screen, mouse: MouseEvent) -> bool {
    if mouse.kind != MouseEventKind::Down || mouse.button != Some(MouseButton::Left) {
        return false;
    }
    let Some(x) = mouse.x.checked_sub(1) else {
        return false;
    };
    let Some(y) = mouse.y.checked_sub(1) else {
        return false;
    };
    let content_area = app_content_area(app);

    match screen {
        Screen::Dashboard => {
            let Some(pins) = app.dashboard_strip_pins() else {
                return false;
            };
            let room_id = crate::app::dashboard::ui::favorites_strip_hit_test(
                content_area,
                app.profile_state.profile().show_dashboard_header,
                &pins,
                x,
                y,
            );
            if let Some(room_id) = room_id {
                app.select_dashboard_favorite_room(room_id);
                app.sync_visible_chat_room();
                return true;
            }
            false
        }
        Screen::Chat => {
            let slot = {
                let chat_badges = app.leaderboard.badges();
                let discover_view = crate::app::chat::discover::ui::DiscoverListView {
                    items: app.chat.discover.all_items(),
                    selected_index: app.chat.discover.selected_index(),
                };
                let notifications_view =
                    crate::app::chat::notifications::ui::NotificationListView {
                        items: app.chat.notifications.all_items(),
                        selected_index: app.chat.notifications.selected_index(),
                    };
                let mut rows_cache = crate::app::chat::ui::ChatRowsCache::default();
                let view = crate::app::chat::ui::ChatRenderInput {
                    news_selected: app.chat.news_selected,
                    news_unread_count: app.chat.news.unread_count(),
                    news_view: crate::app::chat::news::ui::ArticleListView {
                        articles: app.chat.news.all_articles(),
                        selected_index: app.chat.news.selected_index(),
                        marker_read_at: app.chat.news.marker_read_at(),
                    },
                    discover_selected: app.chat.discover_selected,
                    discover_view,
                    rows_cache: &mut rows_cache,
                    chat_rooms: &app.chat.rooms,
                    overlay: app.chat.overlay(),
                    usernames: app.chat.usernames(),
                    countries: app.chat.countries(),
                    badges: &chat_badges,
                    message_reactions: app.chat.message_reactions(),
                    unread_counts: &app.chat.unread_counts,
                    selected_room_id: app.chat.selected_room_id,
                    room_jump_active: app.chat.room_jump_active,
                    selected_message_id: app.chat.selected_message_id,
                    reaction_picker_active: app.chat.is_reaction_leader_active(),
                    highlighted_message_id: app.chat.highlighted_message_id,
                    composer: app.chat.composer(),
                    composing: app.chat.composing,
                    current_user_id: app.user_id,
                    cursor_visible: true,
                    mention_matches: &app.chat.mention_ac.matches,
                    mention_selected: app.chat.mention_ac.selected,
                    mention_active: app.chat.mention_ac.active,
                    reply_author: app.chat.reply_target().map(|reply| reply.author.as_str()),
                    is_editing: app.chat.edited_message_id.is_some(),
                    bonsai_glyphs: app.chat.bonsai_glyphs(),
                    news_composer: app.chat.news.composer(),
                    news_composing: app.chat.news.composing(),
                    news_processing: app.chat.news.processing(),
                    notifications_selected: app.chat.notifications_selected,
                    notifications_unread_count: app.chat.notifications.unread_count(),
                    notifications_view,
                    showcase_selected: app.chat.showcase_selected,
                    showcase_unread_count: app.chat.showcase.unread_count(),
                    showcase_view: crate::app::chat::showcase::ui::ShowcaseListView {
                        items: app.chat.showcase.all_items(),
                        selected_index: app.chat.showcase.selected_index(),
                        current_user_id: app.user_id,
                        is_admin: app.chat.showcase.is_admin(),
                        marker_read_at: app.chat.showcase.marker_read_at(),
                    },
                    showcase_state: Some(&app.chat.showcase),
                    showcase_composing: app.chat.showcase.composing(),
                };
                crate::app::chat::ui::room_list_hit_test(content_area, &view, x, y)
            };
            if let Some(slot) = slot {
                let changed = app.chat.select_room_slot(slot);
                if changed {
                    app.chat.reset_composer();
                    app.sync_visible_chat_room();
                    app.chat.request_list();
                }
                return true;
            }
            false
        }
        _ => false,
    }
}

fn handle_notifications_hud_click(app: &mut App, mouse: MouseEvent) -> bool {
    if mouse.kind != MouseEventKind::Down || mouse.button != Some(MouseButton::Left) {
        return false;
    }
    if app.show_splash {
        return false;
    }

    let unread = app.chat.notifications.unread_count();
    // SGR mouse coords are 1-indexed; the top border row is y=1.
    if unread == 0 || mouse.y != 1 {
        return false;
    }

    let noun = if unread == 1 { "mention" } else { "mentions" };
    let hud_width = format!(" {unread} unread {noun} ").len() as u16;
    if mouse.x < app.size.0.saturating_sub(hud_width) {
        return false;
    }

    app.set_screen(Screen::Chat);
    app.chat.select_notifications();
    true
}

fn app_content_area(app: &App) -> Rect {
    let area = Rect::new(0, 0, app.size.0, app.size.1);
    let inner = Block::default().borders(Borders::ALL).inner(area);
    if app.profile_state.profile().show_right_sidebar {
        Layout::horizontal([Constraint::Fill(1), Constraint::Length(24)]).split(inner)[0]
    } else {
        inner
    }
}

fn mouse_scroll_delta(mouse: MouseEvent) -> Option<isize> {
    match mouse.kind {
        MouseEventKind::ScrollUp => Some(1),
        MouseEventKind::ScrollDown => Some(-1),
        _ => None,
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
        Screen::Games => crate::app::games::input::handle_arrow(app, key),
        Screen::Rooms => crate::app::rooms::input::handle_arrow(app, key),
        Screen::Artboard => crate::app::artboard::page::handle_arrow(app, key),
    }
}

fn handle_modal_input(app: &mut App, ctx: InputContext, byte: u8) -> bool {
    if (ctx.screen == Screen::Dashboard || ctx.screen == Screen::Chat) && ctx.chat_composing {
        chat::input::handle_compose_input(
            app,
            byte,
            compose_room_switch_allowed(ctx.screen),
            ctx.screen == Screen::Dashboard,
        );
        return true;
    }

    if ctx.screen == Screen::Chat && ctx.news_composing {
        chat::news::input::handle_composer_input(app, byte);
        return true;
    }

    if ctx.screen == Screen::Chat && ctx.showcase_composing {
        chat::showcase::input::handle_composer_input(app, byte);
        return true;
    }

    false
}

fn compose_room_switch_allowed(screen: Screen) -> bool {
    screen == Screen::Chat
}

fn reset_composers_for_page_change(app: &mut App) {
    app.chat.reset_composer();
    app.chat.news.stop_composing();
    app.chat.showcase.stop_composing();
}

fn open_settings_modal_globally(app: &mut App) {
    app.show_help = false;
    app.show_profile_modal = false;
    app.show_bonsai_modal = false;
    app.show_web_chat_qr = false;
    app.show_quit_confirm = false;
    app.icon_picker_open = false;
    app.chat.close_overlay();
    app.chat.cancel_room_jump();
    app.settings_modal_state.open_from_profile(
        app.profile_state.profile(),
        app.chat.favorite_room_options(),
        crate::app::settings_modal::ui::MODAL_WIDTH,
    );
    app.show_settings = true;
}

pub(crate) fn trigger_global_quit(app: &mut App) {
    match quit_confirm::input::action_for(app.show_quit_confirm) {
        quit_confirm::input::QuitAction::OpenConfirm => {
            app.show_quit_confirm = true;
        }
        quit_confirm::input::QuitAction::QuitNow => {
            app.running = false;
        }
    }
}

fn handle_global_key(app: &mut App, ctx: InputContext, byte: u8) -> bool {
    let artboard_blocks_page_switch = artboard_blocks_global_page_switch(app, ctx.screen);

    // ? opens the global guide unless the current screen owns it.
    if byte == b'?'
        && !ctx.chat_composing
        && !ctx.news_composing
        && !ctx.showcase_composing
        && ctx.screen != Screen::Artboard
    {
        app.help_modal_state
            .open(crate::app::help_modal::data::HelpTopic::Overview);
        app.show_help = true;
        return true;
    }

    if matches!(byte, b'1' | b'2' | b'3' | b'4' | b'5' | b'6' | b'7' | b'8')
        && (ctx.screen == Screen::Dashboard || ctx.screen == Screen::Chat)
        && app.chat.is_reaction_leader_active()
    {
        return false;
    }

    // When the dashboard's `g` favorite-jump prefix is armed, digits 1-9
    // belong to the jump (`g3` → favorite slot 3), not the global screen
    // switcher. Let them fall through to dashboard::input::handle_key.
    if (b'1'..=b'9').contains(&byte)
        && ctx.screen == Screen::Dashboard
        && app.dashboard_g_prefix_armed
    {
        return false;
    }

    if ctx.screen == Screen::Games && app.is_playing_game {
        return false;
    }

    if ctx.screen == Screen::Artboard && app.artboard_interacting {
        return false;
    }

    match byte {
        b'q' | b'Q' => {
            if ctx.screen == Screen::Artboard
                && app
                    .dartboard_state
                    .as_ref()
                    .is_some_and(|state| state.is_snapshot_browser_open())
            {
                return false;
            }
            trigger_global_quit(app);
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
        b'w' | b'W' if !ctx.chat_composing && !ctx.news_composing && !ctx.showcase_composing => {
            app.show_help = false;
            app.show_profile_modal = false;
            app.show_settings = false;
            app.show_quit_confirm = false;
            app.show_bonsai_modal = true;
            true
        }
        b'1' if !artboard_blocks_page_switch => {
            reset_composers_for_page_change(app);
            app.set_screen(Screen::Dashboard);
            true
        }
        b'2' if !artboard_blocks_page_switch => {
            reset_composers_for_page_change(app);
            app.set_screen(Screen::Chat);
            true
        }
        b'3' if !artboard_blocks_page_switch => {
            reset_composers_for_page_change(app);
            app.set_screen(Screen::Games);
            true
        }
        b'4' if !artboard_blocks_page_switch => {
            reset_composers_for_page_change(app);
            app.set_screen(Screen::Rooms);
            true
        }
        b'5' if !artboard_blocks_page_switch => {
            reset_composers_for_page_change(app);
            app.set_screen(Screen::Artboard);
            true
        }
        b'\t' if !artboard_blocks_page_switch => {
            reset_composers_for_page_change(app);
            app.set_screen(ctx.screen.next());
            true
        }
        b'P' => {
            app.pending_clipboard = Some(app.connect_url.clone());
            app.web_chat_qr_url = Some(app.connect_url.clone());
            app.show_web_chat_qr = true;
            true
        }
        _ => false,
    }
}

fn artboard_blocks_global_page_switch(app: &App, screen: Screen) -> bool {
    if screen != Screen::Artboard {
        return false;
    }
    let Some(state) = app.dartboard_state.as_ref() else {
        return app.artboard_interacting;
    };
    app.artboard_interacting || state.is_help_open() || state.is_glyph_picker_open()
}

fn dispatch_screen_key(app: &mut App, screen: Screen, byte: u8) {
    match screen {
        Screen::Dashboard => {
            dashboard::input::handle_key(app, byte);
        }
        Screen::Chat => {
            chat::input::handle_byte(app, byte);
        }
        Screen::Games => {
            crate::app::games::input::handle_key(app, byte);
        }
        Screen::Rooms => {
            crate::app::rooms::input::handle_key(app, byte);
        }
        Screen::Artboard => {
            let _ = crate::app::artboard::page::handle_key(app, byte);
        }
    }
}

fn try_open_icon_picker(app: &mut App) {
    let ctx = InputContext::from_app(app);
    // Only the chat composer (dashboard and chat screens) can receive icons.
    if ctx.screen != Screen::Dashboard && ctx.screen != Screen::Chat {
        return;
    }
    if !ctx.chat_composing {
        // The dashboard card posts to the currently-active favorite (or
        // #general when no favorites are pinned). Pin it explicitly so
        // opening the icon picker from the dashboard doesn't inherit a
        // stale `selected_room_id` from the chat screen.
        if ctx.screen == Screen::Dashboard {
            if let Some(room_id) = app.dashboard_active_room_id() {
                app.chat.start_composing_in_room(room_id);
            }
        } else {
            app.chat.start_composing();
        }
    }
    if app.icon_catalog.is_none() {
        app.icon_catalog = Some(icon_picker::catalog::IconCatalogData::load());
    }
    app.icon_picker_state = icon_picker::IconPickerState::default();
    app.icon_picker_open = true;
}

fn handle_icon_picker_input(app: &mut App, event: ParsedInput) {
    match event {
        ParsedInput::Byte(b'\r') => apply_icon_selection(app, false),
        ParsedInput::AltEnter => apply_icon_selection(app, true),
        ParsedInput::Byte(b'\t') => app.icon_picker_state.next_tab(),
        ParsedInput::BackTab => app.icon_picker_state.prev_tab(),
        ParsedInput::Byte(0x7f) => app.icon_picker_state.search_delete_char(),
        ParsedInput::Delete => app.icon_picker_state.search_delete_next_char(),
        ParsedInput::CtrlBackspace | ParsedInput::Byte(0x08) => {
            app.icon_picker_state.search_delete_word_left()
        }
        ParsedInput::CtrlDelete => app.icon_picker_state.search_delete_word_right(),
        ParsedInput::Arrow(b'A') => picker_move_selection(app, -1),
        ParsedInput::Arrow(b'B') => picker_move_selection(app, 1),
        // Ctrl+K / Ctrl+J mirror vim-style up/down without stealing plain j/k
        // from the search box. These stay claimed for list nav and are NOT
        // forwarded to ratatui-textarea's keymap (which would kill-to-EOL /
        // insert-newline respectively).
        ParsedInput::Byte(0x0B) => picker_move_selection(app, -1),
        ParsedInput::Byte(0x0A) => picker_move_selection(app, 1),
        ParsedInput::Mouse(MouseEvent {
            kind: MouseEventKind::Down,
            button: Some(MouseButton::Left),
            x,
            y,
            ..
        }) => handle_icon_picker_click(app, x, y),
        ParsedInput::Mouse(mouse) => match mouse.kind {
            MouseEventKind::ScrollUp => picker_move_selection(app, -3),
            MouseEventKind::ScrollDown => picker_move_selection(app, 3),
            _ => {}
        },
        ParsedInput::Arrow(b'C') => app.icon_picker_state.search_cursor_right(),
        ParsedInput::Arrow(b'D') => app.icon_picker_state.search_cursor_left(),
        ParsedInput::CtrlArrow(b'C') | ParsedInput::AltArrow(b'C') => {
            app.icon_picker_state.search_cursor_word_right()
        }
        ParsedInput::CtrlArrow(b'D') | ParsedInput::AltArrow(b'D') => {
            app.icon_picker_state.search_cursor_word_left()
        }
        ParsedInput::PageUp => {
            let page = app.icon_picker_state.visible_height.get().max(1) as isize;
            picker_move_selection(app, -page);
        }
        ParsedInput::PageDown => {
            let page = app.icon_picker_state.visible_height.get().max(1) as isize;
            picker_move_selection(app, page);
        }
        // Ctrl+U / Ctrl+D half-page jumps mirror the chat viewport convention
        // and intentionally shadow ratatui-textarea's undo / delete-next-char.
        ParsedInput::Byte(0x15) => {
            let half = (app.icon_picker_state.visible_height.get() / 2).max(1) as isize;
            picker_move_selection(app, -half);
        }
        ParsedInput::Byte(0x04) => {
            let half = (app.icon_picker_state.visible_height.get() / 2).max(1) as isize;
            picker_move_selection(app, half);
        }
        // ^/ (^_) stays on the app-level undo path so `reset_selection()` fires.
        ParsedInput::Byte(0x1F) => app.icon_picker_state.search_undo(),
        ParsedInput::Char(ch) if !ch.is_control() => app.icon_picker_state.search_insert_char(ch),
        ParsedInput::Byte(byte) => {
            // Fallthrough: forward remaining Ctrl+<letter> chords (^A/^E/^F/
            // ^B/^Y/...) to ratatui-textarea's emacs keymap. The wrapper
            // resets icon-list selection whenever the query is modified.
            if let Some(input) = ctrl_byte_to_input(byte) {
                app.icon_picker_state.search_input(input);
            }
        }
        _ => {}
    }
}

fn picker_move_selection(app: &mut App, delta: isize) {
    let Some(catalog) = app.icon_catalog.as_ref() else {
        return;
    };
    icon_picker::picker::move_selection(&mut app.icon_picker_state, catalog, delta);
}

/// Handle a left-button press at SGR 1-based coordinates (x, y).
/// A click on a visible icon row selects it; a second click on the
/// same item within DOUBLE_CLICK_WINDOW_MS inserts it (keeps the picker open).
fn handle_icon_picker_click(app: &mut App, x: u16, y: u16) {
    let Some(col) = x.checked_sub(1) else {
        return;
    };
    let Some(row) = y.checked_sub(1) else {
        return;
    };

    if icon_picker::picker::click_tab(&mut app.icon_picker_state, col, row) {
        return;
    }

    let Some(catalog) = app.icon_catalog.as_ref() else {
        return;
    };
    if icon_picker::picker::click_list(&mut app.icon_picker_state, catalog, col, row) {
        apply_icon_selection(app, true);
    }
}

fn apply_icon_selection(app: &mut App, keep_open: bool) {
    let icon_str = {
        let Some(catalog) = app.icon_catalog.as_ref() else {
            app.icon_picker_open = false;
            return;
        };
        let Some(icon) = icon_picker::picker::selected_icon(&app.icon_picker_state, catalog) else {
            if !keep_open {
                app.icon_picker_open = false;
            }
            return;
        };
        if icon.is_empty() {
            return;
        }
        icon
    };

    if !keep_open {
        app.icon_picker_open = false;
    }

    let ctx = InputContext::from_app(app);
    if (ctx.screen == Screen::Dashboard || ctx.screen == Screen::Chat) && ctx.chat_composing {
        for ch in icon_str.chars() {
            app.chat.composer_push(ch);
        }
        app.chat.update_autocomplete();
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
            showcase_composing: false,
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
            showcase_composing: false,
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
            showcase_composing: false,
        };
        assert!(!ctx.blocks_arrow_sequence());
    }

    #[test]
    fn compose_room_switch_only_allowed_on_chat_screen() {
        assert!(compose_room_switch_allowed(Screen::Chat));
        assert!(!compose_room_switch_allowed(Screen::Dashboard));
        assert!(!compose_room_switch_allowed(Screen::Games));
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
            screen: Screen::Chat,
            chat_composing: true,
            chat_ac_active: false,
            news_composing: true,
            showcase_composing: false,
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
            showcase_composing: false,
        };
        assert_eq!(paste_target(ctx), PasteTarget::NewsComposer);
    }

    #[test]
    fn paste_target_routes_to_showcase_composer() {
        let ctx = InputContext {
            screen: Screen::Chat,
            chat_composing: false,
            chat_ac_active: false,
            news_composing: false,
            showcase_composing: true,
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

    // --- autocomplete arrow routing ---

    #[test]
    fn allows_arrow_when_autocomplete_active() {
        let ctx = InputContext {
            screen: Screen::Chat,
            chat_composing: true,
            chat_ac_active: true,
            news_composing: false,
            showcase_composing: false,
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
            showcase_composing: false,
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
}
