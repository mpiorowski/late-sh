use late_core::models::user::RadioStation;

/// Upper bound on the `/` filter query, mirroring the booth History filter.
const FILTER_MAX_LEN: usize = 32;

#[derive(Clone, Debug, Default)]
pub(crate) struct StationsModalState {
    open: bool,
    selected: usize,
    /// True while the `/` filter input is capturing keystrokes.
    filter_active: bool,
    /// Case-insensitive substring over label, provider and tags. Kept after
    /// Enter so the list stays filtered until cleared.
    filter_query: String,
}

impl StationsModalState {
    /// Open with the cursor on `current` so Enter without moving is a no-op
    /// rather than a surprise retune.
    pub(crate) fn open(&mut self, current: RadioStation) {
        self.open = true;
        self.filter_active = false;
        self.filter_query.clear();
        self.selected = self
            .stations()
            .iter()
            .position(|station| *station == current)
            .unwrap_or(0);
    }

    pub(crate) fn close(&mut self) {
        self.open = false;
        self.selected = 0;
        self.filter_active = false;
        self.filter_query.clear();
    }

    pub(crate) fn is_open(&self) -> bool {
        self.open
    }

    pub(crate) fn selected(&self) -> usize {
        self.selected
    }

    /// The highlighted station, if the filtered list has one.
    pub(crate) fn selected_station(&self) -> Option<RadioStation> {
        self.stations().get(self.selected).copied()
    }

    /// Enabled catalogue stations in catalogue order, narrowed by the
    /// filter query.
    pub(crate) fn stations(&self) -> Vec<RadioStation> {
        let query = self.filter_query.trim().to_lowercase();
        RadioStation::enabled()
            .filter(|station| {
                query.is_empty()
                    || station.label().contains(&query)
                    || station.as_str().contains(&query)
                    || station.provider().label().contains(&query)
                    || station.tags().iter().any(|tag| tag.contains(&query))
            })
            .collect()
    }

    pub(crate) fn move_selection(&mut self, delta: isize) {
        let len = self.stations().len();
        if len == 0 {
            self.selected = 0;
            return;
        }
        self.selected = (self.selected as isize + delta).rem_euclid(len as isize) as usize;
    }

    pub(crate) fn clamp(&mut self) {
        let len = self.stations().len();
        self.selected = if len == 0 {
            0
        } else {
            self.selected.min(len - 1)
        };
    }

    pub(crate) fn filter_active(&self) -> bool {
        self.filter_active
    }

    pub(crate) fn filter_query(&self) -> &str {
        &self.filter_query
    }

    pub(crate) fn start_filter(&mut self) {
        self.filter_active = true;
    }

    /// Enter: keep the query, stop capturing.
    pub(crate) fn apply_filter(&mut self) {
        self.filter_active = false;
        self.clamp();
    }

    /// Esc while capturing: drop the query and stop capturing.
    pub(crate) fn cancel_filter(&mut self) {
        self.filter_active = false;
        self.filter_query.clear();
        self.clamp();
    }

    pub(crate) fn push_filter(&mut self, ch: char) {
        if ch.is_control() || self.filter_query.chars().count() >= FILTER_MAX_LEN {
            return;
        }
        self.filter_query.push(ch);
        self.clamp();
    }

    pub(crate) fn backspace_filter(&mut self) {
        self.filter_query.pop();
        self.clamp();
    }

    pub(crate) fn clear_filter(&mut self) {
        self.filter_query.clear();
        self.clamp();
    }
}

#[cfg(test)]
#[path = "state_test.rs"]
mod state_test;
