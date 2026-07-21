use late_core::models::character_sheet::SHEET_NAME_MAX_CHARS;
use ratatui_textarea::{CursorMove, TextArea, WrapMode};
use uuid::Uuid;

use crate::app::chat::state::SheetOpenRequest;
use crate::app::common::composer::{new_themed_textarea, set_themed_textarea_cursor_visible};
use crate::app::common::textarea_input::char_count;

/// Which sheet field has focus (and, while editing, receives keystrokes).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SheetField {
    Name,
    Body,
}

/// Save handed from the modal to the app tick, which forwards it to
/// `ChatService::save_sheet_task` (same pump pattern as chat's `requested_*`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SheetSaveRequest {
    pub room_id: Uuid,
    pub name: String,
    pub body: String,
}

pub struct SheetModalState {
    room_id: Option<Uuid>,
    target_username: String,
    editable: bool,
    focus: SheetField,
    editing: bool,
    name_input: TextArea<'static>,
    body_input: TextArea<'static>,
    /// Last committed name, restored when a name edit is cancelled.
    committed_name: String,
    pending_save: Option<SheetSaveRequest>,
}

impl SheetModalState {
    pub fn new() -> Self {
        Self {
            room_id: None,
            target_username: String::new(),
            editable: false,
            focus: SheetField::Name,
            editing: false,
            name_input: new_name_textarea(),
            body_input: new_body_textarea(),
            committed_name: String::new(),
            pending_save: None,
        }
    }

    pub fn open(&mut self, request: SheetOpenRequest) {
        self.room_id = Some(request.room_id);
        self.target_username = request.target_username;
        self.editable = request.editable;
        self.focus = SheetField::Name;
        self.editing = false;
        self.committed_name = request.name;
        self.name_input = new_name_textarea();
        self.name_input.insert_str(&self.committed_name);
        self.body_input = new_body_textarea();
        self.body_input.insert_str(&request.body);
        scroll_to_top(&mut self.body_input);
        self.pending_save = None;
    }

    pub fn close(&mut self) {
        self.room_id = None;
        self.editing = false;
    }

    pub fn target_username(&self) -> &str {
        &self.target_username
    }

    pub fn editable(&self) -> bool {
        self.editable
    }

    pub fn focus(&self) -> SheetField {
        self.focus
    }

    pub fn editing(&self) -> bool {
        self.editing
    }

    pub fn name_input(&self) -> &TextArea<'static> {
        &self.name_input
    }

    pub fn body_input(&self) -> &TextArea<'static> {
        &self.body_input
    }

    pub fn name_input_mut(&mut self) -> &mut TextArea<'static> {
        &mut self.name_input
    }

    pub fn body_input_mut(&mut self) -> &mut TextArea<'static> {
        &mut self.body_input
    }

    pub fn name_text(&self) -> String {
        self.name_input.lines().join("")
    }

    pub fn body_text(&self) -> String {
        self.body_input.lines().join("\n")
    }

    /// Char count using the shared helper's limit accounting.
    pub fn body_char_count(&self) -> usize {
        char_count(&self.body_input)
    }

    pub fn toggle_focus(&mut self) {
        self.focus = match self.focus {
            SheetField::Name => SheetField::Body,
            SheetField::Body => SheetField::Name,
        };
    }

    pub fn set_focus(&mut self, field: SheetField) {
        self.focus = field;
    }

    pub fn start_edit(&mut self) {
        if !self.editable || self.editing {
            return;
        }
        self.editing = true;
        match self.focus {
            SheetField::Name => {
                self.name_input.move_cursor(CursorMove::End);
                set_themed_textarea_cursor_visible(&mut self.name_input, true);
            }
            SheetField::Body => {
                self.body_input.move_cursor(CursorMove::Bottom);
                self.body_input.move_cursor(CursorMove::End);
                set_themed_textarea_cursor_visible(&mut self.body_input, true);
            }
        }
    }

    /// Commit the focused field and queue a save. Called on Enter, and on Esc
    /// in the body field (bio convention: leaving the body edit commits).
    pub fn submit_edit(&mut self) {
        if !self.editing {
            return;
        }
        self.editing = false;
        if self.focus == SheetField::Name {
            let name = clamp_chars(self.name_text().trim(), SHEET_NAME_MAX_CHARS);
            self.committed_name = name;
            self.name_input = new_name_textarea();
            self.name_input.insert_str(&self.committed_name);
        } else {
            scroll_to_top(&mut self.body_input);
        }
        set_themed_textarea_cursor_visible(&mut self.name_input, false);
        set_themed_textarea_cursor_visible(&mut self.body_input, false);
        let Some(room_id) = self.room_id else {
            return;
        };
        self.pending_save = Some(SheetSaveRequest {
            room_id,
            name: self.committed_name.clone(),
            body: self.body_text().trim_end().to_string(),
        });
    }

    /// Revert a name edit (Esc on the name field). The body field never
    /// reverts; its Esc maps to `submit_edit` in the input layer.
    pub fn cancel_edit(&mut self) {
        if !self.editing {
            return;
        }
        self.editing = false;
        if self.focus == SheetField::Name {
            self.name_input = new_name_textarea();
            self.name_input.insert_str(&self.committed_name);
        }
        set_themed_textarea_cursor_visible(&mut self.name_input, false);
        set_themed_textarea_cursor_visible(&mut self.body_input, false);
    }

    pub fn take_pending_save(&mut self) -> Option<SheetSaveRequest> {
        self.pending_save.take()
    }

    pub fn scroll_body(&mut self, delta: i16) {
        let movement = if delta < 0 {
            CursorMove::Up
        } else {
            CursorMove::Down
        };
        for _ in 0..delta.unsigned_abs() {
            self.body_input.move_cursor(movement);
        }
    }
}

impl Default for SheetModalState {
    fn default() -> Self {
        Self::new()
    }
}

fn new_name_textarea() -> TextArea<'static> {
    new_themed_textarea("unnamed", WrapMode::None, false)
}

fn new_body_textarea() -> TextArea<'static> {
    new_themed_textarea(
        "press ↵ to write this character's story",
        WrapMode::Word,
        false,
    )
}

fn clamp_chars(text: &str, max_chars: usize) -> String {
    text.chars().take(max_chars).collect()
}

fn scroll_to_top(ta: &mut TextArea<'static>) {
    ta.move_cursor(CursorMove::Top);
    ta.move_cursor(CursorMove::Head);
}
