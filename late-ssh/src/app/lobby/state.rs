//! Per-session Lobby modal state: the cursor over the combined entry list,
//! the claim confirmation, and the unseen-challenge glow. The entries
//! themselves are views over `DailyState`'s snapshot plus the fixed
//! `HouseTable` roster; this struct owns only the presentation state that
//! spans both domains.

use std::collections::HashSet;

use uuid::Uuid;

use crate::app::lobby::{
    daily::{
        state::DailyState,
        svc::{DailyChallengeItem, DailyFinishedItem, DailyMatchItem},
    },
    house::tables::HouseTable,
};

/// One selectable row in the Lobby modal: unseen results first, then your
/// matches, then the open lobby, then other people's live games you can
/// watch, then the fixed house tables.
pub enum LobbyEntry<'a> {
    Finished(&'a DailyFinishedItem),
    Match(&'a DailyMatchItem),
    Challenge(&'a DailyChallengeItem),
    Spectate(&'a DailyMatchItem),
    House(HouseTable),
}

pub struct LobbyState {
    /// Modal cursor over `entry_count()` rows.
    pub selected: usize,
    /// Challenge awaiting claim confirmation (Enter pressed once).
    pub confirm_claim: Option<Uuid>,
    /// Open-challenge ids already seen; anything newer glows the lobby line
    /// until the modal is opened.
    seen_open_ids: HashSet<Uuid>,
    glow: bool,
}

impl LobbyState {
    /// Challenges that predate the session don't glow; only ones posted
    /// while connected count as news.
    pub(crate) fn new(daily: &DailyState) -> Self {
        Self {
            selected: 0,
            confirm_claim: None,
            seen_open_ids: daily.lobby().iter().map(|challenge| challenge.id).collect(),
            glow: false,
        }
    }

    pub fn glow(&self) -> bool {
        self.glow
    }

    /// Follow the daily snapshot: pick up new-challenge glow edges and keep
    /// the cursor and pending claim valid. Idempotent; runs every tick.
    pub fn sync(&mut self, daily: &DailyState) {
        self.refresh_glow(daily);
        self.clamp_selection(daily);
    }

    fn refresh_glow(&mut self, daily: &DailyState) {
        let challenges = daily.lobby();
        let open_ids: HashSet<Uuid> = challenges.iter().map(|challenge| challenge.id).collect();
        // Own challenges never glow; mark them seen immediately.
        let own_ids: Vec<Uuid> = challenges
            .iter()
            .filter(|challenge| challenge.challenger_id == daily.user_id())
            .map(|challenge| challenge.id)
            .collect();
        self.seen_open_ids.extend(own_ids);
        if challenges
            .iter()
            .any(|challenge| !self.seen_open_ids.contains(&challenge.id))
        {
            self.glow = true;
        }
        // Drop ids that left the lobby so the set can't grow unbounded.
        self.seen_open_ids.retain(|id| open_ids.contains(id));
    }

    /// Called when the modal opens: the lobby has been looked at.
    pub fn mark_seen(&mut self, daily: &DailyState) {
        self.seen_open_ids = daily.lobby().iter().map(|challenge| challenge.id).collect();
        self.glow = false;
        self.clamp_selection(daily);
    }

    // ── Modal navigation ───────────────────────────────────────

    pub fn entry_count(&self, daily: &DailyState) -> usize {
        daily.my_finished().len()
            + daily.my_matches().len()
            + daily.lobby().len()
            + daily.live_games().len()
            + HouseTable::ALL.len()
    }

    pub fn entry_at<'a>(&self, daily: &'a DailyState, index: usize) -> Option<LobbyEntry<'a>> {
        let finished = daily.my_finished();
        if index < finished.len() {
            return Some(LobbyEntry::Finished(finished[index]));
        }
        let index = index - finished.len();
        let matches = daily.my_matches();
        if index < matches.len() {
            return Some(LobbyEntry::Match(matches[index]));
        }
        let index = index - matches.len();
        let lobby = daily.lobby();
        if index < lobby.len() {
            return Some(LobbyEntry::Challenge(lobby[index]));
        }
        let index = index - lobby.len();
        let live = daily.live_games();
        if index < live.len() {
            return Some(LobbyEntry::Spectate(live[index]));
        }
        // The fixed house-table block sits at the bottom, always present.
        HouseTable::ALL
            .get(index - live.len())
            .copied()
            .map(LobbyEntry::House)
    }

    pub fn selected_entry<'a>(&self, daily: &'a DailyState) -> Option<LobbyEntry<'a>> {
        self.entry_at(daily, self.selected)
    }

    pub fn move_selection(&mut self, daily: &DailyState, delta: isize) {
        self.selected = wrap_index(self.selected, delta, self.entry_count(daily));
        self.confirm_claim = None;
    }

    fn clamp_selection(&mut self, daily: &DailyState) {
        let count = self.entry_count(daily);
        if count == 0 {
            self.selected = 0;
        } else {
            self.selected = self.selected.min(count - 1);
        }
        if let Some(pending) = self.confirm_claim
            && !daily
                .lobby()
                .iter()
                .any(|challenge| challenge.id == pending)
        {
            self.confirm_claim = None;
        }
    }
}

/// Cursor arithmetic for the modal list. Moves wrap in both directions: `k`
/// on the first row lands on the last, `j` on the last returns to the first.
/// The list is one flat index space over all five sections, so wrapping
/// backwards from the top always reaches the house-table block (the only
/// section that is never empty). An empty list pins at 0.
fn wrap_index(selected: usize, delta: isize, count: usize) -> usize {
    if count == 0 {
        return 0;
    }
    (selected as isize + delta).rem_euclid(count as isize) as usize
}

#[cfg(test)]
#[path = "state_test.rs"]
mod state_test;
