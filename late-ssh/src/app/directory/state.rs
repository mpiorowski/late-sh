use super::super::chat::{showcase::svc::ShowcaseFeedItem, work::svc::WorkFeedItem};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DirectoryTab {
    Profiles,
    Projects,
    Pinstar,
}

impl DirectoryTab {
    pub(crate) fn next(self) -> Self {
        match self {
            Self::Profiles => Self::Projects,
            Self::Projects => Self::Pinstar,
            Self::Pinstar => Self::Profiles,
        }
    }

    pub(crate) fn prev(self) -> Self {
        match self {
            Self::Profiles => Self::Pinstar,
            Self::Projects => Self::Profiles,
            Self::Pinstar => Self::Projects,
        }
    }
}

pub(crate) struct DirectoryState {
    pub(crate) tab: DirectoryTab,
    search_mode: bool,
    search_query: String,
    search_selected: usize,
}

impl DirectoryState {
    pub(crate) fn new() -> Self {
        Self {
            tab: DirectoryTab::Profiles,
            search_mode: false,
            search_query: String::new(),
            search_selected: 0,
        }
    }

    pub(crate) fn select(&mut self, tab: DirectoryTab) {
        self.tab = tab;
        self.search_selected = 0;
    }

    pub(crate) fn enter_search(&mut self) {
        self.search_mode = true;
        self.search_query.clear();
        self.search_selected = 0;
    }

    pub(crate) fn exit_search(&mut self) {
        self.search_mode = false;
        self.search_query.clear();
        self.search_selected = 0;
    }

    pub(crate) fn search_mode(&self) -> bool {
        self.search_mode
    }

    pub(crate) fn search_query(&self) -> &str {
        &self.search_query
    }

    pub(crate) fn search_selected(&self) -> usize {
        self.search_selected
    }

    pub(crate) fn search_push(&mut self, ch: char) {
        if !ch.is_control() {
            self.search_query.push(ch);
            self.search_selected = 0;
        }
    }

    pub(crate) fn search_backspace(&mut self) {
        self.search_query.pop();
        self.search_selected = 0;
    }

    pub(crate) fn move_search_selection(&mut self, delta: isize, len: usize) {
        if len == 0 {
            self.search_selected = 0;
            return;
        }
        self.search_selected =
            (self.search_selected as isize + delta).rem_euclid(len as isize) as usize;
    }

    pub(crate) fn clamp_search_selection(&mut self, len: usize) {
        if len == 0 {
            self.search_selected = 0;
        } else {
            self.search_selected = self.search_selected.min(len - 1);
        }
    }
}

pub(crate) fn filtered_profile_indices<'a>(
    items: &'a [WorkFeedItem],
    query: &str,
) -> Vec<(usize, &'a WorkFeedItem)> {
    let query = normalize_query(query);
    items
        .iter()
        .enumerate()
        .filter(|(_, item)| query.is_empty() || profile_matches(item, &query))
        .collect()
}

pub(crate) fn filtered_project_indices<'a>(
    items: &'a [ShowcaseFeedItem],
    query: &str,
) -> Vec<(usize, &'a ShowcaseFeedItem)> {
    let query = normalize_query(query);
    items
        .iter()
        .enumerate()
        .filter(|(_, item)| query.is_empty() || project_matches(item, &query))
        .collect()
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
