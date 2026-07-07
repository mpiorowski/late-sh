use late_core::models::chat_poll::{
    POLL_DURATION_OPTIONS_SECS, POLL_MAX_OPTIONS, POLL_MIN_OPTIONS, POLL_OPTION_MAX_CHARS,
    POLL_QUESTION_MAX_CHARS,
};
use ratatui_textarea::{TextArea, WrapMode};
use uuid::Uuid;

use crate::app::common::composer::{new_themed_textarea, set_themed_textarea_cursor_visible};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PollField {
    Question,
    Option(usize),
    Duration,
}

#[derive(Debug)]
pub(crate) struct PollSubmit {
    pub room_id: Uuid,
    pub question: String,
    pub options: Vec<String>,
    pub duration_secs: i64,
}

#[derive(Debug)]
pub(crate) struct PollModalState {
    room_id: Option<Uuid>,
    focus: PollField,
    question: TextArea<'static>,
    options: [TextArea<'static>; POLL_MAX_OPTIONS],
    duration_index: usize,
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
            duration_index: 0,
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
        self.duration_index = 0;
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
            PollField::Duration => unreachable!("duration field has no text input"),
        }
    }

    pub(crate) fn focused_max_chars(&self) -> usize {
        match self.focus {
            PollField::Question => POLL_QUESTION_MAX_CHARS,
            PollField::Option(_) => POLL_OPTION_MAX_CHARS,
            PollField::Duration => 0,
        }
    }

    pub(crate) fn move_focus(&mut self, delta: isize) {
        let current = match self.focus {
            PollField::Question => 0,
            PollField::Option(index) => index + 1,
            PollField::Duration => 1 + POLL_MAX_OPTIONS,
        };
        let field_count = 2 + POLL_MAX_OPTIONS as isize;
        let next = (current as isize + delta).rem_euclid(field_count) as usize;
        self.focus = if next == 0 {
            PollField::Question
        } else if next == 1 + POLL_MAX_OPTIONS {
            PollField::Duration
        } else {
            PollField::Option(next - 1)
        };
        self.sync_cursor_visibility();
    }

    pub(crate) fn move_duration(&mut self, delta: isize) {
        self.duration_index = (self.duration_index as isize + delta)
            .rem_euclid(POLL_DURATION_OPTIONS_SECS.len() as isize)
            as usize;
    }

    pub(crate) fn set_duration_index(&mut self, index: usize) {
        if index < POLL_DURATION_OPTIONS_SECS.len() {
            self.duration_index = index;
        }
    }

    pub(crate) fn duration_index(&self) -> usize {
        self.duration_index
    }

    pub(crate) fn duration_options_secs(&self) -> &'static [i64] {
        &POLL_DURATION_OPTIONS_SECS
    }

    pub(crate) fn duration_secs(&self) -> i64 {
        POLL_DURATION_OPTIONS_SECS[self.duration_index]
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
            duration_secs: self.duration_secs(),
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
