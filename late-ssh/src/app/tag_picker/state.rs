//! The picker's state machine: a scope (languages alone, or the whole
//! vocabulary), a query that filters the list, a cursor that walks the
//! tags and skips the group headings, and the chosen tags in the order
//! they were picked. No I/O, no host knowledge beyond the target enum.

use std::cell::Cell;

use late_core::vocab::{self, Group, TAG_LIMIT};

/// What the cap reads as when a thirteenth tag is picked.
pub(crate) const CAP_NOTICE: &str = "Twelve is the cap; drop one to add another.";

/// Which field the picker is filling: it decides the scope, and where the
/// chosen tags go when the picker closes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TagPickerTarget {
    /// The langs row of the settings modal.
    SettingsLangs,
    /// The skills row of the profile editor's card page.
    EditorSkills,
    /// The langs row of the profile editor's about page.
    EditorLangs,
    /// The stack row of the job post form.
    JobPost,
}

impl TagPickerTarget {
    pub(crate) const fn scope(self) -> TagScope {
        match self {
            Self::SettingsLangs | Self::EditorLangs => TagScope::Langs,
            Self::EditorSkills | Self::JobPost => TagScope::Skills,
        }
    }
}

/// Which part of the vocabulary the list offers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TagScope {
    Langs,
    Skills,
}

impl TagScope {
    pub(crate) const fn title(self) -> &'static str {
        match self {
            Self::Langs => "langs",
            Self::Skills => "skills",
        }
    }

    pub(crate) fn groups(self) -> &'static [Group] {
        match self {
            Self::Langs => &[Group::Language],
            Self::Skills => &Group::ALL,
        }
    }
}

/// One line of the list: a group heading, which the cursor skips, or a tag.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Row {
    Heading(Group),
    Tag(&'static str),
}

#[derive(Default)]
pub(crate) struct TagPickerState {
    target: Option<TagPickerTarget>,
    query: String,
    cursor: usize,
    scroll: usize,
    /// Rows the list showed last frame, so a page and a scroll fit it.
    visible_height: Cell<usize>,
    chosen: Vec<String>,
    notice: Option<&'static str>,
}

impl TagPickerState {
    /// Open for `target`, seeded with what the field holds now. Whatever
    /// the vocabulary does not know is shed here, so the picker only ever
    /// shows and hands back canonical tags.
    pub(crate) fn open(&mut self, target: TagPickerTarget, current: Vec<String>) {
        *self = Self::default();
        self.target = Some(target);
        for word in &current {
            let Some(tag) = vocab::canonical(word) else {
                continue;
            };
            if !target.scope().groups().contains(&group(tag)) {
                continue;
            }
            if self.chosen.len() < TAG_LIMIT && !self.is_chosen(tag) {
                self.chosen.push(tag.to_string());
            }
        }
        self.cursor = self.first_tag_row();
    }

    /// Close and hand back the target with the chosen tags; none when the
    /// picker was not open.
    pub(crate) fn close(&mut self) -> Option<(TagPickerTarget, Vec<String>)> {
        let target = self.target.take()?;
        let chosen = std::mem::take(&mut self.chosen);
        *self = Self::default();
        Some((target, chosen))
    }

    pub(crate) fn is_open(&self) -> bool {
        self.target.is_some()
    }

    pub(crate) fn scope(&self) -> Option<TagScope> {
        self.target.map(TagPickerTarget::scope)
    }

    pub(crate) fn query(&self) -> &str {
        &self.query
    }

    pub(crate) fn chosen(&self) -> &[String] {
        &self.chosen
    }

    pub(crate) fn cursor(&self) -> usize {
        self.cursor
    }

    pub(crate) fn scroll(&self) -> usize {
        self.scroll
    }

    pub(crate) fn notice(&self) -> Option<&'static str> {
        self.notice
    }

    pub(crate) fn set_visible_height(&self, height: usize) {
        self.visible_height.set(height.max(1));
    }

    pub(crate) fn is_chosen(&self, tag: &str) -> bool {
        self.chosen.iter().any(|known| known == tag)
    }

    /// The list as shown: with no query, every group of the scope under
    /// its heading; with one, the tags whose canonical name or any alias
    /// contains it, those that start with it first, no headings.
    pub(crate) fn rows(&self) -> Vec<Row> {
        let Some(scope) = self.scope() else {
            return Vec::new();
        };
        let query = self.query.trim();
        if query.is_empty() {
            let mut rows = Vec::new();
            for group in scope.groups() {
                rows.push(Row::Heading(*group));
                rows.extend(vocab::tags_in(*group).map(Row::Tag));
            }
            return rows;
        }
        let mut starts = Vec::new();
        let mut contains = Vec::new();
        for group in scope.groups() {
            for tag in vocab::tags_in(*group) {
                let aliases = vocab::aliases_of(tag);
                if aliases.iter().any(|alias| alias.starts_with(query)) {
                    starts.push(Row::Tag(tag));
                } else if aliases.iter().any(|alias| alias.contains(query)) {
                    contains.push(Row::Tag(tag));
                }
            }
        }
        starts.extend(contains);
        starts
    }

    /// The tag under the cursor, if the cursor is on one.
    pub(crate) fn current(&self) -> Option<&'static str> {
        match self.rows().get(self.cursor) {
            Some(Row::Tag(tag)) => Some(tag),
            Some(Row::Heading(_)) | None => None,
        }
    }

    /// A typed character narrows the list. Only what a tag can be spelled
    /// with is taken, lowercased, so the query never holds a character no
    /// alias has.
    pub(crate) fn push(&mut self, ch: char) {
        let ch = ch.to_ascii_lowercase();
        if !(ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | '+' | '#')) {
            return;
        }
        self.query.push(ch);
        self.notice = None;
        self.cursor = self.first_tag_row();
        self.scroll = 0;
    }

    /// Backspace edits the query; on an empty query it drops the last
    /// chosen tag, so the chosen line edits like a line of text.
    pub(crate) fn backspace(&mut self) {
        self.notice = None;
        if self.query.pop().is_some() {
            self.cursor = self.first_tag_row();
            self.scroll = 0;
        } else {
            self.chosen.pop();
        }
    }

    /// Move the cursor `delta` tags, skipping headings, clamped to the list.
    pub(crate) fn move_cursor(&mut self, delta: isize) {
        let rows = self.rows();
        if rows.is_empty() {
            self.cursor = 0;
            self.scroll = 0;
            return;
        }
        let step: isize = if delta < 0 { -1 } else { 1 };
        let mut left = delta.unsigned_abs();
        let mut at = self.cursor.min(rows.len() - 1) as isize;
        while left > 0 {
            let mut next = at + step;
            while (0..rows.len() as isize).contains(&next)
                && matches!(rows[next as usize], Row::Heading(_))
            {
                next += step;
            }
            if !(0..rows.len() as isize).contains(&next) {
                break;
            }
            at = next;
            left -= 1;
        }
        self.cursor = at as usize;
        self.keep_cursor_visible();
    }

    /// Space: pick or drop the tag under the cursor. A thirteenth pick is
    /// refused with the notice.
    pub(crate) fn toggle(&mut self) {
        let Some(tag) = self.current() else {
            return;
        };
        match self.chosen.iter().position(|known| known == tag) {
            Some(index) => {
                self.chosen.remove(index);
                self.notice = None;
            }
            None if self.chosen.len() >= TAG_LIMIT => {
                self.notice = Some(CAP_NOTICE);
            }
            None => {
                self.chosen.push(tag.to_string());
                self.notice = None;
            }
        }
    }

    /// Enter: toggle, then clear the query so the next word can be typed,
    /// keeping the cursor on the tag just picked.
    pub(crate) fn enter(&mut self) {
        let Some(tag) = self.current() else {
            return;
        };
        self.toggle();
        if !self.query.is_empty() {
            self.query.clear();
            self.cursor = self
                .rows()
                .iter()
                .position(|row| *row == Row::Tag(tag))
                .unwrap_or(0);
            self.keep_cursor_visible();
        }
    }

    fn first_tag_row(&self) -> usize {
        self.rows()
            .iter()
            .position(|row| matches!(row, Row::Tag(_)))
            .unwrap_or(0)
    }

    /// Scroll so the cursor's row is on screen, with its heading when the
    /// cursor sits right under one.
    fn keep_cursor_visible(&mut self) {
        let visible = self.visible_height.get().max(1);
        let rows = self.rows();
        let mut top = self.cursor;
        if top > 0 && matches!(rows.get(top - 1), Some(Row::Heading(_))) {
            top -= 1;
        }
        if top < self.scroll {
            self.scroll = top;
        } else if self.cursor >= self.scroll + visible {
            self.scroll = self.cursor + 1 - visible;
        }
    }
}

fn group(tag: &str) -> Group {
    vocab::group_of(tag).expect("a canonical tag has a group")
}
