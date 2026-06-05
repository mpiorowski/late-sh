use late_core::models::character_sheet::SHEET_NAME_MAX_CHARS;
use ratatui_textarea::{CursorMove, TextArea, WrapMode};
use uuid::Uuid;

use crate::app::chat::state::SheetOpenRequest;
use crate::app::common::composer::{new_themed_textarea, set_themed_textarea_cursor_visible};

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

    /// Char count matching the shared helper's limit accounting (newlines
    /// between rows count as one char each).
    pub fn body_char_count(&self) -> usize {
        self.body_input
            .lines()
            .iter()
            .map(|l| l.chars().count())
            .sum::<usize>()
            + self.body_input.lines().len().saturating_sub(1)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn request(editable: bool) -> SheetOpenRequest {
        SheetOpenRequest {
            room_id: Uuid::from_u128(7),
            target_username: "frodo".to_string(),
            name: "Frodo".to_string(),
            body: "Ring bearer".to_string(),
            editable,
        }
    }

    #[test]
    fn open_populates_fields() {
        let mut state = SheetModalState::new();
        state.open(request(true));
        assert_eq!(state.target_username(), "frodo");
        assert_eq!(state.name_text(), "Frodo");
        assert_eq!(state.body_text(), "Ring bearer");
        assert!(state.editable());
        assert!(!state.editing());
        assert_eq!(state.take_pending_save(), None);
    }

    #[test]
    fn read_only_sheet_blocks_editing() {
        let mut state = SheetModalState::new();
        state.open(request(false));
        state.start_edit();
        assert!(!state.editing());
    }

    #[test]
    fn name_submit_commits_and_queues_save() {
        let mut state = SheetModalState::new();
        state.open(request(true));
        state.start_edit();
        state.name_input_mut().insert_str(" Baggins");
        state.submit_edit();
        assert!(!state.editing());
        assert_eq!(state.name_text(), "Frodo Baggins");
        let save = state.take_pending_save().expect("queued save");
        assert_eq!(save.room_id, Uuid::from_u128(7));
        assert_eq!(save.name, "Frodo Baggins");
        assert_eq!(save.body, "Ring bearer");
    }

    #[test]
    fn name_cancel_reverts_to_committed_value() {
        let mut state = SheetModalState::new();
        state.open(request(true));
        state.start_edit();
        state.name_input_mut().insert_str(" the Brave");
        state.cancel_edit();
        assert_eq!(state.name_text(), "Frodo");
        assert_eq!(state.take_pending_save(), None);
    }

    #[test]
    fn body_submit_queues_save_with_trimmed_body() {
        let mut state = SheetModalState::new();
        state.open(request(true));
        state.set_focus(SheetField::Body);
        state.start_edit();
        state.body_input_mut().insert_str(" of the Shire\n\n");
        state.submit_edit();
        let save = state.take_pending_save().expect("queued save");
        assert_eq!(save.body, "Ring bearer of the Shire");
    }

    #[test]
    fn submitted_name_is_clamped_to_max_chars() {
        let mut state = SheetModalState::new();
        state.open(request(true));
        state.start_edit();
        let long: String = "x".repeat(SHEET_NAME_MAX_CHARS * 2);
        state.name_input_mut().insert_str(&long);
        state.submit_edit();
        assert!(state.name_text().chars().count() <= SHEET_NAME_MAX_CHARS);
    }

    #[test]
    fn toggle_focus_switches_fields() {
        let mut state = SheetModalState::new();
        state.open(request(true));
        assert_eq!(state.focus(), SheetField::Name);
        state.toggle_focus();
        assert_eq!(state.focus(), SheetField::Body);
        state.toggle_focus();
        assert_eq!(state.focus(), SheetField::Name);
    }

    #[test]
    fn reopen_resets_state_and_drops_stale_pending_save() {
        let mut state = SheetModalState::new();
        state.open(request(true));
        state.start_edit();
        state.name_input_mut().insert_str("!");
        state.submit_edit();
        assert!(state.take_pending_save().is_some());

        state.start_edit();
        state.name_input_mut().insert_str("?");
        state.submit_edit();
        // Re-open before the pump consumed the queued save: it must be dropped.
        let mut second = request(false);
        second.target_username = "sam".to_string();
        second.name = "Sam".to_string();
        second.body = "Gardener".to_string();
        state.open(second);
        assert_eq!(state.take_pending_save(), None);
        assert_eq!(state.target_username(), "sam");
        assert_eq!(state.name_text(), "Sam");
        assert_eq!(state.body_text(), "Gardener");
        assert!(!state.editable());
        assert!(!state.editing());
    }

    #[test]
    fn close_keeps_queued_save_for_the_tick_pump() {
        let mut state = SheetModalState::new();
        state.open(request(true));
        state.start_edit();
        state.name_input_mut().insert_str("!");
        state.submit_edit();
        state.close();
        // The user's last edit must still reach the pump after close.
        assert!(state.take_pending_save().is_some());
    }
}
