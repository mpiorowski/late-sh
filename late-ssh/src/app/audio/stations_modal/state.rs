use late_core::models::user::RadioStation;

#[derive(Clone, Debug, Default)]
pub(crate) struct StationsModalState {
    open: bool,
    selected: usize,
}

impl StationsModalState {
    /// Open with the cursor on `current` so Enter without moving is a no-op
    /// rather than a surprise retune.
    pub(crate) fn open(&mut self, current: RadioStation) {
        self.open = true;
        self.selected = self
            .stations()
            .iter()
            .position(|station| *station == current)
            .unwrap_or(0);
    }

    pub(crate) fn close(&mut self) {
        self.open = false;
        self.selected = 0;
    }

    pub(crate) fn is_open(&self) -> bool {
        self.open
    }

    pub(crate) fn selected(&self) -> usize {
        self.selected
    }

    /// The highlighted station.
    pub(crate) fn selected_station(&self) -> Option<RadioStation> {
        self.stations().get(self.selected).copied()
    }

    /// Enabled catalogue stations in catalogue order.
    pub(crate) fn stations(&self) -> Vec<RadioStation> {
        RadioStation::enabled().collect()
    }

    pub(crate) fn move_selection(&mut self, delta: isize) {
        let len = self.stations().len();
        if len == 0 {
            self.selected = 0;
            return;
        }
        self.selected = (self.selected as isize + delta).rem_euclid(len as isize) as usize;
    }
}

#[cfg(test)]
#[path = "state_test.rs"]
mod state_test;
