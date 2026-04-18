use super::data::{HelpTopic, lines_for};

pub struct HelpModalState {
    selected_topic: HelpTopic,
    modal_width: u16,
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
            modal_width: 88,
            scroll_offsets: [0; HelpTopic::ALL.len()],
        }
    }

    pub fn open(&mut self, topic: HelpTopic, modal_width: u16) {
        self.selected_topic = topic;
        self.set_modal_width(modal_width);
    }

    pub fn set_modal_width(&mut self, width: u16) {
        self.modal_width = width.max(40);
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
        let next = (self.selected_topic.index() as isize + delta).clamp(0, len - 1) as usize;
        self.selected_topic = HelpTopic::ALL[next];
    }

    pub fn scroll(&mut self, delta: i16, visible_height: u16) {
        let idx = self.selected_topic.index();
        let current = self.scroll_offsets[idx] as i32;
        let max_scroll = self.max_scroll_for(self.selected_topic, visible_height) as i32;
        self.scroll_offsets[idx] = (current + delta as i32).clamp(0, max_scroll) as u16;
    }

    pub fn page_scroll(&mut self, delta_pages: i16, visible_height: u16) {
        let step = visible_height.max(1) as i16;
        self.scroll(delta_pages.saturating_mul(step), visible_height);
    }

    fn max_scroll_for(&self, topic: HelpTopic, visible_height: u16) -> u16 {
        let body_width = self.body_width();
        let row_count: u16 = lines_for(topic)
            .iter()
            .map(|line| wrapped_row_count(line, body_width))
            .sum();
        row_count.saturating_sub(visible_height.max(1))
    }

    fn body_width(&self) -> u16 {
        self.modal_width.saturating_sub(8).max(20)
    }
}

fn wrapped_row_count(line: &str, width: u16) -> u16 {
    let width = width.max(1) as usize;
    let cell_count = 1 + line.chars().count();
    cell_count.div_ceil(width) as u16
}
