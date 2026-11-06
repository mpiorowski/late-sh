use ratatui_textarea::{CursorMove, TextArea, WrapMode};
use uuid::Uuid;

use crate::app::common::composer::{new_themed_textarea, set_themed_textarea_cursor_visible};

/// Field length caps. The topic is one line (it also becomes the IRC topic,
/// which tops out at 300); rules get more room.
pub(crate) const TOPIC_MAX: usize = 140;
pub(crate) const RULES_MAX: usize = 400;

/// Which field the cursor is in.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(crate) enum Field {
    #[default]
    Topic,
    Rules,
}

impl Field {
    fn other(self) -> Self {
        match self {
            Field::Topic => Field::Rules,
            Field::Rules => Field::Topic,
        }
    }

    pub(crate) fn max_len(self) -> usize {
        match self {
            Field::Topic => TOPIC_MAX,
            Field::Rules => RULES_MAX,
        }
    }
}

/// What the form will do on submit.
#[derive(Clone, Debug)]
pub(crate) enum Mode {
    /// Opening a new private room from `/private`.
    Create { slug: String },
    /// Setting an existing room's info from `/roominfo`.
    Edit { room_id: Uuid },
}

/// The topic is one line; the rules are a short block, so they wrap and take
/// Alt+Enter for a new line (the bio convention).
fn topic_input() -> TextArea<'static> {
    new_themed_textarea("what this room is about", WrapMode::None, false)
}

fn rules_input() -> TextArea<'static> {
    new_themed_textarea(
        "the general rules (Alt+Enter for a new line)",
        WrapMode::Word,
        false,
    )
}

fn seed(mut ta: TextArea<'static>, text: Option<&str>) -> TextArea<'static> {
    if let Some(text) = text.map(str::trim).filter(|s| !s.is_empty()) {
        ta.insert_str(text);
    }
    ta
}

/// The room-info form. Default is closed.
#[derive(Default)]
pub(crate) struct RoomInfoModalState {
    open: bool,
    mode: Option<Mode>,
    focus: Field,
    /// The room this form is about, shown in the title and never editable.
    room_label: String,
    /// Who holds the room: "you", a username, or "moderators".
    owner_label: String,
    topic: TextArea<'static>,
    rules: TextArea<'static>,
    /// Screen rects of the two fields (Topic, Rules), recorded each frame so a
    /// click can focus the field under the pointer.
    field_rects: std::cell::Cell<[Option<ratatui::layout::Rect>; 2]>,
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

    pub(crate) fn set_focus(&mut self, field: Field) {
        self.focus = field;
    }

    /// Record a field's screen rect during draw (for click-to-focus).
    pub(crate) fn record_field_rect(&self, field: Field, rect: ratatui::layout::Rect) {
        let mut rects = self.field_rects.get();
        rects[field as usize] = Some(rect);
        self.field_rects.set(rects);
    }

    /// The field whose rect contains `(x, y)`, if a click landed on one.
    pub(crate) fn field_at(&self, x: u16, y: u16) -> Option<Field> {
        let rects = self.field_rects.get();
        for (i, rect) in rects.iter().enumerate() {
            if let Some(r) = rect
                && x >= r.x
                && x < r.x + r.width
                && y >= r.y
                && y < r.y + r.height
            {
                return Some(if i == 0 { Field::Topic } else { Field::Rules });
            }
        }
        None
    }

    pub(crate) fn room_label(&self) -> &str {
        &self.room_label
    }

    pub(crate) fn owner_label(&self) -> &str {
        &self.owner_label
    }

    pub(crate) fn field(&self, field: Field) -> &TextArea<'static> {
        match field {
            Field::Topic => &self.topic,
            Field::Rules => &self.rules,
        }
    }

    pub(crate) fn field_mut(&mut self, field: Field) -> &mut TextArea<'static> {
        match field {
            Field::Topic => &mut self.topic,
            Field::Rules => &mut self.rules,
        }
    }

    /// Open the form for a private room about to be created. The creator owns
    /// it, so the owner line reads "you" before the room even exists.
    pub(crate) fn open_create(&mut self, slug: String) {
        self.room_label = format!("#{slug}");
        self.mode = Some(Mode::Create { slug });
        self.owner_label = "you".to_string();
        self.topic = topic_input();
        self.rules = rules_input();
        self.reset_focus();
        self.open = true;
    }

    /// Open the form for an existing room.
    pub(crate) fn open_edit(
        &mut self,
        room_id: Uuid,
        room_label: String,
        owner_label: String,
        topic: Option<&str>,
        rules: Option<&str>,
    ) {
        self.mode = Some(Mode::Edit { room_id });
        self.room_label = room_label;
        self.owner_label = owner_label;
        self.topic = seed(topic_input(), topic);
        self.rules = seed(rules_input(), rules);
        self.reset_focus();
        self.open = true;
    }

    fn reset_focus(&mut self) {
        self.focus = Field::Topic;
        self.topic.move_cursor(CursorMove::End);
        self.rules.move_cursor(CursorMove::End);
        self.sync_cursors();
    }

    /// Only the focused field shows a cursor, so the form reads like the other
    /// multi-field modals.
    fn sync_cursors(&mut self) {
        let focus = self.focus;
        set_themed_textarea_cursor_visible(&mut self.topic, focus == Field::Topic);
        set_themed_textarea_cursor_visible(&mut self.rules, focus == Field::Rules);
    }

    pub(crate) fn close(&mut self) {
        self.open = false;
        self.mode = None;
        self.room_label = String::new();
        self.owner_label = String::new();
        self.topic = topic_input();
        self.rules = rules_input();
        self.focus = Field::Topic;
    }

    /// Two fields, so next and previous are the same move.
    pub(crate) fn toggle_focus(&mut self) {
        self.focus = self.focus.other();
        self.sync_cursors();
    }

    /// The trimmed values, empty when unset. Both fields are optional: a room
    /// with neither simply has no info, which is how every room starts.
    pub(crate) fn values(&self) -> (String, String) {
        (
            self.topic.lines().join(" ").trim().to_string(),
            self.rules.lines().join("\n").trim().to_string(),
        )
    }

    /// Characters used in a field, matching the accounting the caps use.
    pub(crate) fn used(&self, field: Field) -> usize {
        crate::app::common::textarea_input::char_count(self.field(field))
    }
}
