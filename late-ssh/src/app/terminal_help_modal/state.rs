use super::data::{TerminalHelpTopic, lines_for};

pub struct TerminalHelpModalState {
    selected_topic: TerminalHelpTopic,
    scroll_offsets: [u16; TerminalHelpTopic::ALL.len()],
}

impl Default for TerminalHelpModalState {
    fn default() -> Self {
        Self::new()
    }
}

impl TerminalHelpModalState {
    pub fn new() -> Self {
        Self {
            selected_topic: TerminalHelpTopic::Copy,
            scroll_offsets: [0; TerminalHelpTopic::ALL.len()],
        }
    }

    pub fn open(&mut self) {
        self.selected_topic = TerminalHelpTopic::Copy;
    }

    pub fn selected_topic(&self) -> TerminalHelpTopic {
        self.selected_topic
    }

    pub fn current_lines(&self) -> Vec<String> {
        lines_for(self.selected_topic)
    }

    pub fn current_scroll(&self) -> u16 {
        self.scroll_offsets[self.selected_topic.index()]
    }

    pub fn move_topic(&mut self, delta: isize) {
        let len = TerminalHelpTopic::ALL.len() as isize;
        let next = (self.selected_topic.index() as isize + delta).rem_euclid(len) as usize;
        self.selected_topic = TerminalHelpTopic::ALL[next];
    }

    pub fn scroll(&mut self, delta: i16) {
        let idx = self.selected_topic.index();
        let current = self.scroll_offsets[idx] as i32;
        self.scroll_offsets[idx] = (current + delta as i32).max(0) as u16;
    }
}
