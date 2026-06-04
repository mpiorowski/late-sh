use super::{
    audio::booth as audio_booth, chat, dashboard, help_modal, hub, icon_picker, mod_modal,
    profile_modal, quit_confirm, room_search_modal, settings_modal, state::App,
};
use crate::app::chat::state::RoomSection;
use crate::app::chat::ui::{ChatRowHit, ChatRowKind, HeaderTarget};
use crate::app::common::primitives::Screen;
use crate::app::common::readline::ctrl_byte_to_input;
use crate::app::directory::state::DirectoryTab;
use crate::app::files::terminal_image::TerminalImageProtocol;
use crate::usernames::UsernameLookup;
use ratatui::{
    layout::{Constraint, Layout, Rect},
    widgets::{Block, Borders},
};
use std::{mem, time::Duration};
use uuid::Uuid;
use vte::{Params, Parser, Perform};

const PENDING_ESCAPE_FLUSH_DELAY: Duration = Duration::from_millis(40);
const CTRL_G: u8 = 0x07;
const CTRL_O: u8 = 0x0F;

#[derive(Clone, Copy)]
struct InputContext {
    screen: Screen,
    directory_tab: DirectoryTab,
    chat_composing: bool,
    chat_ac_active: bool,
    feeds_processing: bool,
    news_composing: bool,
    showcase_composing: bool,
    work_composing: bool,
}

impl InputContext {
    fn from_app(app: &App) -> Self {
        Self {
            screen: app.screen,
            directory_tab: app.directory_state.tab,
            chat_composing: app.chat.is_composing(),
            chat_ac_active: app.chat.is_autocomplete_active(),
            feeds_processing: app.chat.feeds.processing(),
            news_composing: app.chat.news.composing(),
            showcase_composing: app.chat.showcase.composing(),
            work_composing: app.chat.work.composing(),
        }
    }

    fn blocks_arrow_sequence(self) -> bool {
        let chat_screen = is_chat_composer_context(self);
        // Allow arrows through when autocomplete is active
        if chat_screen && self.chat_ac_active {
            return false;
        }
        chat_screen
            || (self.screen == Screen::Dashboard
                && (self.feeds_processing
                    || self.news_composing
                    || self.showcase_composing
                    || self.work_composing))
            || (self.screen == Screen::Pinstar
                && matches!(
                    self.directory_tab,
                    DirectoryTab::Profiles | DirectoryTab::Projects
                )
                && (self.showcase_composing || self.work_composing))
    }
}

fn is_chat_composer_context(ctx: InputContext) -> bool {
    matches!(ctx.screen, Screen::Dashboard | Screen::Rooms) && ctx.chat_composing
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PasteTarget {
    None,
    ChatComposer,
    NewsComposer,
    ShowcaseComposer,
    WorkComposer,
    Pinstar,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ParsedInput {
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
    // Dropped on the floor in chat-composer contexts when the user has
    // the `keep_composer_focused` tweak enabled — Enter then owns send-
    // and-stay and the binding is explicitly cleared.
    AltS,
    AltC,
    Paste(Vec<u8>),
    PageUp,
    PageDown,
    End,
    Home,
    FocusGained,
    FocusLost,
    TerminalVersion(String),
    TerminalCapabilities(String),
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
    xtversion: Option<Vec<u8>>,
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

fn parse_iterm2_capabilities(params: &[&[u8]]) -> Option<String> {
    let value = if params.len() >= 2 && params[0] == b"1337" {
        params[1].strip_prefix(b"Capabilities=")?
    } else if params.len() == 1 {
        params[0].strip_prefix(b"1337;Capabilities=")?
    } else {
        return None;
    };
    std::str::from_utf8(value)
        .ok()
        .map(|value| value.to_string())
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

    fn hook(&mut self, _: &Params, intermediates: &[u8], ignore: bool, action: char) {
        if !ignore && intermediates == [b'>'] && action == '|' {
            self.xtversion = Some(Vec::new());
        }
    }

    fn put(&mut self, byte: u8) {
        if let Some(buf) = &mut self.xtversion {
            buf.push(byte);
        }
    }

    fn unhook(&mut self) {
        let Some(buf) = self.xtversion.take() else {
            return;
        };
        if let Ok(value) = String::from_utf8(buf) {
            self.events.push(ParsedInput::TerminalVersion(value));
        }
    }

    fn osc_dispatch(&mut self, params: &[&[u8]], _: bool) {
        if let Some(value) = parse_iterm2_capabilities(params) {
            self.events.push(ParsedInput::TerminalCapabilities(value));
        }
    }

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
            // console encode End as 4~; xterm uses 8~. Home is parsed below
            // for text inputs and surfaces that opt into it.
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
            // Kitty keyboard protocol for Ctrl+/.
            'u' if p0 == Some(b'/' as u16) && p1 == Some(5) => {
                self.events.push(ParsedInput::Byte(0x1F));
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
        // Do not process user input while splash screen is showing, but still
        // consume terminal capability replies sent by the startup probe.
        let events = app.vt_input.feed(data);
        let saw_terminal_reply = events.iter().any(|event| match event {
            ParsedInput::TerminalVersion(version) => {
                app.apply_xtversion_reply(version);
                true
            }
            ParsedInput::TerminalCapabilities(capabilities) => {
                app.apply_terminal_capabilities(capabilities);
                true
            }
            _ => false,
        });
        // Escape skips the rest of the intro animation. XTVERSION DCS replies
        // also begin with ESC, so avoid treating those as user cancellation.
        if !saw_terminal_reply && data.contains(&0x1B) {
            app.show_splash = false;
        }
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
    let close_on_any_key = app
        .chat
        .overlay()
        .is_some_and(|overlay| overlay.close_on_any_key);

    match overlay_input_action(event) {
        Some(OverlayInputAction::Close) => app.chat.close_overlay(),
        Some(OverlayInputAction::Scroll(delta)) => app.chat.scroll_overlay(delta),
        None if close_on_any_key && input_dismisses_key_modal(event) => {
            app.chat.close_overlay();
        }
        None => {}
    }
}

fn handle_news_modal_input(app: &mut App, event: &ParsedInput) {
    match event {
        ParsedInput::Byte(b'\r' | b'\n') => {
            if let Some(url) = app.chat.news_modal_url() {
                let cleaned = sanitize_paste_markers(url);
                app.pending_clipboard = Some(cleaned.trim().to_owned());
                app.banner = Some(crate::app::common::primitives::Banner::success(
                    "Link copied!",
                ));
            }
            app.chat.close_news_modal();
        }
        ParsedInput::Byte(b'n' | b'N') | ParsedInput::Char('n' | 'N') => {
            app.chat.jump_to_news_modal_article();
            app.set_screen(Screen::Dashboard);
        }
        ParsedInput::Byte(0x1B) => app.chat.close_news_modal(),
        _ => {}
    }
}

fn handle_image_modal_input(app: &mut App, event: &ParsedInput) {
    match event {
        ParsedInput::Byte(0x1B | b'q' | b'Q') | ParsedInput::Char('q' | 'Q') => {
            close_image_modal(app);
        }
        ParsedInput::Byte(b'\r' | b'\n' | b'c' | b'C') | ParsedInput::Char('c' | 'C') => {
            if let Some(url) = app.chat.image_modal().map(|modal| modal.url.clone()) {
                app.pending_clipboard = Some(url);
                app.banner = Some(crate::app::common::primitives::Banner::success(
                    "Image URL copied!",
                ));
            }
        }
        _ => {}
    }
}

fn close_image_modal(app: &mut App) {
    let needs_full_repaint = matches!(
        app.terminal_image_protocol,
        Some(TerminalImageProtocol::Iterm2 | TerminalImageProtocol::Sixel)
    );
    app.chat.close_image_modal();
    if needs_full_repaint {
        app.force_full_repaint();
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
    if let ParsedInput::TerminalVersion(version) = &event {
        app.apply_xtversion_reply(version);
        return;
    }
    if let ParsedInput::TerminalCapabilities(capabilities) = &event {
        app.apply_terminal_capabilities(capabilities);
        return;
    }

    if handle_reserved_global_chord(app, &event) {
        return;
    }

    if app.show_quit_confirm {
        quit_confirm::input::handle_input(app, event);
        return;
    }

    if is_room_search_shortcut(&event) {
        if app.room_search_modal_state.is_open() {
            app.room_search_modal_state.close();
        } else {
            open_room_search_modal_globally(app);
        }
        return;
    }

    if app.room_search_modal_state.is_open() {
        room_search_modal::input::handle_input(app, event);
        return;
    }

    if app.booth_modal_state.is_open() {
        audio_booth::input::handle_input(app, event);
        return;
    }

    if app.chat.has_news_modal() {
        handle_news_modal_input(app, &event);
        return;
    }

    if app.chat.has_image_modal() {
        handle_image_modal_input(app, &event);
        return;
    }

    if matches!(event, ParsedInput::Byte(0x11)) {
        toggle_aquarium_tray_globally(app);
        return;
    }

    // Reserved global chords and tray shortcuts have already had first claim.
    // Otherwise the existing modal stack owns input.
    if app.show_help {
        help_modal::input::handle_input(app, event);
        return;
    }

    if app.show_mod_modal {
        mod_modal::input::handle_input(app, event);
        return;
    }

    if app.show_hub_modal {
        hub::input::handle_input(app, event);
        return;
    }

    if app.show_ultimate_modal {
        crate::app::ultimates::handle_input(app, event);
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

    if app.show_bonsai_v2_modal {
        crate::app::bonsai_v2::modal_input::handle_input(app, event);
        return;
    }

    if app.show_bonsai_modal {
        crate::app::bonsai::modal_input::handle_input(app, event);
        return;
    }

    if app.show_cat_modal {
        crate::app::pet::modal_input::handle_input(app, event);
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

    if matches!(ctx.screen, Screen::Dashboard | Screen::Rooms) && app.chat.has_overlay() {
        handle_overlay_input(app, &event);
        return;
    }

    if ctx.screen == Screen::Dashboard && ctx.feeds_processing {
        return;
    }

    // Screen-specific rich event handlers get first crack at
    // Mouse/Home/modified-arrow events before the generic dispatch below.
    if ctx.screen == Screen::Arcade
        && app.is_playing_game
        && crate::app::arcade::input::handle_event(app, &event)
    {
        return;
    }
    if ctx.screen == Screen::Artboard && crate::app::artboard::page::handle_event(app, &event) {
        return;
    }
    if ctx.screen == Screen::Pinstar
        && (ctx.directory_tab == DirectoryTab::Pinstar || app.pinstar_state.is_some())
    {
        let content_area = app_content_area(app);
        if let Some(state) = &mut app.pinstar_state {
            let pinstar_area = ratatui::layout::Rect::new(
                content_area.x,
                content_area.y.saturating_add(1),
                content_area.width,
                content_area.height.saturating_sub(1),
            );
            if let ParsedInput::Mouse(mouse) = &event {
                let crossterm_mouse = crossterm::event::MouseEvent {
                    kind: match mouse.kind {
                        MouseEventKind::Down => {
                            crossterm::event::MouseEventKind::Down(match mouse.button {
                                Some(MouseButton::Left) => crossterm::event::MouseButton::Left,
                                Some(MouseButton::Middle) => crossterm::event::MouseButton::Middle,
                                Some(MouseButton::Right) => crossterm::event::MouseButton::Right,
                                _ => crossterm::event::MouseButton::Left,
                            })
                        }
                        MouseEventKind::Up => {
                            crossterm::event::MouseEventKind::Up(match mouse.button {
                                Some(MouseButton::Left) => crossterm::event::MouseButton::Left,
                                Some(MouseButton::Middle) => crossterm::event::MouseButton::Middle,
                                Some(MouseButton::Right) => crossterm::event::MouseButton::Right,
                                _ => crossterm::event::MouseButton::Left,
                            })
                        }
                        MouseEventKind::Drag => {
                            crossterm::event::MouseEventKind::Drag(match mouse.button {
                                Some(MouseButton::Left) => crossterm::event::MouseButton::Left,
                                Some(MouseButton::Middle) => crossterm::event::MouseButton::Middle,
                                Some(MouseButton::Right) => crossterm::event::MouseButton::Right,
                                _ => crossterm::event::MouseButton::Left,
                            })
                        }
                        MouseEventKind::Moved => crossterm::event::MouseEventKind::Moved,
                        MouseEventKind::ScrollUp => crossterm::event::MouseEventKind::ScrollUp,
                        MouseEventKind::ScrollDown => crossterm::event::MouseEventKind::ScrollDown,
                        MouseEventKind::ScrollLeft => crossterm::event::MouseEventKind::ScrollLeft,
                        MouseEventKind::ScrollRight => {
                            crossterm::event::MouseEventKind::ScrollRight
                        }
                    },
                    column: mouse.x.saturating_sub(1),
                    row: mouse.y.saturating_sub(1),
                    modifiers: crossterm::event::KeyModifiers::NONE,
                };
                crate::app::pinstar::input::handle_pinstar_mouse(
                    state,
                    crossterm_mouse,
                    pinstar_area,
                );
                return;
            }
        } else if matches!(&event, ParsedInput::Mouse(_)) {
            // No active diagram — handle mouse on the browser list
            let browser_area = ratatui::layout::Rect::new(
                content_area.x,
                content_area.y.saturating_add(1),
                content_area.width,
                content_area.height.saturating_sub(1),
            );
            if handle_pinstar_browser_mouse(app, &event, browser_area) {
                return;
            }
        }
    }
    if ctx.screen == Screen::Rooms
        && !ctx.chat_composing
        && crate::app::rooms::input::handle_event(app, &event)
    {
        return;
    }

    match event {
        ParsedInput::FocusGained
        | ParsedInput::FocusLost
        | ParsedInput::TerminalVersion(_)
        | ParsedInput::TerminalCapabilities(_) => {}
        ParsedInput::Paste(pasted) => handle_bracketed_paste(app, &pasted),
        ParsedInput::AltEnter => {
            if is_chat_composer_context(ctx) {
                app.chat.composer_push('\n');
                app.chat.update_autocomplete();
            } else if ctx.screen == Screen::Dashboard && ctx.showcase_composing {
                app.chat.showcase.field_newline();
            } else if ctx.screen == Screen::Dashboard && ctx.work_composing {
                app.chat.work.field_newline();
            }
        }
        ParsedInput::AltS => {
            if is_chat_composer_context(ctx) {
                if app.profile_state.profile().keep_composer_focused {
                    return;
                }
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
                if handle_mouse_scroll_over_screen(app, ctx.screen, mouse, delta) {
                    return;
                }
                handle_scroll_for_screen(app, ctx.screen, delta);
            }
        }
        ParsedInput::BackTab => {
            if room_jump_active_on_current_screen(app, ctx.screen) {
                return;
            }
            if ctx.screen == Screen::Dashboard && ctx.showcase_composing {
                app.chat.showcase.cycle_field(false);
                return;
            }
            if ctx.screen == Screen::Dashboard && ctx.work_composing {
                app.chat.work.cycle_field(false);
                return;
            }
            if is_chat_composer_context(ctx) {
                return;
            }
            if ctx.screen == Screen::Dashboard
                && (ctx.feeds_processing
                    || ctx.news_composing
                    || ctx.showcase_composing
                    || ctx.work_composing)
            {
                return;
            }
            if ctx.screen == Screen::Arcade && app.is_playing_game {
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
            if room_jump_active_on_current_screen(app, ctx.screen) {
                return;
            }
            let step = (app.size.1 / 6).max(1) as isize;
            handle_scroll_for_screen(app, ctx.screen, step);
        }
        ParsedInput::PageDown => {
            if room_jump_active_on_current_screen(app, ctx.screen) {
                return;
            }
            let step = (app.size.1 / 6).max(1) as isize;
            handle_scroll_for_screen(app, ctx.screen, -step);
        }
        ParsedInput::Home if is_chat_composer_context(ctx) => {
            app.chat.composer_cursor_home();
            app.chat.update_autocomplete();
        }
        ParsedInput::End if is_chat_composer_context(ctx) => {
            app.chat.composer_cursor_end();
            app.chat.update_autocomplete();
        }
        ParsedInput::Home if ctx.screen == Screen::Dashboard && ctx.news_composing => {
            app.chat.news.composer_cursor_home();
        }
        ParsedInput::End if ctx.screen == Screen::Dashboard && ctx.news_composing => {
            app.chat.news.composer_cursor_end();
        }
        ParsedInput::Home if ctx.screen == Screen::Dashboard && ctx.showcase_composing => {
            let field = app.chat.showcase.active_field();
            app.chat.showcase.field_input(
                field,
                ratatui_textarea::Input {
                    key: ratatui_textarea::Key::Home,
                    ..Default::default()
                },
            );
        }
        ParsedInput::End if ctx.screen == Screen::Dashboard && ctx.showcase_composing => {
            let field = app.chat.showcase.active_field();
            app.chat.showcase.field_input(
                field,
                ratatui_textarea::Input {
                    key: ratatui_textarea::Key::End,
                    ..Default::default()
                },
            );
        }
        ParsedInput::Home if ctx.screen == Screen::Dashboard && ctx.work_composing => {
            let field = app.chat.work.active_field();
            app.chat.work.field_input(
                field,
                ratatui_textarea::Input {
                    key: ratatui_textarea::Key::Home,
                    ..Default::default()
                },
            );
        }
        ParsedInput::End if ctx.screen == Screen::Dashboard && ctx.work_composing => {
            let field = app.chat.work.active_field();
            app.chat.work.field_input(
                field,
                ratatui_textarea::Input {
                    key: ratatui_textarea::Key::End,
                    ..Default::default()
                },
            );
        }
        ParsedInput::Delete if is_chat_composer_context(ctx) => {
            app.chat.composer_delete_right();
            app.chat.update_autocomplete();
        }
        ParsedInput::Delete if ctx.screen == Screen::Dashboard && ctx.news_composing => {
            app.chat.news.composer_delete_right();
        }
        ParsedInput::CtrlBackspace if is_chat_composer_context(ctx) => {
            app.chat.composer_delete_word_left();
            app.chat.update_autocomplete();
        }
        ParsedInput::CtrlBackspace if ctx.screen == Screen::Dashboard && ctx.news_composing => {
            app.chat.news.composer_delete_word_left();
        }
        ParsedInput::Byte(0x17) if is_chat_composer_context(ctx) => {
            app.chat.composer_delete_word_left();
            app.chat.update_autocomplete();
        }
        ParsedInput::Byte(0x17) if ctx.screen == Screen::Dashboard && ctx.news_composing => {
            app.chat.news.composer_delete_word_left();
        }
        // Many terminals encode Ctrl+Backspace as raw BS (^H / 0x08) rather
        // than a distinct escape sequence. Treat that as delete-word-left in
        // the chat composer; plain Backspace continues to come through as DEL.
        ParsedInput::Byte(0x08) if is_chat_composer_context(ctx) => {
            app.chat.composer_delete_word_left();
            app.chat.update_autocomplete();
        }
        ParsedInput::Byte(0x08) if ctx.screen == Screen::Dashboard && ctx.news_composing => {
            app.chat.news.composer_delete_word_left();
        }
        ParsedInput::CtrlDelete if is_chat_composer_context(ctx) => {
            app.chat.composer_delete_word_right();
            app.chat.update_autocomplete();
        }
        ParsedInput::CtrlDelete if ctx.screen == Screen::Dashboard && ctx.news_composing => {
            app.chat.news.composer_delete_word_right();
        }
        ParsedInput::CtrlArrow(key) | ParsedInput::AltArrow(key)
            if is_chat_composer_context(ctx) && !ctx.chat_ac_active =>
        {
            if key == b'C' {
                app.chat.composer_cursor_word_right();
            } else {
                app.chat.composer_cursor_word_left();
            }
        }
        ParsedInput::CtrlArrow(key) | ParsedInput::AltArrow(key)
            if ctx.screen == Screen::Dashboard && ctx.news_composing =>
        {
            if key == b'C' {
                app.chat.news.composer_cursor_word_right();
            } else if key == b'D' {
                app.chat.news.composer_cursor_word_left();
            }
        }
        ParsedInput::CtrlArrow(key) | ParsedInput::AltArrow(key)
            if ctx.screen == Screen::Dashboard && ctx.showcase_composing =>
        {
            let _ = chat::showcase::input::handle_arrow(app, key);
        }
        ParsedInput::CtrlArrow(key) | ParsedInput::AltArrow(key)
            if ctx.screen == Screen::Dashboard && ctx.work_composing =>
        {
            let _ = chat::work::input::handle_arrow(app, key);
        }
        ParsedInput::Delete
        | ParsedInput::CtrlArrow(_)
        | ParsedInput::AltArrow(_)
        | ParsedInput::CtrlBackspace
        | ParsedInput::CtrlDelete => {}
        // Modified arrows are only bound on screens that opt in via the early
        // `handle_event` hook. Everywhere else they're inert.
        ParsedInput::ShiftArrow(_)
        | ParsedInput::CtrlShiftArrow(_)
        | ParsedInput::Home
        | ParsedInput::End => {}
        ParsedInput::Arrow(key) => {
            if room_jump_active_on_current_screen(app, ctx.screen) {
                let _ = chat::input::handle_arrow(app, key);
                return;
            }
            if is_chat_composer_context(ctx)
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
            if ctx.screen == Screen::Dashboard && ctx.news_composing {
                match key {
                    b'C' => app.chat.news.composer_cursor_right(),
                    b'D' => app.chat.news.composer_cursor_left(),
                    _ => {}
                }
                return;
            }
            if ctx.screen == Screen::Dashboard && ctx.showcase_composing {
                let _ = chat::showcase::input::handle_arrow(app, key);
                return;
            }
            if ctx.screen == Screen::Dashboard && ctx.work_composing {
                let _ = chat::work::input::handle_arrow(app, key);
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
        ParsedInput::Byte(b'\n') if is_chat_composer_context(ctx) => {
            app.chat.composer_push('\n');
            app.chat.update_autocomplete();
        }
        // 0x1D (Ctrl+] / Ctrl+5 / raw GS) opens the chat icon picker on
        // chat-bearing screens, but active Artboard editing owns this
        // keystroke as the glyph-picker open key — let it fall through
        // to the byte dispatch below.
        ParsedInput::Byte(0x1D)
            if !((ctx.screen == Screen::Arcade && app.is_playing_game)
                || (ctx.screen == Screen::Artboard && app.artboard_interacting)
                || ctx.screen == Screen::Pinstar) =>
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
    if ctx.screen == Screen::DoorGames {
        if door_games_allows_global_navigation(event) {
            return false;
        }
        app.enter_lateania();
        let Some(state) = app.lateania_state.as_mut() else {
            return true;
        };
        match event {
            ParsedInput::Byte(byte) => {
                let action = crate::app::door::lateania::input::handle_key(state, *byte);
                if action == crate::app::door::lateania::input::InputAction::Leave {
                    app.set_screen(Screen::Dashboard);
                }
            }
            ParsedInput::Char(ch) if ch.is_ascii() => {
                let action = crate::app::door::lateania::input::handle_key(state, *ch as u8);
                if action == crate::app::door::lateania::input::InputAction::Leave {
                    app.set_screen(Screen::Dashboard);
                }
            }
            ParsedInput::Arrow(key) => {
                let _ = crate::app::door::lateania::input::handle_arrow(state, *key);
            }
            _ => {}
        }
        return true;
    }

    if ctx.screen == Screen::Arcade && app.is_playing_game {
        match event {
            ParsedInput::Byte(byte) => {
                crate::app::arcade::input::handle_key(app, *byte);
            }
            ParsedInput::Char(ch) if ch.is_ascii() => {
                crate::app::arcade::input::handle_key(app, *ch as u8);
            }
            ParsedInput::Arrow(key) => {
                crate::app::arcade::input::handle_arrow(app, *key);
            }
            _ => {}
        }
        return true;
    }

    if ctx.screen == Screen::Rooms && app.rooms_active_room.is_some() {
        if ctx.chat_composing {
            return false;
        }
        let _ = crate::app::rooms::input::handle_event(app, event);
        return true;
    }

    if ctx.screen == Screen::Pinstar {
        if app.pinstar_state.is_none() && ctx.directory_tab != DirectoryTab::Pinstar {
            return handle_directory_catalog_input(app, ctx, event);
        }
        if app.pinstar_state.is_none() {
            match event {
                ParsedInput::Byte(b'[') | ParsedInput::Char('[') => {
                    select_directory_tab(app, ctx.directory_tab.prev());
                    return true;
                }
                ParsedInput::Byte(b']') | ParsedInput::Char(']') => {
                    select_directory_tab(app, ctx.directory_tab.next());
                    return true;
                }
                _ => {}
            }
        }
        // If no active diagram, handle browser input
        if app.pinstar_state.is_none() {
            return handle_pinstar_browser_input(app, event);
        }
        // Otherwise handle active diagram input
        let mut area = app_content_area(app);
        area.y = area.y.saturating_add(1);
        area.height = area.height.saturating_sub(1);
        let mut handled = false;
        if let Some(state) = &mut app.pinstar_state {
            if state.show_invite_dialog
                && matches!(event, ParsedInput::Byte(0x0D) | ParsedInput::Byte(0x0A))
                && let Some(token) = &state.invite_token
            {
                app.pending_clipboard = Some(token.clone());
                app.banner = Some(crate::app::common::primitives::Banner::success(
                    "Invite link copied to clipboard!",
                ));
                return true;
            }

            match event {
                ParsedInput::Byte(byte) => {
                    let mut modifiers = crossterm::event::KeyModifiers::NONE;
                    let code = if *byte < 32
                        && *byte != 0x1B
                        && *byte != 0x09
                        && *byte != 0x0D
                        && *byte != 0x0A
                        && *byte != 0x08
                    {
                        modifiers |= crossterm::event::KeyModifiers::CONTROL;
                        crossterm::event::KeyCode::Char((*byte + 96) as char)
                    } else {
                        match *byte {
                            0x0D | 0x0A => crossterm::event::KeyCode::Enter,
                            0x09 => crossterm::event::KeyCode::Tab,
                            // 0x08 (BS/^H) = Ctrl+Backspace on terminals that
                            // emit raw bytes; 0x7F (DEL) = plain Backspace.
                            // Matches chat composer handling at line ~1086.
                            0x08 => {
                                modifiers |= crossterm::event::KeyModifiers::CONTROL;
                                crossterm::event::KeyCode::Backspace
                            }
                            0x7F => crossterm::event::KeyCode::Backspace,
                            0x1B => crossterm::event::KeyCode::Esc,
                            _ => crossterm::event::KeyCode::Char(*byte as char),
                        }
                    };
                    let key = crossterm::event::KeyEvent::new(code, modifiers);
                    handled = crate::app::pinstar::input::handle_pinstar_key(
                        state,
                        key,
                        area,
                        app.pinstar_registry.db(),
                    );
                }
                ParsedInput::Char(ch) => {
                    let mut modifiers = crossterm::event::KeyModifiers::NONE;
                    if ch.is_uppercase() {
                        modifiers |= crossterm::event::KeyModifiers::SHIFT;
                    }
                    let key = crossterm::event::KeyEvent::new(
                        crossterm::event::KeyCode::Char(*ch),
                        modifiers,
                    );
                    handled = crate::app::pinstar::input::handle_pinstar_key(
                        state,
                        key,
                        area,
                        app.pinstar_registry.db(),
                    );
                }
                ParsedInput::Arrow(key) => {
                    let code = match key {
                        b'A' => crossterm::event::KeyCode::Up,
                        b'B' => crossterm::event::KeyCode::Down,
                        b'C' => crossterm::event::KeyCode::Right,
                        b'D' => crossterm::event::KeyCode::Left,
                        _ => return false,
                    };
                    let key =
                        crossterm::event::KeyEvent::new(code, crossterm::event::KeyModifiers::NONE);
                    handled = crate::app::pinstar::input::handle_pinstar_key(
                        state,
                        key,
                        area,
                        app.pinstar_registry.db(),
                    );
                }
                ParsedInput::CtrlArrow(key) => {
                    let code = match key {
                        b'A' => crossterm::event::KeyCode::Up,
                        b'B' => crossterm::event::KeyCode::Down,
                        b'C' => crossterm::event::KeyCode::Right,
                        b'D' => crossterm::event::KeyCode::Left,
                        _ => return false,
                    };
                    let key = crossterm::event::KeyEvent::new(
                        code,
                        crossterm::event::KeyModifiers::CONTROL,
                    );
                    handled = crate::app::pinstar::input::handle_pinstar_key(
                        state,
                        key,
                        area,
                        app.pinstar_registry.db(),
                    );
                }
                ParsedInput::AltArrow(key) => {
                    let code = match key {
                        b'A' => crossterm::event::KeyCode::Up,
                        b'B' => crossterm::event::KeyCode::Down,
                        b'C' => crossterm::event::KeyCode::Right,
                        b'D' => crossterm::event::KeyCode::Left,
                        _ => return false,
                    };
                    let key =
                        crossterm::event::KeyEvent::new(code, crossterm::event::KeyModifiers::ALT);
                    handled = crate::app::pinstar::input::handle_pinstar_key(
                        state,
                        key,
                        area,
                        app.pinstar_registry.db(),
                    );
                }
                ParsedInput::ShiftArrow(key) => {
                    let code = match key {
                        b'A' => crossterm::event::KeyCode::Up,
                        b'B' => crossterm::event::KeyCode::Down,
                        b'C' => crossterm::event::KeyCode::Right,
                        b'D' => crossterm::event::KeyCode::Left,
                        _ => return false,
                    };
                    let key = crossterm::event::KeyEvent::new(
                        code,
                        crossterm::event::KeyModifiers::SHIFT,
                    );
                    handled = crate::app::pinstar::input::handle_pinstar_key(
                        state,
                        key,
                        area,
                        app.pinstar_registry.db(),
                    );
                }
                ParsedInput::CtrlShiftArrow(key) => {
                    let code = match key {
                        b'A' => crossterm::event::KeyCode::Up,
                        b'B' => crossterm::event::KeyCode::Down,
                        b'C' => crossterm::event::KeyCode::Right,
                        b'D' => crossterm::event::KeyCode::Left,
                        _ => return false,
                    };
                    let key = crossterm::event::KeyEvent::new(
                        code,
                        crossterm::event::KeyModifiers::CONTROL
                            | crossterm::event::KeyModifiers::SHIFT,
                    );
                    handled = crate::app::pinstar::input::handle_pinstar_key(
                        state,
                        key,
                        area,
                        app.pinstar_registry.db(),
                    );
                }
                ParsedInput::CtrlBackspace => {
                    let key = crossterm::event::KeyEvent::new(
                        crossterm::event::KeyCode::Backspace,
                        crossterm::event::KeyModifiers::CONTROL,
                    );
                    handled = crate::app::pinstar::input::handle_pinstar_key(
                        state,
                        key,
                        area,
                        app.pinstar_registry.db(),
                    );
                }
                ParsedInput::CtrlDelete => {
                    let key = crossterm::event::KeyEvent::new(
                        crossterm::event::KeyCode::Delete,
                        crossterm::event::KeyModifiers::CONTROL,
                    );
                    handled = crate::app::pinstar::input::handle_pinstar_key(
                        state,
                        key,
                        area,
                        app.pinstar_registry.db(),
                    );
                }
                ParsedInput::Delete => {
                    let key = crossterm::event::KeyEvent::new(
                        crossterm::event::KeyCode::Delete,
                        crossterm::event::KeyModifiers::NONE,
                    );
                    handled = crate::app::pinstar::input::handle_pinstar_key(
                        state,
                        key,
                        area,
                        app.pinstar_registry.db(),
                    );
                }
                ParsedInput::Home => {
                    let key = crossterm::event::KeyEvent::new(
                        crossterm::event::KeyCode::Home,
                        crossterm::event::KeyModifiers::NONE,
                    );
                    handled = crate::app::pinstar::input::handle_pinstar_key(
                        state,
                        key,
                        area,
                        app.pinstar_registry.db(),
                    );
                }
                ParsedInput::End => {
                    let key = crossterm::event::KeyEvent::new(
                        crossterm::event::KeyCode::End,
                        crossterm::event::KeyModifiers::NONE,
                    );
                    handled = crate::app::pinstar::input::handle_pinstar_key(
                        state,
                        key,
                        area,
                        app.pinstar_registry.db(),
                    );
                }
                ParsedInput::PageUp => {
                    let key = crossterm::event::KeyEvent::new(
                        crossterm::event::KeyCode::PageUp,
                        crossterm::event::KeyModifiers::NONE,
                    );
                    handled = crate::app::pinstar::input::handle_pinstar_key(
                        state,
                        key,
                        area,
                        app.pinstar_registry.db(),
                    );
                }
                ParsedInput::PageDown => {
                    let key = crossterm::event::KeyEvent::new(
                        crossterm::event::KeyCode::PageDown,
                        crossterm::event::KeyModifiers::NONE,
                    );
                    handled = crate::app::pinstar::input::handle_pinstar_key(
                        state,
                        key,
                        area,
                        app.pinstar_registry.db(),
                    );
                }
                _ => {}
            }
        }

        if !handled && app.pinstar_state.is_some() {
            if matches!(
                event,
                ParsedInput::Byte(0x1B) | ParsedInput::Byte(b'q') | ParsedInput::Char('q')
            ) {
                app.pinstar_state = None;
                app.refresh_pinstar_browser();
                return true;
            }

            // Pinstar normally owns '?'. If the active handler declined it,
            // let the generic path ignore it without treating it as back/quit.
            if matches!(event, ParsedInput::Byte(b'?') | ParsedInput::Char('?')) {
                return false;
            }

            // Do not return true for Mouse events here, let them fall through to the
            // specialized Pinstar mouse handler in handle_parsed_input.
            if matches!(event, ParsedInput::Mouse(_)) {
                return false;
            }

            return false;
        }
        return handled;
    }

    false
}

fn door_games_allows_global_navigation(event: &ParsedInput) -> bool {
    match event {
        ParsedInput::BackTab => true,
        ParsedInput::Byte(b'\t' | b'1'..=b'6') => true,
        ParsedInput::Char('1'..='6') => true,
        _ => false,
    }
}

fn handle_directory_catalog_input(app: &mut App, ctx: InputContext, event: &ParsedInput) -> bool {
    match event {
        ParsedInput::AltEnter => {
            match ctx.directory_tab {
                DirectoryTab::Profiles if app.chat.work.composing() => {
                    app.chat.work.field_newline()
                }
                DirectoryTab::Projects if app.chat.showcase.composing() => {
                    app.chat.showcase.field_newline();
                }
                _ => {}
            }
            true
        }
        ParsedInput::Arrow(key) => match ctx.directory_tab {
            DirectoryTab::Profiles => crate::app::chat::work::input::handle_arrow(app, *key),
            DirectoryTab::Projects => crate::app::chat::showcase::input::handle_arrow(app, *key),
            DirectoryTab::Pinstar => false,
        },
        ParsedInput::PageUp => {
            move_directory_selection(app, ctx.directory_tab, -6);
            true
        }
        ParsedInput::PageDown => {
            move_directory_selection(app, ctx.directory_tab, 6);
            true
        }
        ParsedInput::Byte(byte) => {
            if *byte == b'[' {
                select_directory_tab(app, ctx.directory_tab.prev());
                return true;
            }
            if *byte == b']' {
                select_directory_tab(app, ctx.directory_tab.next());
                return true;
            }
            match ctx.directory_tab {
                DirectoryTab::Profiles => {
                    if app.chat.work.composing() {
                        crate::app::chat::work::input::handle_composer_input(app, *byte);
                        true
                    } else {
                        crate::app::chat::work::input::handle_byte(app, *byte)
                    }
                }
                DirectoryTab::Projects => {
                    if app.chat.showcase.composing() {
                        crate::app::chat::showcase::input::handle_composer_input(app, *byte);
                        true
                    } else {
                        crate::app::chat::showcase::input::handle_byte(app, *byte)
                    }
                }
                DirectoryTab::Pinstar => false,
            }
        }
        ParsedInput::Char(ch) => {
            if *ch == '[' {
                select_directory_tab(app, ctx.directory_tab.prev());
                return true;
            }
            if *ch == ']' {
                select_directory_tab(app, ctx.directory_tab.next());
                return true;
            }
            if route_directory_char_to_composer(app, ctx, *ch) {
                return true;
            }
            if ch.is_ascii() {
                let byte = *ch as u8;
                match ctx.directory_tab {
                    DirectoryTab::Profiles => crate::app::chat::work::input::handle_byte(app, byte),
                    DirectoryTab::Projects => {
                        crate::app::chat::showcase::input::handle_byte(app, byte)
                    }
                    DirectoryTab::Pinstar => false,
                }
            } else {
                false
            }
        }
        _ => false,
    }
}

fn route_directory_char_to_composer(app: &mut App, ctx: InputContext, ch: char) -> bool {
    match ctx.directory_tab {
        DirectoryTab::Profiles if app.chat.work.composing() => {
            app.chat.work.field_insert_char(ch);
            true
        }
        DirectoryTab::Projects if app.chat.showcase.composing() => {
            app.chat.showcase.field_insert_char(ch);
            true
        }
        _ => false,
    }
}

fn move_directory_selection(app: &mut App, tab: DirectoryTab, delta: isize) {
    match tab {
        DirectoryTab::Profiles => app.chat.work.move_selection(delta),
        DirectoryTab::Projects => app.chat.showcase.move_selection(delta),
        DirectoryTab::Pinstar => {}
    }
}

fn select_directory_tab(app: &mut App, tab: DirectoryTab) {
    if app.directory_state.tab == tab {
        return;
    }
    app.chat.showcase.stop_composing();
    app.chat.work.stop_composing();
    app.directory_state.select(tab);
    match tab {
        DirectoryTab::Profiles => {
            app.chat.work.list();
            app.chat.work.mark_read();
        }
        DirectoryTab::Projects => {
            app.chat.showcase.list();
            app.chat.showcase.mark_read();
        }
        DirectoryTab::Pinstar => {
            app.refresh_pinstar_browser();
        }
    }
}

fn route_char_to_composer(app: &mut App, ctx: InputContext, ch: char) -> bool {
    if is_chat_composer_context(ctx) {
        chat::input::handle_compose_char(app, ch);
        return true;
    }
    if ctx.screen == Screen::Dashboard && ctx.feeds_processing {
        return true;
    }
    if ctx.screen == Screen::Dashboard && ctx.showcase_composing {
        app.chat.showcase.field_insert_char(ch);
        return true;
    }
    if ctx.screen == Screen::Dashboard && ctx.work_composing {
        app.chat.work.field_insert_char(ch);
        return true;
    }
    false
}

fn handle_byte_event(app: &mut App, ctx: InputContext, byte: u8) {
    if room_jump_active_on_current_screen(app, ctx.screen) {
        let _ = chat::input::handle_byte(app, byte);
        return;
    }

    if handle_modal_input(app, ctx, byte) {
        return;
    }

    if byte == b'/' && start_slash_command_composer(app, ctx.screen) {
        return;
    }

    if handle_global_key(app, ctx, byte) {
        app.chat.clear_message_selection();
        return;
    }

    dispatch_screen_key(app, ctx.screen, byte);
}

fn room_jump_active_on_current_screen(app: &App, screen: Screen) -> bool {
    app.chat.room_jump_active && matches!(screen, Screen::Dashboard)
}

fn toggle_room_section_from_key(app: &mut App, ctx: InputContext, section: RoomSection) -> bool {
    if ctx.screen != Screen::Dashboard
        || ctx.chat_composing
        || ctx.feeds_processing
        || ctx.news_composing
        || ctx.showcase_composing
        || ctx.work_composing
        || app.chat.room_jump_active
    {
        return false;
    }

    app.chat.toggle_section(section);
    app.chat.reset_composer();
    app.sync_visible_chat_room();
    app.chat.request_list();
    true
}

fn input_dismisses_key_modal(event: &ParsedInput) -> bool {
    !matches!(
        event,
        ParsedInput::Mouse(_)
            | ParsedInput::FocusGained
            | ParsedInput::FocusLost
            | ParsedInput::TerminalVersion(_)
            | ParsedInput::TerminalCapabilities(_)
    )
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
    if app.show_mod_modal {
        app.show_mod_modal = false;
        return;
    }
    if app.show_hub_modal {
        hub::input::handle_escape(app);
        return;
    }
    if app.show_ultimate_modal {
        app.show_ultimate_modal = false;
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
    if app.show_bonsai_v2_modal {
        crate::app::bonsai_v2::modal_input::handle_escape(app);
        return;
    }
    if app.show_bonsai_modal {
        crate::app::bonsai::modal_input::handle_escape(app);
        return;
    }
    if app.show_cat_modal {
        app.pet_state.cancel_play();
        app.show_cat_modal = false;
        return;
    }
    if app.icon_picker_open {
        app.icon_picker_open = false;
        return;
    }
    if app.room_search_modal_state.is_open() {
        app.room_search_modal_state.close();
        return;
    }
    if app.booth_modal_state.is_open() {
        app.booth_modal_state.close();
        return;
    }
    if app.chat.has_news_modal() {
        app.chat.close_news_modal();
        return;
    }
    if app.chat.has_image_modal() {
        close_image_modal(app);
        return;
    }
    let ctx = InputContext::from_app(app);
    if app.room_section_prefix_armed {
        app.room_section_prefix_armed = false;
        return;
    }
    if ctx.screen == Screen::Dashboard && app.chat.room_jump_active {
        app.chat.cancel_room_jump();
        return;
    }
    if handle_modal_input(app, ctx, 0x1B) {
        return;
    }
    if matches!(ctx.screen, Screen::Dashboard | Screen::Rooms)
        && app.chat.is_reaction_leader_active()
    {
        app.chat.cancel_reaction_leader();
        return;
    }
    if matches!(ctx.screen, Screen::Dashboard | Screen::Rooms) && app.chat.has_overlay() {
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
    if ctx.screen == Screen::Arcade && app.is_playing_game {
        dispatch_screen_key(app, ctx.screen, 0x1B);
        return;
    }
    if ctx.screen == Screen::Pinstar {
        if app.pinstar_state.is_none() && ctx.directory_tab == DirectoryTab::Profiles {
            if app.chat.work.composing() {
                app.chat.work.stop_composing();
            }
            return;
        }
        if app.pinstar_state.is_none() && ctx.directory_tab == DirectoryTab::Projects {
            if app.chat.showcase.composing() {
                app.chat.showcase.stop_composing();
            }
            return;
        }
        // If a browser popup is active (Create, Rename, Delete, AcceptInvite),
        // forward Esc to the browser input handler
        let is_browser_popup = !matches!(
            app.pinstar_browser.mode,
            crate::app::pinstar::browser::BrowserMode::List
        );
        if app.pinstar_state.is_none() && is_browser_popup {
            let event = ParsedInput::Byte(0x1B);
            handle_pinstar_browser_input(app, &event);
            return;
        }
        let mut area = app_content_area(app);
        area.y = area.y.saturating_add(1);
        area.height = area.height.saturating_sub(1);
        if let Some(state) = &mut app.pinstar_state {
            let key = crossterm::event::KeyEvent::new(
                crossterm::event::KeyCode::Esc,
                crossterm::event::KeyModifiers::NONE,
            );
            let handled = crate::app::pinstar::input::handle_pinstar_key(
                state,
                key,
                area,
                app.pinstar_registry.db(),
            );
            if handled {
                return;
            }
            app.pinstar_state = None;
            app.refresh_pinstar_browser();
        }
        return;
    }
    if ctx.screen == Screen::Dashboard && ctx.feeds_processing {
        app.chat.feeds.stop_processing();
        return;
    }
    if ctx.screen == Screen::Rooms {
        dispatch_screen_key(app, ctx.screen, 0x1B);
        return;
    }
    if ctx.screen == Screen::Dashboard && app.chat.selected_message_id.is_some() {
        app.chat.clear_message_selection();
    }
}

fn handle_bracketed_paste(app: &mut App, pasted: &[u8]) {
    let ctx = InputContext::from_app(app);
    match paste_target(ctx) {
        PasteTarget::ChatComposer => {
            if crate::app::files::image_upload::detect_image_mime(pasted).is_some() {
                trigger_image_upload(app, pasted.to_vec());
                return;
            }
            insert_pasted_text(pasted, |ch| app.chat.composer_push(ch));
            app.chat.update_autocomplete();
        }
        PasteTarget::NewsComposer => {
            insert_pasted_text(pasted, |ch| app.chat.news.composer_push(ch));
        }
        PasteTarget::ShowcaseComposer => {
            insert_pasted_text(pasted, |ch| app.chat.showcase.field_insert_char(ch));
        }
        PasteTarget::WorkComposer => {
            insert_pasted_text(pasted, |ch| app.chat.work.field_insert_char(ch));
        }
        PasteTarget::Pinstar => {
            if let Some(state) = &mut app.pinstar_state {
                if let Some(textarea) = &mut state.rename_popup {
                    insert_pasted_text(pasted, |ch| {
                        textarea.insert_char(ch);
                    });
                } else if let Some(textarea) = &mut state.floating_editor {
                    insert_pasted_text(pasted, |ch| {
                        textarea.insert_char(ch);
                    });
                }
            } else if app.pinstar_browser.mode
                == crate::app::pinstar::browser::BrowserMode::ImportCanvas
            {
                insert_pasted_text(pasted, |ch| {
                    app.pinstar_browser.import_input.push(ch);
                });
            } else if app.pinstar_browser.mode
                == crate::app::pinstar::browser::BrowserMode::AcceptInvite
            {
                insert_pasted_text(pasted, |ch| {
                    let _ = app.pinstar_browser.push_invite_token_char(ch);
                });
            }
        }
        PasteTarget::None => {}
    }
}

fn trigger_image_upload(app: &mut App, data: Vec<u8>) {
    if let Some(banner) = app.chat.start_image_upload(data) {
        app.banner = Some(banner);
        return;
    }
    app.banner = Some(crate::app::common::primitives::Banner::success(
        "Image detected - uploading...",
    ));
}

pub(crate) fn trigger_url_image_upload(app: &mut App, url: String, room_id: Option<uuid::Uuid>) {
    use crate::app::files::image_upload::{download_and_reupload_url, is_file_upload_configured};
    if !is_file_upload_configured() {
        app.banner = Some(crate::app::common::primitives::Banner::error(
            "File uploads are disabled",
        ));
        return;
    }

    let (tx, rx) = tokio::sync::oneshot::channel();
    if let Some(banner) = app.chat.begin_image_upload(room_id, rx) {
        app.banner = Some(banner);
        return;
    }
    tokio::spawn(async move {
        let result = download_and_reupload_url(url)
            .await
            .map_err(|e| e.to_string());
        let _ = tx.send(result);
    });
    app.banner = Some(crate::app::common::primitives::Banner::success(
        "Downloading and uploading image...",
    ));
}

fn paste_target(ctx: InputContext) -> PasteTarget {
    if is_chat_composer_context(ctx) {
        PasteTarget::ChatComposer
    } else if ctx.screen == Screen::Dashboard && ctx.news_composing {
        PasteTarget::NewsComposer
    } else if (ctx.screen == Screen::Dashboard
        || (ctx.screen == Screen::Pinstar && ctx.directory_tab == DirectoryTab::Projects))
        && ctx.showcase_composing
    {
        PasteTarget::ShowcaseComposer
    } else if (ctx.screen == Screen::Dashboard
        || (ctx.screen == Screen::Pinstar && ctx.directory_tab == DirectoryTab::Profiles))
        && ctx.work_composing
    {
        PasteTarget::WorkComposer
    } else if ctx.screen == Screen::Pinstar {
        PasteTarget::Pinstar
    } else {
        PasteTarget::None
    }
}

pub(crate) fn insert_pasted_text(pasted: &[u8], mut push: impl FnMut(char)) {
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
            if let Some(room_id) = app.chat.selected_room_id {
                chat::input::handle_scroll_in_room(app, room_id, delta);
            }
        }
        Screen::Rooms => {
            if let Some(room) = app.rooms_active_room.as_ref() {
                chat::input::handle_scroll_in_room(app, room.chat_room_id, delta);
            }
        }
        Screen::Artboard => {}
        Screen::Pinstar => {}
        _ => {}
    }
}

fn topbar_screen_hit_test(x: u16, y: u16) -> Option<Screen> {
    if y != 0 {
        return None;
    }

    match x {
        // Top title text starts immediately after the left border. The digit
        // cells in " late.sh | 1 2 3 4 5 6 | ..." land on these columns.
        12 => Some(Screen::Dashboard),
        14 => Some(Screen::Arcade),
        16 => Some(Screen::Rooms),
        18 => Some(Screen::DoorGames),
        20 => Some(Screen::Artboard),
        22 => Some(Screen::Pinstar),
        _ => None,
    }
}

fn select_screen_from_topbar(app: &mut App, current: Screen, target: Screen) {
    if target == current {
        return;
    }

    reset_composers_for_page_change(app);
    if target == Screen::Rooms {
        app.rooms_active_room = None;
    }
    app.set_screen(target);
    app.chat.clear_message_selection();
}

fn chat_room_list_view<'a>(
    app: &'a App,
    usernames: &'a UsernameLookup<'a>,
) -> crate::app::chat::ui::ChatRoomListView<'a> {
    crate::app::chat::ui::ChatRoomListView {
        chat_rooms: &app.chat.rooms,
        usernames,
        unread_counts: &app.chat.unread_counts,
        room_last_message_at: &app.chat.room_last_message_at,
        favorite_room_ids: &app.profile_state.profile().favorite_room_ids,
        collapsed_sections: &app.chat.collapsed_sections,
        selected_room_id: app.chat.selected_room_id,
        room_jump_active: app.chat.room_jump_active,
        room_section_prefix_armed: app.room_section_prefix_armed,
        current_user_id: app.user_id,
        feeds_available: app.chat.feeds.has_feeds(),
        feeds_selected: app.chat.feeds_selected,
        feeds_unread_count: app.chat.feeds.unread_count(),
        news_selected: app.chat.news_selected,
        news_unread_count: app.chat.news.unread_count(),
        notifications_selected: app.chat.notifications_selected,
        notifications_unread_count: app.chat.notifications.unread_count(),
        voice_selected: app.chat.voice_selected,
        voice_participant_count: app.voice.snapshot().participants.len(),
        discover_selected: app.chat.discover_selected,
        showcase_selected: app.chat.showcase_selected,
        showcase_unread_count: app.chat.showcase.unread_count(),
        work_selected: app.chat.work_selected,
        work_unread_count: app.chat.work.unread_count(),
    }
}

fn apply_chat_room_selection_delta(app: &mut App, delta: isize) {
    if app.chat.move_selection(delta) {
        app.chat.reset_composer();
        app.sync_visible_chat_room();
        app.chat.request_list();
    }
}

fn handle_mouse_scroll_over_screen(
    app: &mut App,
    screen: Screen,
    mouse: MouseEvent,
    delta: isize,
) -> bool {
    if !matches!(screen, Screen::Dashboard) {
        return false;
    }
    let Some(x) = mouse.x.checked_sub(1) else {
        return false;
    };
    let Some(y) = mouse.y.checked_sub(1) else {
        return false;
    };

    // Home top-strip Activity panel: wheel scrolls the recent-events feed
    // through the in-memory `activity` buffer. Bigger offset = older
    // events; clamp to the events outside the visible window so a trim
    // can't strand us past the end.
    if let Some(rect) = app.last_dashboard_activity_rect.get()
        && rect_contains(rect, x, y)
    {
        let visible = activity_visible_event_rows(!app.chat.active_friend_names().is_empty());
        let max_offset = app.activity.len().saturating_sub(visible) as u16;
        let current = app.dashboard_activity_scroll.min(max_offset);
        // delta > 0 (wheel up) reveals newer events → smaller offset.
        // delta < 0 (wheel down) reveals older events → larger offset.
        let next = if delta > 0 {
            current.saturating_sub(ACTIVITY_SCROLL_STEP)
        } else {
            current.saturating_add(ACTIVITY_SCROLL_STEP).min(max_offset)
        };
        app.dashboard_activity_scroll = next;
        return true;
    }

    let Some(rooms_area) = dashboard_room_rail_area(app) else {
        return false;
    };
    let username_directory_snapshot = app
        .username_directory
        .as_ref()
        .map(crate::usernames::snapshot);
    let usernames =
        UsernameLookup::new(app.chat.usernames(), username_directory_snapshot.as_deref());
    let room_list_view = chat_room_list_view(app, &usernames);
    let over_room_list =
        crate::app::chat::ui::room_list_panel_contains(rooms_area, &room_list_view, x, y);
    if !over_room_list {
        return false;
    }

    let selection_delta = if delta > 0 { -1 } else { 1 };
    apply_chat_room_selection_delta(app, selection_delta);
    true
}

/// One wheel notch moves the Activity feed by this many events. Single-step
/// keeps the scroll readable on small panels without overshooting the
/// 3-4 visible rows.
const ACTIVITY_SCROLL_STEP: u16 = 1;

fn activity_visible_event_rows(has_active_friends: bool) -> usize {
    if has_active_friends { 3 } else { 4 }
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
    if let Some(target) = topbar_screen_hit_test(x, y) {
        app.pending_chat_profile_open = None;
        select_screen_from_topbar(app, screen, target);
        return true;
    }
    if handle_chat_composer_click(app, screen, x, y) {
        return true;
    }
    if handle_chat_scroll_click(app, screen, x, y) {
        return true;
    }
    match screen {
        Screen::Dashboard => {
            let Some(rooms_area) = dashboard_room_rail_area(app) else {
                return false;
            };
            // Resolve both hits before any mutation so the `app` borrow held
            // by `room_list_view` is released first.
            let username_directory_snapshot = app
                .username_directory
                .as_ref()
                .map(crate::usernames::snapshot);
            let usernames =
                UsernameLookup::new(app.chat.usernames(), username_directory_snapshot.as_deref());
            let room_list_view = chat_room_list_view(app, &usernames);
            let section =
                crate::app::chat::ui::room_list_section_hit_test(rooms_area, &room_list_view, x, y);
            let slot = crate::app::chat::ui::room_list_hit_test(rooms_area, &room_list_view, x, y);
            if let Some(section) = section {
                app.pending_chat_profile_open = None;
                app.chat.toggle_section(section);
                app.chat.reset_composer();
                app.sync_visible_chat_room();
                app.chat.request_list();
                return true;
            }
            if let Some(slot) = slot {
                app.pending_chat_profile_open = None;
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

/// Double-click inside the chat composer bar enters compose mode, mirroring
/// `i`/Enter. Only fires on Dashboard / Rooms — the only screens where the
/// chat composer is drawn. A single click is intentionally a no-op so that
/// the existing message-row click flow (selection, link-open) keeps working
/// for clicks that just miss the composer.
fn handle_chat_composer_click(app: &mut App, screen: Screen, x: u16, y: u16) -> bool {
    if !matches!(screen, Screen::Dashboard | Screen::Rooms) {
        return false;
    }
    let Some(rect) = app.chat.last_composer_rect.get() else {
        return false;
    };
    if !rect_contains(rect, x, y) {
        return false;
    }
    app.pending_chat_profile_open = None;
    let now = std::time::Instant::now();
    let is_double = matches!(
        app.chat.last_composer_click,
        Some((px, py, pt))
            if px == x
                && py == y
                && now.duration_since(pt) <= COMPOSER_DOUBLE_CLICK_WINDOW
    );
    if is_double {
        app.chat.last_composer_click = None;
        if let Some(room_id) = chat_click_room_id(app, screen) {
            app.chat.start_composing_in_room(room_id);
        }
    } else {
        app.chat.last_composer_click = Some((x, y, now));
    }
    true
}

fn rect_contains(rect: Rect, x: u16, y: u16) -> bool {
    x >= rect.x
        && x < rect.x.saturating_add(rect.width)
        && y >= rect.y
        && y < rect.y.saturating_add(rect.height)
}

const COMPOSER_DOUBLE_CLICK_WINDOW: std::time::Duration = std::time::Duration::from_millis(500);

/// Window for treating two clicks on the same chat-scroll cell + target as a
/// double-click. Mirrors the composer-bar window.
const CHAT_CLICK_DOUBLE_WINDOW: std::time::Duration = std::time::Duration::from_millis(500);

/// Delay before a click on a username actually opens the profile modal —
/// a fast second click on the same username within this window converts
/// the action into inserting an `@mention` into the composer instead.
pub(crate) const PROFILE_CLICK_DEBOUNCE: std::time::Duration = CHAT_CLICK_DOUBLE_WINDOW;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ChatClickKind {
    /// Click landed on a message body (or reaction footer) — single click
    /// selects the message, double click starts a reply to it.
    BodySelect { message_id: Uuid },
    /// Click landed on the author label (username, friend badge, special
    /// badge, or bonsai glyph). Debounced single click opens the
    /// profile modal; a fast second click on the same row inserts
    /// `@username` into the composer.
    ProfileOf { message_id: Uuid },
    /// Click landed on the user's currently-equipped chat-shop badge —
    /// opens the Hub Shop on the Badges sub-store. No double-click verb.
    StoreBadge,
    /// Click landed on the user's currently-equipped chat flag — opens
    /// the Hub Shop on the Flags sub-store. No double-click verb.
    StoreFlag,
    /// Click landed on an inline image preview row — selects the message
    /// and opens the image viewer modal. No double-click verb.
    Image { message_id: Uuid },
}

impl ChatClickKind {
    /// `true` when a second click on the same cell would change behavior
    /// — only body and username clicks promote on the second tap; store
    /// badges and image previews always act on the first.
    fn has_double_click_followup(self) -> bool {
        matches!(self, Self::BodySelect { .. } | Self::ProfileOf { .. })
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ChatClickRecord {
    pub x: u16,
    pub y: u16,
    pub kind: ChatClickKind,
    pub time: std::time::Instant,
}

#[derive(Clone, Debug)]
pub(crate) struct PendingChatProfileOpen {
    pub user_id: Uuid,
    pub username: String,
    pub time: std::time::Instant,
}

/// Resolve which chat room a click in the message scroll targets.
/// Single source of truth so the composer bar
/// (`handle_chat_composer_click`) and the message scroll above it
/// always agree on which room a click belongs to.
fn chat_click_room_id(app: &App, screen: Screen) -> Option<Uuid> {
    match screen {
        Screen::Rooms => app.rooms_active_room.as_ref().map(|r| r.chat_room_id),
        Screen::Dashboard => app.chat.selected_room_id,
        _ => None,
    }
}

/// `true` when a top-level modal/overlay is up on top of the chat scroll
/// and should consume clicks instead of letting them fall through to
/// message hit-testing. `chat::ui` already skips publishing the hit
/// layout when an in-chat overlay or image modal is open, so this only
/// covers the global modals that sit above the whole screen.
fn chat_scroll_clicks_blocked(app: &App) -> bool {
    app.show_splash
        || app.show_settings
        || app.show_hub_modal
        || app.show_profile_modal
        || app.show_quit_confirm
        || app.show_bonsai_modal
        || app.show_cat_modal
        || app.icon_picker_open
}

/// Pure classification of a chat-scroll hit by column. Splits header
/// rows into username (→ profile/mention), equipped chat-shop badge
/// (→ Badges), or equipped chat flag (→ Flags), and leaves body / image
/// / blank rows untouched. Extracted so it can be unit-tested without
/// standing up an `App`.
fn classify_chat_hit(hit: &ChatRowHit, col: u16) -> Option<ChatClickKind> {
    let message_id = hit.message_id?;
    match &hit.kind {
        ChatRowKind::Header(segments) => Some(
            match segments
                .iter()
                .find(|seg| seg.contains(col))
                .map(|s| s.target)
            {
                Some(HeaderTarget::Profile) => ChatClickKind::ProfileOf { message_id },
                Some(HeaderTarget::StoreBadge) => ChatClickKind::StoreBadge,
                Some(HeaderTarget::StoreFlag) => ChatClickKind::StoreFlag,
                None => ChatClickKind::BodySelect { message_id },
            },
        ),
        ChatRowKind::Image => Some(ChatClickKind::Image { message_id }),
        ChatRowKind::Body => Some(ChatClickKind::BodySelect { message_id }),
        ChatRowKind::None => None,
    }
}

/// Resolve a left-button click against the most recently painted chat
/// scroll layout. Single-click semantics fire immediately for body /
/// image / store-badge targets; the username (`ProfileOf`) target is
/// debounced via `app.pending_chat_profile_open` so a fast second click
/// can be promoted to an `@mention` insertion in `App::tick`. Returns
/// `true` if the click was consumed.
fn handle_chat_scroll_click(app: &mut App, screen: Screen, x: u16, y: u16) -> bool {
    if !matches!(screen, Screen::Dashboard | Screen::Rooms) {
        return false;
    }
    if chat_scroll_clicks_blocked(app) {
        return false;
    }
    let Some(layout) = app.chat.last_chat_hit_layout.take() else {
        return false;
    };
    if !rect_contains(layout.content, x, y) {
        return false;
    }
    app.pending_chat_profile_open = None;
    let row_idx = (y - layout.content.y) as usize;
    let col = x - layout.content.x;
    let Some(hit) = layout.rows.get(row_idx) else {
        return false;
    };
    let Some(room_id) = chat_click_room_id(app, screen) else {
        return false;
    };
    let Some(kind) = classify_chat_hit(hit, col) else {
        return false;
    };

    let now = std::time::Instant::now();
    let is_double = matches!(
        app.last_chat_click,
        Some(rec)
            if rec.x == x
                && rec.y == y
                && rec.kind == kind
                && now.duration_since(rec.time) <= CHAT_CLICK_DOUBLE_WINDOW
    );

    match kind {
        ChatClickKind::BodySelect { message_id } => {
            app.chat.select_message_by_id_in_room(room_id, message_id);
            if is_double && let Some(banner) = app.chat.begin_reply_to_selected_in_room(room_id) {
                app.banner = Some(banner);
            }
        }
        ChatClickKind::ProfileOf { message_id } => {
            let Some((user_id, username)) = app.chat.message_author_in_room(room_id, message_id)
            else {
                return true;
            };
            if is_double {
                // Promote to `@mention` insertion — cancel any pending
                // profile-open so the modal does not pop afterwards.
                app.chat.insert_mention_in_room(room_id, &username);
            } else {
                // Hold the profile-open until the debounce elapses; the
                // tick loop fires it if no double-click overrides.
                app.pending_chat_profile_open = Some(PendingChatProfileOpen {
                    user_id,
                    username,
                    time: now,
                });
            }
        }
        ChatClickKind::StoreBadge => {
            app.hub_state.open(crate::app::hub::state::HubTab::Shop);
            app.show_hub_modal = true;
            app.shop_state
                .select_category(crate::app::hub::shop::catalog::ShopCategory::Badges);
        }
        ChatClickKind::StoreFlag => {
            app.hub_state.open(crate::app::hub::state::HubTab::Shop);
            app.show_hub_modal = true;
            app.shop_state
                .select_category(crate::app::hub::shop::catalog::ShopCategory::Flags);
        }
        ChatClickKind::Image { message_id } => {
            app.chat.select_message_by_id_in_room(room_id, message_id);
            app.chat.open_selected_image_modal_in_room(room_id);
        }
    }

    // Single bookkeeping point: remember the click only when a fast
    // follow-up would change its outcome — and not when this click was
    // already that follow-up.
    app.last_chat_click =
        (!is_double && kind.has_double_click_followup()).then_some(ChatClickRecord {
            x,
            y,
            kind,
            time: now,
        });

    true
}

fn dashboard_room_rail_area(app: &App) -> Option<Rect> {
    if !app.profile_state.profile().show_room_list_sidebar {
        return None;
    }
    const HOME_RAIL_WIDTH: u16 = 24;
    let content_area = app_content_area(app);
    (content_area.width > HOME_RAIL_WIDTH + 20).then_some(Rect {
        x: content_area.x,
        y: content_area.y,
        width: HOME_RAIL_WIDTH,
        height: content_area.height,
    })
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

    app.pending_chat_profile_open = None;
    app.set_screen(Screen::Dashboard);
    app.chat.select_notifications();
    true
}

fn app_content_area(app: &App) -> Rect {
    let area = Rect::new(0, 0, app.size.0, app.size.1);
    let inner = Block::default().borders(Borders::ALL).inner(area);
    let profile = app.profile_state.profile();
    if crate::app::render::resolve_right_sidebar_enabled(
        profile.right_sidebar_mode,
        &profile.right_sidebar_screens,
        app.screen,
    ) {
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
    if matches!(screen, Screen::Dashboard | Screen::Rooms)
        && app.chat.is_composing()
        && app.chat.is_autocomplete_active()
    {
        chat::input::handle_autocomplete_arrow(app, key);
        return true;
    }

    match screen {
        Screen::Dashboard => dashboard::input::handle_arrow(app, key),
        Screen::DoorGames => false,
        Screen::Arcade => crate::app::arcade::input::handle_arrow(app, key),
        Screen::Rooms => crate::app::rooms::input::handle_arrow(app, key),
        Screen::Artboard => crate::app::artboard::page::handle_arrow(app, key),
        Screen::Pinstar => {
            // Arrows handled via handle_dedicated_screen_input
            false
        }
    }
}

fn handle_modal_input(app: &mut App, ctx: InputContext, byte: u8) -> bool {
    if is_chat_composer_context(ctx) {
        chat::input::handle_compose_input(
            app,
            byte,
            compose_room_switch_allowed(ctx.screen),
            ctx.screen == Screen::Dashboard,
        );
        return true;
    }

    if ctx.screen == Screen::Dashboard && ctx.news_composing {
        chat::news::input::handle_composer_input(app, byte);
        return true;
    }

    if ctx.screen == Screen::Dashboard && ctx.showcase_composing {
        chat::showcase::input::handle_composer_input(app, byte);
        return true;
    }

    if ctx.screen == Screen::Dashboard && ctx.work_composing {
        chat::work::input::handle_composer_input(app, byte);
        return true;
    }

    false
}

fn compose_room_switch_allowed(screen: Screen) -> bool {
    matches!(screen, Screen::Dashboard)
}

fn start_slash_command_composer(app: &mut App, screen: Screen) -> bool {
    if app.chat.is_composing()
        || app.chat.news.composing()
        || app.chat.showcase.composing()
        || app.chat.work.composing()
    {
        return false;
    }

    // On synthetic chat entries (News/Showcase/Work), `/` is the
    // filter-mine toggle, not a slash-command starter.
    if matches!(screen, Screen::Dashboard)
        && (app.chat.news_selected || app.chat.showcase_selected || app.chat.work_selected)
    {
        return false;
    }

    let room_id = match screen {
        Screen::Dashboard => app.chat.selected_room_id,
        _ => None,
    };
    let Some(room_id) = room_id else {
        return false;
    };

    if matches!(screen, Screen::Dashboard) {
        app.chat
            .select_room_slot(crate::app::chat::state::RoomSlot::Room(room_id));
    }
    app.chat.start_command_composer_in_room(room_id);
    true
}

fn reset_composers_for_page_change(app: &mut App) {
    app.chat.reset_composer();
    app.chat.feeds.stop_processing();
    app.chat.news.stop_composing();
    app.chat.showcase.stop_composing();
    app.chat.work.stop_composing();
    app.chat.close_news_modal();
}

fn is_room_search_shortcut(event: &ParsedInput) -> bool {
    matches!(event, ParsedInput::Byte(0x1F))
}

fn clear_prefix_arms(app: &mut App) {
    app.vote_prefix_armed = false;
    app.room_join_prefix_armed = false;
    app.room_section_prefix_armed = false;
}

fn open_room_search_modal_globally(app: &mut App) {
    clear_prefix_arms(app);
    app.show_help = false;
    app.show_mod_modal = false;
    app.show_hub_modal = false;
    app.show_profile_modal = false;
    app.show_bonsai_modal = false;
    app.show_bonsai_v2_modal = false;
    app.pet_state.cancel_play();
    app.show_cat_modal = false;
    app.show_settings = false;
    app.show_quit_confirm = false;
    app.icon_picker_open = false;
    app.chat.close_overlay();
    app.chat.close_news_modal();
    app.chat.cancel_room_jump();
    app.room_search_modal_state.open();
}

fn open_settings_modal_globally(app: &mut App) {
    clear_prefix_arms(app);
    app.show_help = false;
    app.show_mod_modal = false;
    app.show_hub_modal = false;
    app.show_profile_modal = false;
    app.show_bonsai_modal = false;
    app.show_bonsai_v2_modal = false;
    app.pet_state.cancel_play();
    app.show_cat_modal = false;
    app.show_quit_confirm = false;
    app.icon_picker_open = false;
    app.chat.close_overlay();
    app.chat.close_news_modal();
    app.chat.cancel_room_jump();
    app.settings_modal_state
        .open_from_profile(app.profile_state.profile());
    app.show_settings = true;
}

fn open_hub_modal_globally(app: &mut App) {
    clear_prefix_arms(app);
    app.show_help = false;
    app.show_mod_modal = false;
    app.show_profile_modal = false;
    app.show_bonsai_modal = false;
    app.show_bonsai_v2_modal = false;
    app.pet_state.cancel_play();
    app.show_cat_modal = false;
    app.show_settings = false;
    app.show_quit_confirm = false;
    app.icon_picker_open = false;
    app.chat.close_overlay();
    app.chat.close_news_modal();
    app.chat.cancel_room_jump();
    app.hub_state.open(crate::app::hub::state::HubTab::Shop);
    app.show_hub_modal = true;
}

fn toggle_aquarium_tray_globally(app: &mut App) {
    clear_prefix_arms(app);
    if !app.shop_state.entitlements().has_aquarium() {
        app.banner = Some(crate::app::common::primitives::Banner::error(
            "Unlock Aquarium in Hub Shop",
        ));
        open_hub_modal_globally(app);
        return;
    }
    app.show_aquarium_tray = !app.show_aquarium_tray;
}

fn open_bonsai_v2_modal_globally(app: &mut App) {
    clear_prefix_arms(app);
    app.show_help = false;
    app.show_mod_modal = false;
    app.show_hub_modal = false;
    app.show_profile_modal = false;
    app.show_bonsai_modal = false;
    app.show_bonsai_v2_modal = false;
    app.pet_state.cancel_play();
    app.show_cat_modal = false;
    app.show_settings = false;
    app.show_quit_confirm = false;
    app.icon_picker_open = false;
    app.chat.close_overlay();
    app.chat.close_news_modal();
    app.chat.cancel_room_jump();
    app.show_bonsai_v2_modal = true;
}

fn room_join_suffix_index(byte: u8) -> Option<usize> {
    match byte {
        b'1' => Some(0),
        b'2' => Some(1),
        b'3' => Some(2),
        b'4' => Some(3),
        _ => None,
    }
}

fn room_section_suffix(byte: u8) -> Option<RoomSection> {
    match byte {
        b'f' | b'F' => Some(RoomSection::Favorites),
        b'o' | b'O' => Some(RoomSection::Core),
        b'c' | b'C' => Some(RoomSection::Channels),
        b'u' | b'U' => Some(RoomSection::Updates),
        b'd' | b'D' => Some(RoomSection::Dms),
        _ => None,
    }
}

fn enter_recent_join_room(app: &mut App, index: usize) -> bool {
    let Some(room) = crate::app::dashboard::ui::recent_dashboard_rooms(
        &app.rooms_snapshot,
        &app.room_game_registry,
        &app.dashboard_room_joins,
        4,
    )
    .into_iter()
    .nth(index)
    .map(|card| card.room) else {
        app.banner = Some(crate::app::common::primitives::Banner::error(&format!(
            "No recent room join in slot {}.",
            index + 1
        )));
        return true;
    };

    if crate::app::rooms::input::enter_room(app, room) {
        reset_composers_for_page_change(app);
        app.set_screen(Screen::Rooms);
    }
    true
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

fn handle_reserved_global_chord(app: &mut App, event: &ParsedInput) -> bool {
    let ParsedInput::Byte(byte) = event else {
        return false;
    };

    // Reserved app-level chords. Do not touch these keys or add local handlers
    // for them without updating help/docs/tests. Active Artboard editing owns
    // raw control bytes as drawing commands.
    if app.screen == Screen::Artboard && app.artboard_interacting {
        return false;
    }

    match *byte {
        CTRL_O => {
            open_settings_modal_globally(app);
            true
        }
        CTRL_G => {
            open_hub_modal_globally(app);
            true
        }
        _ => false,
    }
}

fn handle_global_key(app: &mut App, ctx: InputContext, byte: u8) -> bool {
    let artboard_blocks_page_switch = artboard_blocks_global_page_switch(app, ctx.screen);

    // `?` opens the global guide unless the current screen owns local help.
    let guide_shortcut = byte == b'?'
        && !ctx.chat_composing
        && !ctx.feeds_processing
        && !ctx.news_composing
        && !ctx.showcase_composing
        && !ctx.work_composing
        && ctx.screen != Screen::Artboard
        && !(ctx.screen == Screen::Pinstar && app.pinstar_state.is_some());
    let chat_message_shortcut = matches!(ctx.screen, Screen::Dashboard | Screen::Rooms)
        && app.chat.selected_message_id.is_some();
    if guide_shortcut && !chat_message_shortcut {
        app.help_modal_state
            .set_keep_composer_focused(app.profile_state.profile().keep_composer_focused);
        app.help_modal_state
            .open(crate::app::help_modal::data::HelpTopic::Pair);
        app.show_help = true;
        return true;
    }

    if matches!(byte, b'1' | b'2' | b'3' | b'4' | b'5' | b'6' | b'7' | b'8')
        && ctx.screen == Screen::Dashboard
        && app.chat.is_reaction_leader_active()
    {
        return false;
    }

    if ctx.screen == Screen::Arcade && app.is_playing_game {
        return false;
    }

    if ctx.screen == Screen::Artboard && app.artboard_interacting {
        return false;
    }

    if app.vote_prefix_armed {
        app.vote_prefix_armed = false;
        if crate::app::vote::input::handle_vote_suffix(app, byte) {
            return true;
        }
    }

    if app.room_section_prefix_armed {
        app.room_section_prefix_armed = false;
        if let Some(section) = room_section_suffix(byte) {
            return toggle_room_section_from_key(app, ctx, section);
        }
        return true;
    }

    if app.room_join_prefix_armed {
        app.room_join_prefix_armed = false;
        if let Some(index) = room_join_suffix_index(byte) {
            return enter_recent_join_room(app, index);
        }
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
                    crate::app::audio::client_state::ClientKind::Unknown => "client".to_string(),
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
                    crate::app::audio::client_state::ClientKind::Unknown => "client".to_string(),
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
                    crate::app::audio::client_state::ClientKind::Unknown => "client".to_string(),
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
        b'b' | b'B'
            if !ctx.chat_composing
                && !ctx.feeds_processing
                && !ctx.news_composing
                && !ctx.showcase_composing
                && !ctx.work_composing =>
        {
            app.room_join_prefix_armed = true;
            true
        }
        b'v' | b'V'
            if !ctx.chat_composing
                && !ctx.feeds_processing
                && !ctx.news_composing
                && !ctx.showcase_composing
                && !ctx.work_composing =>
        {
            app.vote_prefix_armed = true;
            true
        }
        b'z' | b'Z'
            if ctx.screen == Screen::Dashboard
                && !ctx.chat_composing
                && !ctx.feeds_processing
                && !ctx.news_composing
                && !ctx.showcase_composing
                && !ctx.work_composing
                && !app.chat.room_jump_active =>
        {
            app.room_section_prefix_armed = true;
            true
        }
        b'w' | b'W'
            if !ctx.chat_composing
                && !ctx.feeds_processing
                && !ctx.news_composing
                && !ctx.showcase_composing
                && !ctx.work_composing =>
        {
            if app.use_bonsai_v2() {
                open_bonsai_v2_modal_globally(app);
            } else {
                app.show_help = false;
                app.show_profile_modal = false;
                app.show_settings = false;
                app.show_hub_modal = false;
                app.show_quit_confirm = false;
                app.show_bonsai_v2_modal = false;
                app.show_bonsai_modal = true;
            }
            true
        }
        b'c' | b'C' if cat_launcher_available(app, ctx) => {
            if !app.shop_state.entitlements().has_pet_companion() {
                app.banner = Some(crate::app::common::primitives::Banner::error(
                    "Unlock Pet Companion in Hub Shop",
                ));
                app.show_help = false;
                app.show_profile_modal = false;
                app.show_settings = false;
                app.show_quit_confirm = false;
                app.show_bonsai_modal = false;
                app.show_bonsai_v2_modal = false;
                app.pet_state.cancel_play();
                app.show_cat_modal = false;
                app.hub_state.open(crate::app::hub::state::HubTab::Shop);
                app.show_hub_modal = true;
                return true;
            }
            app.show_help = false;
            app.show_profile_modal = false;
            app.show_settings = false;
            app.show_hub_modal = false;
            app.show_quit_confirm = false;
            app.show_cat_modal = true;
            true
        }
        b'c' | b'C' if cat_launcher_available(app, ctx) => true,
        b'1' if !artboard_blocks_page_switch => {
            reset_composers_for_page_change(app);
            app.set_screen(Screen::Dashboard);
            true
        }
        b'2' if !artboard_blocks_page_switch => {
            reset_composers_for_page_change(app);
            app.set_screen(Screen::Arcade);
            true
        }
        b'3' if !artboard_blocks_page_switch => {
            reset_composers_for_page_change(app);
            app.rooms_active_room = None;
            app.set_screen(Screen::Rooms);
            true
        }
        b'4' if !artboard_blocks_page_switch => {
            reset_composers_for_page_change(app);
            app.set_screen(Screen::DoorGames);
            true
        }
        b'5' if !artboard_blocks_page_switch => {
            reset_composers_for_page_change(app);
            app.set_screen(Screen::Artboard);
            true
        }
        b'6' if !artboard_blocks_page_switch => {
            reset_composers_for_page_change(app);
            app.set_screen(Screen::Pinstar);
            true
        }
        b'\t' if !artboard_blocks_page_switch => {
            reset_composers_for_page_change(app);
            app.set_screen(ctx.screen.next());
            true
        }
        _ => false,
    }
}

fn cat_launcher_available(app: &App, ctx: InputContext) -> bool {
    if ctx.chat_composing
        || ctx.feeds_processing
        || ctx.news_composing
        || ctx.showcase_composing
        || ctx.work_composing
    {
        return false;
    }

    if ctx.screen == Screen::Dashboard {
        if app.chat.selected_message_id.is_some() {
            return false;
        }
        if app.chat.work_selected {
            return false;
        }
    }

    true
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
        Screen::DoorGames => {
            // Door Games key dispatch is handled via handle_dedicated_screen_input.
        }
        Screen::Arcade => {
            crate::app::arcade::input::handle_key(app, byte);
        }
        Screen::Rooms => {
            crate::app::rooms::input::handle_key(app, byte);
        }
        Screen::Artboard => {
            let _ = crate::app::artboard::page::handle_key(app, byte);
        }
        Screen::Pinstar => {
            // Pinstar key dispatch is handled via handle_dedicated_screen_input
            // and the rich-event path; byte dispatch is a no-op here.
        }
    }
}

fn handle_pinstar_browser_mouse(
    app: &mut App,
    event: &ParsedInput,
    area: ratatui::layout::Rect,
) -> bool {
    use crate::app::pinstar::browser::BrowserMode;

    let ParsedInput::Mouse(mouse) = event else {
        return false;
    };

    // Only handle mouse in List mode
    if app.pinstar_browser.mode != BrowserMode::List {
        return false;
    }

    let mx = mouse.x.saturating_sub(1);
    let my = mouse.y.saturating_sub(1);

    let inside_browser =
        mx >= area.x && mx < area.x + area.width && my >= area.y && my < area.y + area.height;

    match mouse.kind {
        MouseEventKind::Down if matches!(mouse.button, Some(MouseButton::Left)) => {
            if !inside_browser {
                return false;
            }

            let mut list_y = area.y.saturating_add(1);
            let mut list_height = area.height.saturating_sub(2);
            if app.pinstar_browser.error.is_some() && list_height > 1 {
                list_y = list_y.saturating_add(1);
                list_height = list_height.saturating_sub(1);
            }
            let header_rows = 2;
            if my < list_y + header_rows || my >= list_y + list_height {
                return true;
            }

            let window_height = (list_height as usize).saturating_sub(header_rows as usize);
            let offset = if window_height == 0 {
                0
            } else {
                app.pinstar_browser
                    .selected
                    .saturating_sub(window_height.saturating_sub(1))
            };
            let clicked_idx = offset + (my - list_y - header_rows) as usize;
            if clicked_idx < app.pinstar_browser.visible_len() {
                let is_double_click = if let Some((lx, ly, lt)) = app.pinstar_browser.last_click {
                    lx == mx && ly == my && lt.elapsed().as_millis() < 500
                } else {
                    false
                };

                app.pinstar_browser.selected = clicked_idx;
                app.pinstar_browser.last_click = Some((mx, my, std::time::Instant::now()));

                if is_double_click {
                    handle_pinstar_browser_double_click(app);
                    app.pinstar_browser.last_click = None;
                }
            }
            true
        }
        MouseEventKind::Down => inside_browser,
        MouseEventKind::ScrollUp => {
            if !inside_browser {
                return false;
            }
            app.pinstar_browser.move_up();
            true
        }
        MouseEventKind::ScrollDown => {
            if !inside_browser {
                return false;
            }
            app.pinstar_browser.move_down();
            true
        }
        _ => false,
    }
}

fn handle_pinstar_browser_double_click(app: &mut App) {
    if let Some(entry) = app.pinstar_browser.selected_entry() {
        app.pinstar_browser.pending_action = Some(
            crate::app::pinstar::browser::BrowserAction::Open(entry.id, entry.role.clone()),
        );
    }
}

fn handle_pinstar_browser_input(app: &mut App, event: &ParsedInput) -> bool {
    use crate::app::pinstar::browser::BrowserMode;

    match &mut app.pinstar_browser.mode {
        BrowserMode::List => match event {
            ParsedInput::Byte(0x10) | ParsedInput::Byte(b'?') | ParsedInput::Char('?') => {
                app.pinstar_browser.mode = BrowserMode::Help;
                true
            }
            ParsedInput::Byte(b'j') | ParsedInput::Char('j') | ParsedInput::Arrow(b'B') => {
                app.pinstar_browser.move_down();
                true
            }
            ParsedInput::Byte(b'k') | ParsedInput::Char('k') | ParsedInput::Arrow(b'A') => {
                app.pinstar_browser.move_up();
                true
            }
            ParsedInput::Byte(b'n') | ParsedInput::Char('n') => {
                app.pinstar_browser.new_diagram_name.clear();
                app.pinstar_browser.mode = BrowserMode::CreateDiagram;
                true
            }
            ParsedInput::Byte(b'a') | ParsedInput::Char('a') => {
                app.pinstar_browser.mode = BrowserMode::AcceptInvite;
                app.pinstar_browser.invite_token_input.clear();
                app.pinstar_browser.error = None;
                true
            }
            ParsedInput::Byte(b'I') | ParsedInput::Char('I') => {
                app.pinstar_browser.import_input.clear();
                app.pinstar_browser.import_name = String::from("Imported Diagram");
                app.pinstar_browser.error = None;
                app.pinstar_browser.mode = BrowserMode::ImportCanvas;
                true
            }
            ParsedInput::Byte(b'd') | ParsedInput::Char('d') => {
                if let Some(entry) = app.pinstar_browser.selected_entry() {
                    if entry.is_owner
                        || app
                            .permissions
                            .has(crate::moderation::policy::Caps::DELETE_PINSTAR_GRAPH)
                    {
                        app.pinstar_browser.delete_target_id = Some(entry.id);
                        app.pinstar_browser.mode = BrowserMode::ConfirmDelete;
                    } else {
                        app.pinstar_browser.error =
                            Some("Only owner or staff can delete diagrams".to_string());
                    }
                }
                true
            }
            ParsedInput::Byte(b'r') | ParsedInput::Char('r') => {
                if let Some(entry) = app.pinstar_browser.selected_entry() {
                    if entry.is_owner {
                        app.pinstar_browser.rename_input = entry.title.clone();
                        app.pinstar_browser.mode = BrowserMode::RenameInput;
                    } else {
                        app.pinstar_browser.error =
                            Some("Only owner can rename diagrams".to_string());
                    }
                }
                true
            }
            ParsedInput::Byte(b'c')
            | ParsedInput::Char('c')
            | ParsedInput::Byte(b'C')
            | ParsedInput::Char('C') => {
                if let Some(entry) = app.pinstar_browser.selected_entry() {
                    app.pinstar_browser.pending_action = Some(
                        crate::app::pinstar::browser::BrowserAction::CopySource(entry.id),
                    );
                }
                true
            }
            ParsedInput::Byte(b'\r')
            | ParsedInput::Byte(b'\n')
            | ParsedInput::Char('\r')
            | ParsedInput::Char('\n') => {
                if let Some(entry) = app.pinstar_browser.selected_entry() {
                    app.pinstar_browser.pending_action =
                        Some(crate::app::pinstar::browser::BrowserAction::Open(
                            entry.id,
                            entry.role.clone(),
                        ));
                }
                true
            }
            ParsedInput::Byte(b'i') | ParsedInput::Char('i') => {
                if let Some((entry_id, is_owner)) = app
                    .pinstar_browser
                    .selected_entry()
                    .map(|entry| (entry.id, entry.is_owner))
                {
                    if is_owner {
                        app.pinstar_browser.generated_invite_token = None;
                        app.pinstar_browser.error = None;
                        app.pinstar_browser.pending_action = Some(
                            crate::app::pinstar::browser::BrowserAction::GenerateInvite(entry_id),
                        );
                        app.pinstar_browser.mode =
                            crate::app::pinstar::browser::BrowserMode::GenerateInvite;
                    } else {
                        app.pinstar_browser.error =
                            Some("Only owner can create invite links".to_string());
                    }
                }
                true
            }
            _ => false,
        },
        BrowserMode::Help => {
            if matches!(
                event,
                ParsedInput::Byte(0x1b)
                    | ParsedInput::Byte(b'q')
                    | ParsedInput::Byte(b'Q')
                    | ParsedInput::Char('\x1b')
                    | ParsedInput::Char('q')
                    | ParsedInput::Char('Q')
            ) {
                app.pinstar_browser.mode = BrowserMode::List;
            }
            true
        }
        BrowserMode::AcceptInvite => match event {
            ParsedInput::Byte(0x1b) | ParsedInput::Char('\x1b') => {
                app.pinstar_browser.mode = BrowserMode::List;
                true
            }
            ParsedInput::Byte(b'\r')
            | ParsedInput::Byte(b'\n')
            | ParsedInput::Char('\r')
            | ParsedInput::Char('\n') => {
                let token = app.pinstar_browser.invite_token_input.clone();
                app.pinstar_browser.pending_action = Some(
                    crate::app::pinstar::browser::BrowserAction::AcceptInvite(token),
                );
                app.pinstar_browser.mode = BrowserMode::List;
                true
            }
            ParsedInput::Byte(0x7f)
            | ParsedInput::Byte(0x08)
            | ParsedInput::Char('\x08')
            | ParsedInput::Char('\x7f')
            | ParsedInput::Delete => {
                app.pinstar_browser.invite_token_input.pop();
                true
            }
            ParsedInput::Char(c) => {
                let _ = app.pinstar_browser.push_invite_token_char(*c);
                true
            }
            ParsedInput::Byte(byte) if !byte.is_ascii_control() && *byte != 0x7f => {
                let _ = app.pinstar_browser.push_invite_token_char(*byte as char);
                true
            }
            _ => false,
        },
        BrowserMode::ImportCanvas => match event {
            ParsedInput::Byte(0x1b) | ParsedInput::Char('\x1b') => {
                app.pinstar_browser.mode = BrowserMode::List;
                true
            }
            ParsedInput::Byte(b'\r')
            | ParsedInput::Byte(b'\n')
            | ParsedInput::Char('\r')
            | ParsedInput::Char('\n') => {
                let raw = app.pinstar_browser.import_input.trim().to_string();
                let name = if app.pinstar_browser.import_name.trim().is_empty() {
                    "Imported Diagram".to_string()
                } else {
                    app.pinstar_browser.import_name.trim().to_string()
                };
                match serde_json::from_str::<crate::app::pinstar::data::CanvasData>(&raw) {
                    Ok(data) => {
                        app.pinstar_browser.pending_action =
                            Some(crate::app::pinstar::browser::BrowserAction::Import {
                                title: name,
                                data,
                            });
                        app.pinstar_browser.mode = BrowserMode::List;
                        app.pinstar_browser.error = None;
                    }
                    Err(e) => {
                        app.pinstar_browser.error = Some(format!("Invalid canvas JSON: {}", e));
                    }
                }
                true
            }
            ParsedInput::Byte(0x7f)
            | ParsedInput::Byte(0x08)
            | ParsedInput::Char('\x08')
            | ParsedInput::Char('\x7f')
            | ParsedInput::Delete => {
                app.pinstar_browser.import_input.pop();
                true
            }
            ParsedInput::Char(c) => {
                app.pinstar_browser.import_input.push(*c);
                true
            }
            ParsedInput::Byte(byte) if !byte.is_ascii_control() && *byte != 0x7f => {
                app.pinstar_browser.import_input.push(*byte as char);
                true
            }
            _ => false,
        },
        BrowserMode::GenerateInvite => match event {
            ParsedInput::Byte(0x1b)
            | ParsedInput::Char('\x1b')
            | ParsedInput::Byte(b'c')
            | ParsedInput::Char('c') => {
                app.pinstar_browser.generated_invite_token = None;
                app.pinstar_browser.mode = BrowserMode::List;
                true
            }
            ParsedInput::Byte(b'\r')
            | ParsedInput::Byte(b'\n')
            | ParsedInput::Char('\r')
            | ParsedInput::Char('\n') => {
                if let Some(token) = &app.pinstar_browser.generated_invite_token {
                    app.pending_clipboard = Some(token.clone());
                    app.banner = Some(crate::app::common::primitives::Banner::success(
                        "Invite link copied to clipboard!",
                    ));
                }
                true
            }
            _ => true,
        },
        BrowserMode::ConfirmDelete => match event {
            ParsedInput::Byte(b'y')
            | ParsedInput::Char('y')
            | ParsedInput::Byte(b'Y')
            | ParsedInput::Char('Y') => {
                if let Some(id) = app.pinstar_browser.delete_target_id.take() {
                    app.pinstar_browser.pending_action =
                        Some(crate::app::pinstar::browser::BrowserAction::Delete(id));
                }
                app.pinstar_browser.mode = BrowserMode::List;
                true
            }
            ParsedInput::Byte(b'n')
            | ParsedInput::Char('n')
            | ParsedInput::Byte(b'N')
            | ParsedInput::Char('N')
            | ParsedInput::Byte(0x1b)
            | ParsedInput::Char('\x1b') => {
                app.pinstar_browser.delete_target_id = None;
                app.pinstar_browser.mode = BrowserMode::List;
                true
            }
            _ => true, // Consume all keys while in confirm mode
        },
        BrowserMode::RenameInput => match event {
            ParsedInput::Byte(0x1b) | ParsedInput::Char('\x1b') => {
                app.pinstar_browser.rename_input.clear();
                app.pinstar_browser.mode = BrowserMode::List;
                true
            }
            ParsedInput::Byte(b'\r')
            | ParsedInput::Byte(b'\n')
            | ParsedInput::Char('\r')
            | ParsedInput::Char('\n') => {
                if let Some(entry) = app.pinstar_browser.selected_entry() {
                    let new_title = app.pinstar_browser.rename_input.trim().to_string();
                    app.pinstar_browser.pending_action = Some(
                        crate::app::pinstar::browser::BrowserAction::Rename(entry.id, new_title),
                    );
                }
                app.pinstar_browser.rename_input.clear();
                app.pinstar_browser.mode = BrowserMode::List;
                true
            }
            ParsedInput::Byte(0x7f)
            | ParsedInput::Byte(0x08)
            | ParsedInput::Char('\x08')
            | ParsedInput::Char('\x7f')
            | ParsedInput::Delete => {
                app.pinstar_browser.rename_input.pop();
                true
            }
            ParsedInput::Char(c) => {
                app.pinstar_browser.rename_input.push(*c);
                true
            }
            // Control keys for text editing
            ParsedInput::Byte(0x15) => {
                // Ctrl+U: clear line
                app.pinstar_browser.rename_input.clear();
                true
            }
            ParsedInput::Byte(0x17) => {
                // Ctrl+W: delete last word
                while let Some(c) = app.pinstar_browser.rename_input.pop() {
                    if c.is_whitespace()
                        && !app
                            .pinstar_browser
                            .rename_input
                            .ends_with(|c: char| c.is_whitespace())
                    {
                        break;
                    }
                }
                true
            }
            // Printable ASCII range (space through ~)
            ParsedInput::Byte(b) if *b >= 0x20 && *b <= 0x7E => {
                app.pinstar_browser.rename_input.push(*b as char);
                true
            }
            _ => true, // Consume all keys while in rename mode
        },
        BrowserMode::CreateDiagram => match event {
            ParsedInput::Byte(0x1b) | ParsedInput::Char('\x1b') => {
                app.pinstar_browser.new_diagram_name.clear();
                app.pinstar_browser.mode = BrowserMode::List;
                true
            }
            ParsedInput::Byte(b'\r')
            | ParsedInput::Byte(b'\n')
            | ParsedInput::Char('\r')
            | ParsedInput::Char('\n') => {
                let title = app.pinstar_browser.new_diagram_name.trim().to_string();
                app.pinstar_browser.pending_action =
                    Some(crate::app::pinstar::browser::BrowserAction::Create { title });
                app.pinstar_browser.mode = BrowserMode::List;
                true
            }
            ParsedInput::Byte(0x7f)
            | ParsedInput::Byte(0x08)
            | ParsedInput::Char('\x08')
            | ParsedInput::Char('\x7f')
            | ParsedInput::Delete => {
                app.pinstar_browser.new_diagram_name.pop();
                true
            }
            ParsedInput::Char(c) => {
                app.pinstar_browser.new_diagram_name.push(*c);
                true
            }
            // Control keys for text editing
            ParsedInput::Byte(0x15) => {
                app.pinstar_browser.new_diagram_name.clear();
                true
            }
            ParsedInput::Byte(0x17) => {
                while let Some(c) = app.pinstar_browser.new_diagram_name.pop() {
                    if c.is_whitespace()
                        && !app
                            .pinstar_browser
                            .new_diagram_name
                            .ends_with(|c: char| c.is_whitespace())
                    {
                        break;
                    }
                }
                true
            }
            // Printable ASCII range (space through ~)
            ParsedInput::Byte(b) if *b >= 0x20 && *b <= 0x7E => {
                app.pinstar_browser.new_diagram_name.push(*b as char);
                true
            }
            _ => true, // Consume all keys while in create mode
        },
    }
}

pub(crate) fn try_open_icon_picker(app: &mut App) {
    let ctx = InputContext::from_app(app);
    // Only chat composers can receive icons.
    if !matches!(ctx.screen, Screen::Dashboard | Screen::Rooms) {
        return;
    }
    if !ctx.chat_composing {
        if ctx.screen == Screen::Dashboard {
            if let Some(room_id) = app.chat.selected_room_id {
                app.chat.start_composing_in_room(room_id);
            }
        } else if ctx.screen == Screen::Rooms {
            if let Some(room) = app.rooms_active_room.as_ref() {
                app.chat.start_composing_in_room(room.chat_room_id);
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
        ParsedInput::Home => app.icon_picker_state.search_cursor_home(),
        ParsedInput::End => app.icon_picker_state.search_cursor_end(),
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
        let Some(icon) = icon_picker::picker::selected_chat_icon(&app.icon_picker_state, catalog)
        else {
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
    if matches!(ctx.screen, Screen::Dashboard | Screen::Rooms) && ctx.chat_composing {
        for ch in icon_str.chars() {
            app.chat.composer_push(ch);
        }
        app.chat.update_autocomplete();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pure clone of the offset clamp + step logic from
    /// `handle_mouse_scroll_over_screen` so we can unit-test it without
    /// spinning up an `App`. Keep in sync with the call site.
    fn next_activity_scroll(
        current: u16,
        total: usize,
        has_active_friends: bool,
        delta: isize,
    ) -> u16 {
        let visible = activity_visible_event_rows(has_active_friends);
        let max_offset = total.saturating_sub(visible) as u16;
        let current = current.min(max_offset);
        if delta > 0 {
            current.saturating_sub(ACTIVITY_SCROLL_STEP)
        } else {
            current.saturating_add(ACTIVITY_SCROLL_STEP).min(max_offset)
        }
    }

    #[test]
    fn activity_scroll_wheel_up_decreases_offset_toward_newest() {
        // 20 events, currently at offset 5; wheel up moves toward newer.
        assert_eq!(next_activity_scroll(5, 20, true, 1), 4);
        // At top already → saturating subtract clamps at 0.
        assert_eq!(next_activity_scroll(0, 20, true, 1), 0);
    }

    #[test]
    fn activity_scroll_wheel_down_clamps_at_max_offset() {
        // 20 events with active friends, 3 event rows visible → max_offset = 17.
        assert_eq!(next_activity_scroll(17, 20, true, -1), 17);
        assert_eq!(next_activity_scroll(16, 20, true, -1), 17);
    }

    #[test]
    fn activity_scroll_uses_four_visible_rows_without_active_friends() {
        // No active-friends row means the renderer shows 4 activity events.
        assert_eq!(next_activity_scroll(16, 20, false, -1), 16);
        assert_eq!(next_activity_scroll(15, 20, false, -1), 16);
    }

    #[test]
    fn activity_scroll_zero_max_when_buffer_smaller_than_visible() {
        // Only 2 events in buffer; nothing to scroll past.
        assert_eq!(next_activity_scroll(0, 2, true, -1), 0);
        assert_eq!(next_activity_scroll(5, 2, false, -1), 0);
    }

    #[test]
    fn activity_scroll_clamps_stale_offset_after_buffer_trim() {
        // User was at offset 30 in a 100-event buffer; buffer trims to 10.
        // Next wheel event must clamp before stepping so we don't underflow.
        assert_eq!(next_activity_scroll(30, 10, true, 1), 6);
        assert_eq!(next_activity_scroll(30, 10, true, -1), 7);
        assert_eq!(next_activity_scroll(30, 10, false, 1), 5);
        assert_eq!(next_activity_scroll(30, 10, false, -1), 6);
    }

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
        assert_eq!(topbar_screen_hit_test(12, 0), Some(Screen::Dashboard));
        assert_eq!(topbar_screen_hit_test(14, 0), Some(Screen::Arcade));
        assert_eq!(topbar_screen_hit_test(16, 0), Some(Screen::Rooms));
        assert_eq!(topbar_screen_hit_test(18, 0), Some(Screen::DoorGames));
        assert_eq!(topbar_screen_hit_test(20, 0), Some(Screen::Artboard));
        assert_eq!(topbar_screen_hit_test(22, 0), Some(Screen::Pinstar));
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
    fn room_join_suffixes_are_one_based_digits() {
        assert_eq!(room_join_suffix_index(b'1'), Some(0));
        assert_eq!(room_join_suffix_index(b'2'), Some(1));
        assert_eq!(room_join_suffix_index(b'3'), Some(2));
        assert_eq!(room_join_suffix_index(b'4'), Some(3));
        assert_eq!(room_join_suffix_index(b'b'), None);
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
}
