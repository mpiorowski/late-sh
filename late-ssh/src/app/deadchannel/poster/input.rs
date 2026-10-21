//! Keys while PRINTS's panel is open. The poster is a typing surface, the
//! Artboard's rule: every printable key paints at the cursor (digits, `q`,
//! and `?` included), so the shop's commands are control chords. Arrows
//! move, Shift+arrows select, Enter goes a row down, Backspace and Delete
//! erase; `^C` / `^X` / `^V` copy, cut, paste; `^K` the next tint, or the
//! colour picker on every colour; `^]` the app's glyph picker; `^S` saves;
//! `^D` arms the burn and a second `^D` burns, any other key disarming it.
//! A click moves the cursor, a drag selects. Tab stays global; everything
//! else is swallowed. A lone Esc never arrives here: the root's
//! `dispatch_escape` calls `handle_escape`, which backs out one step at a
//! time (the picker, the burn, the selection, then the panel).

use dartboard_editor::{AppModifiers, AppPointerButton, AppPointerEvent, AppPointerKind, MoveDir};

use super::state::{Draft, Edit};
use crate::app::common::color_picker::{self, COARSE_STEP, ColorPicker, Edge};
use crate::app::common::primitives::Screen;
use crate::app::deadchannel::city::map::Landmark;
use crate::app::input::{MouseButton, MouseEvent, MouseEventKind, ParsedInput};
use crate::app::state::{App, IconPickerTarget};

const CTRL_C: u8 = 0x03;
const CTRL_D: u8 = 0x04;
const CTRL_K: u8 = 0x0B;
const CTRL_S: u8 = 0x13;
const CTRL_V: u8 = 0x16;
const CTRL_X: u8 = 0x18;
/// Ctrl+] (Ctrl+5, raw GS): the glyph picker, as on the Artboard.
const CTRL_RIGHT_BRACKET: u8 = 0x1D;

/// What one event at the press does.
enum Key {
    Edit(Edit),
    /// Text from a terminal paste, control characters dropped.
    Lay(String),
    Point(AppPointerEvent),
    Colour,
    Glyphs,
    Save,
    Burn,
    Unbound,
}

/// PRINTS holds the keys: its panel is open on the page. The root asks
/// before spending a chord the poster binds (`^V` is the voice join
/// everywhere else).
pub fn holds_keys(app: &App) -> bool {
    app.screen == Screen::City && app.city.panel() == Some(Landmark::Prints)
}

pub fn handle_event(app: &mut App, event: &ParsedInput) -> bool {
    if let ParsedInput::Byte(b'\t') = event {
        return false;
    }
    let Some(draft) = app.poster.draft_mut() else {
        // Nothing on the bench: Enter leaves, like every counter.
        if let ParsedInput::Byte(b'\r' | b'\n') = event {
            app.city.dismiss();
            app.poster.close();
        }
        return true;
    };
    if draft.picker().is_some() {
        match event {
            ParsedInput::Byte(b'\r' | b'\n') => draft.apply_picker(),
            ParsedInput::Byte(CTRL_K) => draft.close_picker(),
            ParsedInput::Mouse(mouse) => {
                if let Some(hit) = picker_click(draft, mouse) {
                    open_picker(draft).click(hit);
                }
            }
            _ => picker_key(open_picker(draft), event),
        }
        return true;
    }
    let key = key(event);
    // Any key but the burn's own leans back from it; the mouse is no key.
    if !matches!(key, Key::Burn) && !matches!(event, ParsedInput::Mouse(_)) {
        draft.disarm_burn();
    }
    match key {
        Key::Edit(edit) => draft.edit(edit),
        Key::Lay(text) => draft.lay_text(&text),
        Key::Point(pointer) => draft.point(pointer),
        Key::Colour => draft.colour_key(),
        Key::Glyphs => crate::app::input::open_icon_picker(app, IconPickerTarget::Poster),
        Key::Save => app.poster.save(),
        Key::Burn => burn(app),
        Key::Unbound => {}
    }
    true
}

/// Esc at the press: the picker shuts, then the burn disarms, then the
/// selection drops, then the panel closes (an unsaved draft with it).
pub fn handle_escape(app: &mut App) {
    if let Some(draft) = app.poster.draft_mut() {
        if draft.picker().is_some() {
            draft.close_picker();
            return;
        }
        if draft.burn_armed {
            draft.disarm_burn();
            return;
        }
        if draft.selecting() {
            draft.edit(Edit::Deselect);
            return;
        }
    }
    app.city.dismiss();
    app.poster.close();
}

/// A glyph chosen in the app's picker (`IconPickerTarget::Poster`), laid
/// at the cursor like a typed key.
pub fn lay_glyph(app: &mut App, glyph: &str) {
    if let Some(draft) = app.poster.draft_mut() {
        draft.lay_text(glyph);
    }
}

/// The first `^D` arms, the second burns. A bare wall has nothing to burn.
fn burn(app: &mut App) {
    let hung = app.poster.hung;
    let Some(draft) = app.poster.draft_mut() else {
        return;
    };
    match (hung, draft.burn_armed) {
        (false, _) => app.poster.word = Some("nothing hangs on your wall to burn.".to_string()),
        (true, false) => draft.arm_burn(),
        (true, true) => {
            draft.disarm_burn();
            app.poster.burn();
        }
    }
}

fn key(event: &ParsedInput) -> Key {
    match event {
        ParsedInput::Arrow(code) => match move_dir(*code) {
            Some(dir) => Key::Edit(Edit::Move(dir)),
            None => Key::Unbound,
        },
        ParsedInput::ShiftArrow(code) => match move_dir(*code) {
            Some(dir) => Key::Edit(Edit::Extend(dir)),
            None => Key::Unbound,
        },
        ParsedInput::Byte(b'\r' | b'\n') => Key::Edit(Edit::NextLine),
        ParsedInput::Byte(0x7f | 0x08) => Key::Edit(Edit::EraseBack),
        ParsedInput::Delete => Key::Edit(Edit::EraseHere),
        ParsedInput::Byte(CTRL_C) => Key::Edit(Edit::Copy),
        ParsedInput::Byte(CTRL_X) => Key::Edit(Edit::Cut),
        ParsedInput::Byte(CTRL_V) => Key::Edit(Edit::Paste),
        ParsedInput::Byte(CTRL_K) => Key::Colour,
        ParsedInput::Byte(CTRL_RIGHT_BRACKET) => Key::Glyphs,
        ParsedInput::Byte(CTRL_S) => Key::Save,
        ParsedInput::Byte(CTRL_D) => Key::Burn,
        // Typed keys arrive as `Char`, control bytes never do.
        ParsedInput::Char(ch) => Key::Edit(Edit::Type(*ch)),
        ParsedInput::Paste(bytes) => Key::Lay(
            String::from_utf8_lossy(bytes)
                .chars()
                .filter(|ch| *ch == '\n' || !ch.is_control())
                .collect(),
        ),
        ParsedInput::Mouse(mouse) => match pointer(mouse) {
            Some(pointer) => Key::Point(pointer),
            None => Key::Unbound,
        },
        _ => Key::Unbound,
    }
}

fn move_dir(code: u8) -> Option<MoveDir> {
    match code {
        b'A' => Some(MoveDir::Up),
        b'B' => Some(MoveDir::Down),
        b'C' => Some(MoveDir::Right),
        b'D' => Some(MoveDir::Left),
        _ => None,
    }
}

/// The left button on the canvas, in 0-based frame cells; every other
/// pointer event is nothing here.
fn pointer(mouse: &MouseEvent) -> Option<AppPointerEvent> {
    let kind = match (mouse.kind, mouse.button) {
        (MouseEventKind::Down, Some(MouseButton::Left)) => {
            AppPointerKind::Down(AppPointerButton::Left)
        }
        (MouseEventKind::Drag, Some(MouseButton::Left)) => {
            AppPointerKind::Drag(AppPointerButton::Left)
        }
        (MouseEventKind::Up, Some(MouseButton::Left)) => AppPointerKind::Up(AppPointerButton::Left),
        _ => return None,
    };
    Some(AppPointerEvent {
        column: mouse.x.checked_sub(1)?,
        row: mouse.y.checked_sub(1)?,
        kind,
        modifiers: AppModifiers::default(),
    })
}

/// A left press on the picker, drawn over the panel body the draft was
/// last drawn into.
fn picker_click(draft: &Draft, mouse: &MouseEvent) -> Option<color_picker::Hit> {
    if !matches!(
        (mouse.kind, mouse.button),
        (MouseEventKind::Down, Some(MouseButton::Left))
    ) {
        return None;
    }
    color_picker::hit(
        draft.drawn_at()?,
        mouse.x.checked_sub(1)?,
        mouse.y.checked_sub(1)?,
    )
}

fn open_picker(draft: &mut Draft) -> &mut ColorPicker {
    draft
        .picker_mut()
        .expect("the picker's routes take keys only while it is open")
}

/// The picker's keys: the rows, the nudges, the hex field.
fn picker_key(picker: &mut ColorPicker, event: &ParsedInput) {
    match event {
        ParsedInput::Arrow(b'A') => picker.move_row(-1),
        ParsedInput::Arrow(b'B') => picker.move_row(1),
        ParsedInput::Arrow(b'C') => picker.adjust(1),
        ParsedInput::Arrow(b'D') => picker.adjust(-1),
        ParsedInput::ShiftArrow(b'C') => picker.adjust(COARSE_STEP),
        ParsedInput::ShiftArrow(b'D') => picker.adjust(-COARSE_STEP),
        ParsedInput::Home => picker.jump(Edge::Min),
        ParsedInput::End => picker.jump(Edge::Max),
        ParsedInput::Byte(0x7f | 0x08) => picker.hex_backspace(),
        ParsedInput::Char(ch) => {
            picker.type_hex(*ch);
        }
        _ => {}
    }
}
