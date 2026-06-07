use late_core::models::chat_poll::{
    POLL_MAX_OPTIONS, POLL_MIN_OPTIONS, POLL_OPTION_MAX_CHARS, POLL_QUESTION_MAX_CHARS,
};
use ratatui_textarea::{TextArea, WrapMode};
use uuid::Uuid;

use crate::app::common::composer::{new_themed_textarea, set_themed_textarea_cursor_visible};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PollField {
    Question,
    Option(usize),
}

#[derive(Debug)]
pub(crate) struct PollSubmit {
    pub room_id: Uuid,
    pub question: String,
    pub options: Vec<String>,
}

#[derive(Debug)]
pub(crate) struct PollModalState {
    room_id: Option<Uuid>,
    focus: PollField,
    question: TextArea<'static>,
    options: [TextArea<'static>; POLL_MAX_OPTIONS],
}

impl PollModalState {
    pub(crate) fn new() -> Self {
        Self {
            room_id: None,
            focus: PollField::Question,
            question: new_input("Question"),
            options: [
                new_input("Option 1"),
                new_input("Option 2"),
                new_input("Option 3 (optional)"),
            ],
        }
    }

    pub(crate) fn open(&mut self, room_id: Uuid) {
        self.room_id = Some(room_id);
        self.focus = PollField::Question;
        self.question = new_input("Question");
        self.options = [
            new_input("Option 1"),
            new_input("Option 2"),
            new_input("Option 3 (optional)"),
        ];
        self.sync_cursor_visibility();
    }

    pub(crate) fn close(&mut self) {
        self.room_id = None;
    }

    pub(crate) fn is_open(&self) -> bool {
        self.room_id.is_some()
    }

    pub(crate) fn focus(&self) -> PollField {
        self.focus
    }

    pub(crate) fn question(&self) -> &TextArea<'static> {
        &self.question
    }

    pub(crate) fn options(&self) -> &[TextArea<'static>; POLL_MAX_OPTIONS] {
        &self.options
    }

    pub(crate) fn focused_input_mut(&mut self) -> &mut TextArea<'static> {
        match self.focus {
            PollField::Question => &mut self.question,
            PollField::Option(index) => &mut self.options[index],
        }
    }

    pub(crate) fn focused_max_chars(&self) -> usize {
        match self.focus {
            PollField::Question => POLL_QUESTION_MAX_CHARS,
            PollField::Option(_) => POLL_OPTION_MAX_CHARS,
        }
    }

    pub(crate) fn move_focus(&mut self, delta: isize) {
        let current = match self.focus {
            PollField::Question => 0,
            PollField::Option(index) => index + 1,
        };
        let next = (current as isize + delta).rem_euclid(1 + POLL_MAX_OPTIONS as isize) as usize;
        self.focus = if next == 0 {
            PollField::Question
        } else {
            PollField::Option(next - 1)
        };
        self.sync_cursor_visibility();
    }

    pub(crate) fn submit(&self) -> Result<PollSubmit, String> {
        let Some(room_id) = self.room_id else {
            return Err("Poll modal is not open".to_string());
        };
        let question = normalized_text(&self.question);
        if question.is_empty() {
            return Err("Add a question".to_string());
        }
        let options: Vec<String> = self
            .options
            .iter()
            .map(normalized_text)
            .filter(|text| !text.is_empty())
            .collect();
        if options.len() < POLL_MIN_OPTIONS {
            return Err("Add at least two options".to_string());
        }
        Ok(PollSubmit {
            room_id,
            question,
            options,
        })
    }

    fn sync_cursor_visibility(&mut self) {
        set_themed_textarea_cursor_visible(
            &mut self.question,
            matches!(self.focus, PollField::Question),
        );
        for (index, option) in self.options.iter_mut().enumerate() {
            set_themed_textarea_cursor_visible(
                option,
                matches!(self.focus, PollField::Option(active) if active == index),
            );
        }
    }
}

fn new_input(placeholder: &str) -> TextArea<'static> {
    new_themed_textarea(placeholder, WrapMode::None, false)
}

fn normalized_text(input: &TextArea<'static>) -> String {
    input.lines().join(" ").trim().to_string()
}
