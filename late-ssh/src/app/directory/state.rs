use chrono::{DateTime, Utc};
use uuid::Uuid;

use super::super::chat::{showcase::svc::ShowcaseFeedItem, work::svc::WorkFeedItem};

/// Which slice of the merged Profiles feed is visible.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DirectoryFilter {
    All,
    Projects,
    People,
}

impl DirectoryFilter {
    pub(crate) fn next(self) -> Self {
        match self {
            Self::All => Self::Projects,
            Self::Projects => Self::People,
            Self::People => Self::All,
        }
    }

    pub(crate) fn prev(self) -> Self {
        match self {
            Self::All => Self::People,
            Self::Projects => Self::All,
            Self::People => Self::Projects,
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Projects => "projects",
            Self::People => "people",
        }
    }
}

/// One row of the merged feed, borrowing from the two chat-side feed states
/// for the duration of a render or input pass.
#[derive(Clone, Copy)]
pub(crate) enum DirectoryEntry<'a> {
    Project(&'a ShowcaseFeedItem),
    Person(&'a WorkFeedItem),
}

/// Stable identity of a merged-feed row, used to re-find a row after the
/// entry list is rebuilt (for example when leaving search mode).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DirectoryEntryId {
    Project(Uuid),
    Person(Uuid),
}

impl DirectoryEntry<'_> {
    pub(crate) fn id(&self) -> DirectoryEntryId {
        match self {
            Self::Project(item) => DirectoryEntryId::Project(item.showcase.id),
            Self::Person(item) => DirectoryEntryId::Person(item.profile.id),
        }
    }

    pub(crate) fn timestamp(&self) -> DateTime<Utc> {
        match self {
            Self::Project(item) => item.showcase.created,
            Self::Person(item) => item.profile.updated,
        }
    }

    pub(crate) fn user_id(&self) -> Uuid {
        match self {
            Self::Project(item) => item.showcase.user_id,
            Self::Person(item) => item.profile.user_id,
        }
    }

    pub(crate) fn author_username(&self) -> &str {
        match self {
            Self::Project(item) => &item.author_username,
            Self::Person(item) => &item.author_username,
        }
    }
}

pub(crate) struct DirectoryState {
    pub(crate) filter: DirectoryFilter,
    pub(crate) mine_only: bool,
    selected: usize,
    search_mode: bool,
    search_query: String,
}

impl DirectoryState {
    pub(crate) fn new() -> Self {
        Self {
            filter: DirectoryFilter::All,
            mine_only: false,
            selected: 0,
            search_mode: false,
            search_query: String::new(),
        }
    }

    pub(crate) fn cycle_filter_next(&mut self) {
        self.filter = self.filter.next();
        self.selected = 0;
    }

    pub(crate) fn cycle_filter_prev(&mut self) {
        self.filter = self.filter.prev();
        self.selected = 0;
    }

    pub(crate) fn toggle_mine_only(&mut self) {
        self.mine_only = !self.mine_only;
        self.selected = 0;
    }

    pub(crate) fn selected(&self) -> usize {
        self.selected
    }

    pub(crate) fn select(&mut self, index: usize) {
        self.selected = index;
    }

    pub(crate) fn move_selection(&mut self, delta: isize, len: usize) {
        if len == 0 {
            self.selected = 0;
            return;
        }
        let clamped = self.selected.min(len - 1) as isize;
        self.selected = (clamped + delta).clamp(0, len as isize - 1) as usize;
    }

    pub(crate) fn clamp_selection(&mut self, len: usize) {
        if len == 0 {
            self.selected = 0;
        } else {
            self.selected = self.selected.min(len - 1);
        }
    }

    pub(crate) fn enter_search(&mut self) {
        self.search_mode = true;
        self.search_query.clear();
        self.selected = 0;
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
        }
    }

    pub(crate) fn search_backspace(&mut self) {
        self.search_query.pop();
        self.selected = 0;
    }

    /// The query the merged list should be filtered by right now: the live
    /// search text while the search box is open, nothing otherwise.
    pub(crate) fn active_query(&self) -> &str {
        if self.search_mode {
            &self.search_query
        } else {
            ""
        }
    }
}

/// Build the merged, filtered, newest-first feed. Both input slices already
/// arrive sorted from their services; the merge re-sorts by row timestamp
/// (showcase created, work profile updated).
pub(crate) fn merged_entries<'a>(
    projects: &'a [ShowcaseFeedItem],
    people: &'a [WorkFeedItem],
    filter: DirectoryFilter,
    mine_only: bool,
    viewer: Uuid,
    query: &str,
) -> Vec<DirectoryEntry<'a>> {
    let query = normalize_query(query);
    let mut entries: Vec<DirectoryEntry<'a>> = Vec::new();
    if matches!(filter, DirectoryFilter::All | DirectoryFilter::Projects) {
        entries.extend(projects.iter().map(DirectoryEntry::Project));
    }
    if matches!(filter, DirectoryFilter::All | DirectoryFilter::People) {
        entries.extend(people.iter().map(DirectoryEntry::Person));
    }
    entries.retain(|entry| {
        (!mine_only || entry.user_id() == viewer)
            && (query.is_empty() || entry_matches(entry, &query))
    });
    entries.sort_by(|a, b| b.timestamp().cmp(&a.timestamp()));
    entries
}

fn entry_matches(entry: &DirectoryEntry<'_>, query: &str) -> bool {
    match entry {
        DirectoryEntry::Project(item) => project_matches(item, query),
        DirectoryEntry::Person(item) => profile_matches(item, query),
    }
}

fn profile_matches(item: &WorkFeedItem, query: &str) -> bool {
    let p = &item.profile;
    [
        p.headline.as_str(),
        p.slug.as_str(),
        p.status.as_str(),
        p.work_type.as_str(),
        p.location.as_str(),
        p.summary.as_str(),
        item.author_username.as_str(),
    ]
    .into_iter()
    .any(|field| normalize_query(field).contains(query))
        || p.skills
            .iter()
            .any(|skill| normalize_query(skill).contains(query))
}

fn project_matches(item: &ShowcaseFeedItem, query: &str) -> bool {
    let s = &item.showcase;
    [
        s.title.as_str(),
        s.url.as_str(),
        s.description.as_str(),
        item.author_username.as_str(),
    ]
    .into_iter()
    .any(|field| normalize_query(field).contains(query))
        || s.tags
            .iter()
            .any(|tag| normalize_query(tag).contains(query))
}

fn normalize_query(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}
