use uuid::Uuid;

use crate::app::audio::svc::QueueItemView;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) enum BoothFocus {
    #[default]
    Submit,
    Queue,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct BoothModalState {
    open: bool,
    submit_input: String,
    selected: usize,
    focus: BoothFocus,
}

impl BoothModalState {
    pub(crate) fn open(&mut self, submit_enabled: bool) {
        self.open = true;
        self.submit_input.clear();
        self.selected = 0;
        self.focus = if submit_enabled {
            BoothFocus::Submit
        } else {
            BoothFocus::Queue
        };
    }

    pub(crate) fn close(&mut self) {
        self.open = false;
        self.submit_input.clear();
        self.selected = 0;
        self.focus = BoothFocus::Submit;
    }

    pub(crate) fn is_open(&self) -> bool {
        self.open
    }

    pub(crate) fn submit_input(&self) -> &str {
        &self.submit_input
    }

    pub(crate) fn focus(&self) -> BoothFocus {
        self.focus
    }

    pub(crate) fn selected(&self) -> usize {
        self.selected
    }

    pub(crate) fn push(&mut self, ch: char) {
        if !ch.is_control() {
            self.submit_input.push(ch);
        }
    }

    pub(crate) fn backspace(&mut self) {
        self.submit_input.pop();
    }

    pub(crate) fn take_input(&mut self) -> String {
        std::mem::take(&mut self.submit_input)
    }

    pub(crate) fn move_selection(&mut self, delta: isize, len: usize) {
        if len == 0 {
            self.selected = 0;
            return;
        }
        let next = (self.selected as isize + delta).rem_euclid(len as isize) as usize;
        self.selected = next;
    }

    pub(crate) fn clamp(&mut self, len: usize) {
        if len == 0 {
            self.selected = 0;
        } else {
            self.selected = self.selected.min(len - 1);
        }
    }

    pub(crate) fn toggle_focus(&mut self) {
        self.focus = match self.focus {
            BoothFocus::Submit => BoothFocus::Queue,
            BoothFocus::Queue => BoothFocus::Submit,
        };
    }

    pub(crate) fn set_focus(&mut self, focus: BoothFocus) {
        self.focus = focus;
    }

    pub(crate) fn selected_item<'a>(&self, queue: &'a [QueueItemView]) -> Option<&'a QueueItemView> {
        queue.get(self.selected)
    }

    pub(crate) fn selected_item_id(&self, queue: &[QueueItemView]) -> Option<Uuid> {
        self.selected_item(queue).map(|item| item.id)
    }
}
