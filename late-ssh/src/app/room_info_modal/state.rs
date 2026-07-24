use ratatui::style::Style;
use ratatui_textarea::{CursorMove, TextArea, WrapMode};
use uuid::Uuid;

/// Field length caps. Title is a header line; about/rules are short blurbs.
pub(crate) const TITLE_MAX: usize = 60;
pub(crate) const ABOUT_MAX: usize = 240;
pub(crate) const RULES_MAX: usize = 400;

/// Which field the cursor is in.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(crate) enum Field {
    #[default]
    Title,
    About,
    Rules,
}

impl Field {
    fn next(self) -> Self {
        match self {
            Field::Title => Field::About,
            Field::About => Field::Rules,
            Field::Rules => Field::Title,
        }
    }

    fn prev(self) -> Self {
        match self {
            Field::Title => Field::Rules,
            Field::About => Field::Title,
            Field::Rules => Field::About,
        }
    }

    fn max_len(self) -> usize {
        match self {
            Field::Title => TITLE_MAX,
            Field::About => ABOUT_MAX,
            Field::Rules => RULES_MAX,
        }
    }
}

/// What the form will do on submit.
#[derive(Clone, Debug)]
pub(crate) enum Mode {
    /// Creating a brand-new room from a `/public` or `/private` command.
    Create { is_private: bool, slug: String },
    /// Editing the info of an existing room the user owns.
    Edit { room_id: Uuid },
}

/// A single-line editable field.
fn field_input() -> TextArea<'static> {
    let mut ta = TextArea::default();
    ta.set_cursor_line_style(Style::default());
    ta.set_wrap_mode(WrapMode::None);
    ta
}

fn seed(text: Option<&str>) -> TextArea<'static> {
    let mut ta = field_input();
    if let Some(text) = text.map(str::trim).filter(|s| !s.is_empty()) {
        ta.insert_str(text);
    }
    ta
}

fn line_text(ta: &TextArea<'static>) -> String {
    ta.lines().first().cloned().unwrap_or_default()
}

fn char_count(ta: &TextArea<'static>) -> usize {
    ta.lines().first().map(|l| l.chars().count()).unwrap_or(0)
}

/// The room-info form. Default is closed.
#[derive(Default)]
pub(crate) struct RoomInfoModalState {
    open: bool,
    mode: Option<Mode>,
    focus: Field,
    title: TextArea<'static>,
    about: TextArea<'static>,
    rules: TextArea<'static>,
    status: Option<String>,
}

impl RoomInfoModalState {
    pub(crate) fn is_open(&self) -> bool {
        self.open
    }

    pub(crate) fn mode(&self) -> Option<&Mode> {
        self.mode.as_ref()
    }

    pub(crate) fn focus(&self) -> Field {
        self.focus
    }

    pub(crate) fn status(&self) -> Option<&str> {
        self.status.as_deref()
    }

    pub(crate) fn field(&self, field: Field) -> &TextArea<'static> {
        match field {
            Field::Title => &self.title,
            Field::About => &self.about,
            Field::Rules => &self.rules,
        }
    }

    /// Open the form to create a room. `suggested_title` pre-fills the name from
    /// the slug the user typed, so hitting Enter straight away still works.
    pub(crate) fn open_create(&mut self, is_private: bool, slug: String, suggested_title: &str) {
        self.mode = Some(Mode::Create { is_private, slug });
        self.title = seed(Some(suggested_title));
        self.about = field_input();
        self.rules = field_input();
        self.focus = Field::Title;
        self.title.move_cursor(CursorMove::End);
        self.status = None;
        self.open = true;
    }

    /// Open the form to edit an existing room's info.
    pub(crate) fn open_edit(
        &mut self,
        room_id: Uuid,
        title: Option<&str>,
        about: Option<&str>,
        rules: Option<&str>,
    ) {
        self.mode = Some(Mode::Edit { room_id });
        self.title = seed(title);
        self.about = seed(about);
        self.rules = seed(rules);
        self.focus = Field::Title;
        self.title.move_cursor(CursorMove::End);
        self.status = None;
        self.open = true;
    }

    pub(crate) fn close(&mut self) {
        self.open = false;
        self.mode = None;
        self.status = None;
        self.title = field_input();
        self.about = field_input();
        self.rules = field_input();
        self.focus = Field::Title;
    }

    pub(crate) fn focus_next(&mut self) {
        self.focus = self.focus.next();
    }

    pub(crate) fn focus_prev(&mut self) {
        self.focus = self.focus.prev();
    }

    fn active_mut(&mut self) -> &mut TextArea<'static> {
        match self.focus {
            Field::Title => &mut self.title,
            Field::About => &mut self.about,
            Field::Rules => &mut self.rules,
        }
    }

    pub(crate) fn push(&mut self, ch: char) {
        if ch.is_control() {
            return;
        }
        let cap = self.focus.max_len();
        let ta = self.active_mut();
        if char_count(ta) < cap {
            ta.insert_char(ch);
            self.status = None;
        }
    }

    pub(crate) fn backspace(&mut self) {
        self.active_mut().delete_char();
    }

    pub(crate) fn delete_forward(&mut self) {
        self.active_mut().delete_next_char();
    }

    pub(crate) fn cursor_left(&mut self) {
        self.active_mut().move_cursor(CursorMove::Back);
    }

    pub(crate) fn cursor_right(&mut self) {
        self.active_mut().move_cursor(CursorMove::Forward);
    }

    pub(crate) fn cursor_home(&mut self) {
        self.active_mut().move_cursor(CursorMove::Head);
    }

    pub(crate) fn cursor_end(&mut self) {
        self.active_mut().move_cursor(CursorMove::End);
    }

    pub(crate) fn clear_active(&mut self) {
        match self.focus {
            Field::Title => self.title = field_input(),
            Field::About => self.about = field_input(),
            Field::Rules => self.rules = field_input(),
        }
    }

    pub(crate) fn set_status(&mut self, msg: impl Into<String>) {
        self.status = Some(msg.into());
    }

    /// The trimmed values. A name is required; about/rules may be empty.
    pub(crate) fn values(&self) -> (String, String, String) {
        (
            line_text(&self.title).trim().to_string(),
            line_text(&self.about).trim().to_string(),
            line_text(&self.rules).trim().to_string(),
        )
    }
}
