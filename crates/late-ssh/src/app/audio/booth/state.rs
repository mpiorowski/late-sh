use uuid::Uuid;

use crate::app::audio::svc::{HistoryItemView, QueueItemView};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) enum BoothFocus {
    #[default]
    Submit,
    Queue,
    History,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct BoothModalState {
    open: bool,
    submit_input: String,
    selected_queue: usize,
    selected_history: usize,
    focus: BoothFocus,
}

impl BoothModalState {
    pub(crate) fn open(&mut self, submit_enabled: bool) {
        self.open = true;
        self.submit_input.clear();
        self.selected_queue = 0;
        self.selected_history = 0;
        self.focus = if submit_enabled {
            BoothFocus::Submit
        } else {
            BoothFocus::Queue
        };
    }

    pub(crate) fn close(&mut self) {
        self.open = false;
        self.submit_input.clear();
        self.selected_queue = 0;
        self.selected_history = 0;
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
        match self.focus {
            BoothFocus::Submit | BoothFocus::Queue => self.selected_queue,
            BoothFocus::History => self.selected_history,
        }
    }

    pub(crate) fn selected_queue(&self) -> usize {
        self.selected_queue
    }

    pub(crate) fn selected_history(&self) -> usize {
        self.selected_history
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
            self.set_selected_for_focus(0);
            return;
        }
        let current = self.selected();
        let next = (current as isize + delta).rem_euclid(len as isize) as usize;
        self.set_selected_for_focus(next);
    }

    pub(crate) fn clamp(&mut self, queue_len: usize, history_len: usize) {
        if queue_len == 0 {
            self.selected_queue = 0;
        } else {
            self.selected_queue = self.selected_queue.min(queue_len - 1);
        }
        if history_len == 0 {
            self.selected_history = 0;
        } else {
            self.selected_history = self.selected_history.min(history_len - 1);
        }
    }

    pub(crate) fn cycle_focus(&mut self, submit_enabled: bool) {
        self.focus = match self.focus {
            BoothFocus::Submit => BoothFocus::Queue,
            BoothFocus::Queue => BoothFocus::History,
            BoothFocus::History if submit_enabled => BoothFocus::Submit,
            BoothFocus::History => BoothFocus::Queue,
        };
    }

    pub(crate) fn set_focus(&mut self, focus: BoothFocus) {
        self.focus = focus;
    }

    pub(crate) fn selected_item<'a>(
        &self,
        queue: &'a [QueueItemView],
    ) -> Option<&'a QueueItemView> {
        queue.get(self.selected_queue)
    }

    pub(crate) fn selected_item_id(&self, queue: &[QueueItemView]) -> Option<Uuid> {
        self.selected_item(queue).map(|item| item.id)
    }

    pub(crate) fn selected_history_item<'a>(
        &self,
        history: &'a [HistoryItemView],
    ) -> Option<&'a HistoryItemView> {
        history.get(self.selected_history)
    }

    pub(crate) fn selected_history_item_id(&self, history: &[HistoryItemView]) -> Option<Uuid> {
        self.selected_history_item(history).map(|item| item.id)
    }

    fn set_selected_for_focus(&mut self, selected: usize) {
        match self.focus {
            BoothFocus::Submit | BoothFocus::Queue => self.selected_queue = selected,
            BoothFocus::History => self.selected_history = selected,
        }
    }
}
