use chrono::{DateTime, Utc};
use std::collections::HashMap;
use uuid::Uuid;

use super::super::chat::{showcase::svc::ShowcaseFeedItem, work::svc::WorkFeedItem};
use late_core::models::profile::Profile;

/// One row of the Profiles feed: a person, aggregated from whatever they
/// brought — a work card, projects, or both. Borrows from the two chat-side
/// feed states for the duration of a render or input pass.
pub(crate) struct PersonEntry<'a> {
    pub(crate) user_id: Uuid,
    pub(crate) username: &'a str,
    pub(crate) work: Option<&'a WorkFeedItem>,
    /// Newest-first, inherited from the showcase feed's sort order.
    pub(crate) projects: Vec<&'a ShowcaseFeedItem>,
}

/// What the detail-pane focus cursor is pointing at for a person: their work
/// card, or one of their projects. Focus index 0 is the card when present,
/// projects follow in feed order.
pub(crate) enum PersonFocus<'a> {
    Card(&'a WorkFeedItem),
    Project(&'a ShowcaseFeedItem),
}

impl<'a> PersonEntry<'a> {
    /// The freshest thing this person did; the feed sorts by it.
    pub(crate) fn latest_activity(&self) -> DateTime<Utc> {
        let work = self.work.map(|item| item.profile.updated);
        let project = self.projects.first().map(|item| item.showcase.created);
        match (work, project) {
            (Some(work), Some(project)) => work.max(project),
            (Some(work), None) => work,
            (None, Some(project)) => project,
            (None, None) => DateTime::<Utc>::MIN_UTC,
        }
    }

    /// The author's settings profile (bio, late.fetch), from whichever feed
    /// item carries it.
    pub(crate) fn author_profile(&self) -> Option<&'a Profile> {
        let from_work = self.work.and_then(|item| item.author_profile.as_ref());
        let from_project = self
            .projects
            .first()
            .and_then(|item| item.author_profile.as_ref());
        from_work.or(from_project)
    }

    pub(crate) fn focus_len(&self) -> usize {
        usize::from(self.work.is_some()) + self.projects.len()
    }

    pub(crate) fn focus_target(&self, focus: usize) -> Option<PersonFocus<'a>> {
        match self.work {
            Some(work) => {
                if focus == 0 {
                    Some(PersonFocus::Card(work))
                } else {
                    self.projects
                        .get(focus - 1)
                        .map(|item| PersonFocus::Project(item))
                }
            }
            None => self
                .projects
                .get(focus)
                .map(|item| PersonFocus::Project(item)),
        }
    }

    /// A person is unread when any of their rows moved past the matching
    /// feed-read marker.
    pub(crate) fn is_unread(
        &self,
        work_marker: Option<DateTime<Utc>>,
        showcase_marker: Option<DateTime<Utc>>,
    ) -> bool {
        let card_unread = self.work.is_some_and(|item| {
            work_marker
                .map(|marker| item.profile.updated > marker)
                .unwrap_or(true)
        });
        let project_unread = self.projects.iter().any(|item| {
            showcase_marker
                .map(|marker| item.showcase.created > marker)
                .unwrap_or(true)
        });
        card_unread || project_unread
    }
}

pub(crate) struct DirectoryState {
    pub(crate) mine_only: bool,
    selected: usize,
    /// Focus cursor inside the selected person's detail: 0 is their work
    /// card when present, projects follow. Reset whenever selection moves.
    focus: usize,
    search_mode: bool,
    search_query: String,
}

impl DirectoryState {
    pub(crate) fn new() -> Self {
        Self {
            mine_only: false,
            selected: 0,
            focus: 0,
            search_mode: false,
            search_query: String::new(),
        }
    }

    pub(crate) fn toggle_mine_only(&mut self) {
        self.mine_only = !self.mine_only;
        self.selected = 0;
        self.focus = 0;
    }

    pub(crate) fn selected(&self) -> usize {
        self.selected
    }

    pub(crate) fn select(&mut self, index: usize) {
        if self.selected != index {
            self.focus = 0;
        }
        self.selected = index;
    }

    pub(crate) fn move_selection(&mut self, delta: isize, len: usize) {
        if len == 0 {
            self.selected = 0;
            self.focus = 0;
            return;
        }
        let clamped = self.selected.min(len - 1) as isize;
        let next = (clamped + delta).clamp(0, len as isize - 1) as usize;
        if next != self.selected {
            self.focus = 0;
        }
        self.selected = next;
    }

    pub(crate) fn clamp_selection(&mut self, len: usize) {
        if len == 0 {
            self.selected = 0;
            self.focus = 0;
        } else if self.selected > len - 1 {
            self.selected = len - 1;
            self.focus = 0;
        }
    }

    pub(crate) fn focus(&self) -> usize {
        self.focus
    }

    /// Cycle the detail focus through the selected person's card + projects,
    /// wrapping at either end.
    pub(crate) fn move_focus(&mut self, delta: isize, len: usize) {
        if len == 0 {
            self.focus = 0;
            return;
        }
        let clamped = self.focus.min(len - 1) as isize;
        self.focus = (clamped + delta).rem_euclid(len as isize) as usize;
    }

    pub(crate) fn enter_search(&mut self) {
        self.search_mode = true;
        self.search_query.clear();
        self.selected = 0;
        self.focus = 0;
    }

    pub(crate) fn exit_search(&mut self) {
        self.search_mode = false;
        self.search_query.clear();
    }

    pub(crate) fn search_mode(&self) -> bool {
        self.search_mode
    }

    pub(crate) fn search_query(&self) -> &str {
        &self.search_query
    }

    pub(crate) fn search_push(&mut self, ch: char) {
        if !ch.is_control() {
            self.search_query.push(ch);
            self.selected = 0;
            self.focus = 0;
        }
    }

    pub(crate) fn search_backspace(&mut self) {
        self.search_query.pop();
        self.selected = 0;
        self.focus = 0;
    }

    /// The query the people list should be filtered by right now: the live
    /// search text while the search box is open, nothing otherwise.
    pub(crate) fn active_query(&self) -> &str {
        if self.search_mode {
            &self.search_query
        } else {
            ""
        }
    }
}

/// Build the people list: one entry per user who has a work card or at least
/// one project, sorted by their latest activity, newest first. Each person's
/// projects keep the showcase feed's newest-first order.
pub(crate) fn person_entries<'a>(
    projects: &'a [ShowcaseFeedItem],
    people: &'a [WorkFeedItem],
    mine_only: bool,
    viewer: Uuid,
    query: &str,
) -> Vec<PersonEntry<'a>> {
    let query = normalize_query(query);
    let mut by_user: HashMap<Uuid, PersonEntry<'a>> = HashMap::new();
    let mut order: Vec<Uuid> = Vec::new();

    for item in people {
        let user_id = item.profile.user_id;
        by_user.entry(user_id).or_insert_with(|| {
            order.push(user_id);
            PersonEntry {
                user_id,
                username: &item.author_username,
                work: Some(item),
                projects: Vec::new(),
            }
        });
    }
    for item in projects {
        let user_id = item.showcase.user_id;
        by_user
            .entry(user_id)
            .or_insert_with(|| {
                order.push(user_id);
                PersonEntry {
                    user_id,
                    username: &item.author_username,
                    work: None,
                    projects: Vec::new(),
                }
            })
            .projects
            .push(item);
    }

    let mut entries: Vec<PersonEntry<'a>> = order
        .into_iter()
        .filter_map(|user_id| by_user.remove(&user_id))
        .filter(|entry| {
            (!mine_only || entry.user_id == viewer)
                && (query.is_empty() || person_matches(entry, &query))
        })
        .collect();
    entries.sort_by_key(|b| std::cmp::Reverse(b.latest_activity()));
    entries
}

fn person_matches(entry: &PersonEntry<'_>, query: &str) -> bool {
    if normalize_query(entry.username).contains(query) {
        return true;
    }
    if let Some(item) = entry.work
        && card_matches(item, query)
    {
        return true;
    }
    entry
        .projects
        .iter()
        .any(|item| project_matches(item, query))
}

fn card_matches(item: &WorkFeedItem, query: &str) -> bool {
    let p = &item.profile;
    [
        p.headline.as_str(),
        p.slug.as_str(),
        p.status.as_str(),
        p.work_type.as_str(),
        p.location.as_str(),
        p.summary.as_str(),
    ]
    .into_iter()
    .any(|field| normalize_query(field).contains(query))
        || p.skills
            .iter()
            .any(|skill| normalize_query(skill).contains(query))
}

fn project_matches(item: &ShowcaseFeedItem, query: &str) -> bool {
    let s = &item.showcase;
    [s.title.as_str(), s.url.as_str(), s.description.as_str()]
        .into_iter()
        .any(|field| normalize_query(field).contains(query))
        || s.tags
            .iter()
            .any(|tag| normalize_query(tag).contains(query))
}

fn normalize_query(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}
