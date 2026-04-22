use super::data::{HelpTopic, lines_for};

pub struct HelpModalState {
    selected_topic: HelpTopic,
    scroll_offsets: [u16; HelpTopic::ALL.len()],
}

impl Default for HelpModalState {
    fn default() -> Self {
        Self::new()
    }
}

impl HelpModalState {
    pub fn new() -> Self {
        Self {
            selected_topic: HelpTopic::Overview,
            scroll_offsets: [0; HelpTopic::ALL.len()],
        }
    }

    pub fn open(&mut self, topic: HelpTopic) {
        self.selected_topic = topic;
    }

    pub fn selected_topic(&self) -> HelpTopic {
        self.selected_topic
    }

    pub fn current_lines(&self) -> Vec<String> {
        lines_for(self.selected_topic)
    }

    pub fn current_scroll(&self) -> u16 {
        self.scroll_offsets[self.selected_topic.index()]
    }

    pub fn move_topic(&mut self, delta: isize) {
        let len = HelpTopic::ALL.len() as isize;
        let next = (self.selected_topic.index() as isize + delta).rem_euclid(len) as usize;
        self.selected_topic = HelpTopic::ALL[next];
    }

    pub fn scroll(&mut self, delta: i16) {
        let idx = self.selected_topic.index();
        let current = self.scroll_offsets[idx] as i32;
        self.scroll_offsets[idx] = (current + delta as i32).max(0) as u16;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn move_topic_wraps_at_both_ends() {
        let mut state = HelpModalState::new();
        state.move_topic(-1);
        assert_eq!(
            state.selected_topic(),
            HelpTopic::ALL[HelpTopic::ALL.len() - 1]
        );

        state.move_topic(1);
        assert_eq!(state.selected_topic(), HelpTopic::Overview);
    }
}
