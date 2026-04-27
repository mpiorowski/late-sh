use chrono::{DateTime, Utc};
use ratatui_textarea::{TextArea, WrapMode};
use tokio::sync::{broadcast, watch};
use uuid::Uuid;

use crate::app::common::{composer, primitives::Banner};
use late_core::models::showcase::ShowcaseParams;

use super::svc::{self, ShowcaseEvent, ShowcaseFeedItem, ShowcaseService, ShowcaseSnapshot};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ComposerField {
    Title,
    Url,
    Tags,
    Description,
}

impl ComposerField {
    pub(crate) fn next(self) -> Self {
        match self {
            Self::Title => Self::Url,
            Self::Url => Self::Tags,
            Self::Tags => Self::Description,
            Self::Description => Self::Title,
        }
    }

    pub(crate) fn prev(self) -> Self {
        match self {
            Self::Title => Self::Description,
            Self::Url => Self::Title,
            Self::Tags => Self::Url,
            Self::Description => Self::Tags,
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Title => "Title",
            Self::Url => "URL",
            Self::Tags => "Tags (comma sep)",
            Self::Description => "Description",
        }
    }

    pub(crate) fn placeholder(self) -> &'static str {
        match self {
            Self::Title => "Project name",
            Self::Url => "https://...",
            Self::Tags => "rust, cli, game",
            Self::Description => "What is it? Why should we look?",
        }
    }
}

pub struct State {
    service: ShowcaseService,
    user_id: Uuid,
    is_admin: bool,
    items: Vec<ShowcaseFeedItem>,
    selected: usize,
    snapshot_rx: watch::Receiver<ShowcaseSnapshot>,
    event_rx: broadcast::Receiver<ShowcaseEvent>,
    composing: bool,
    editing_id: Option<Uuid>,
    field: ComposerField,
    title: TextArea<'static>,
    url: TextArea<'static>,
    tags: TextArea<'static>,
    description: TextArea<'static>,
    submitted: bool,
    unread_count: i64,
    last_read_at: Option<DateTime<Utc>>,
    marker_read_at: Option<DateTime<Utc>>,
}

impl State {
    pub fn new(service: ShowcaseService, user_id: Uuid, is_admin: bool) -> Self {
        let state = Self::new_without_initial_load(service, user_id, is_admin);
        state.list();
        state.refresh_unread_count();
        state
    }

    /// Build a fresh `State` without spawning the initial list task. Used by
    /// tests that don't have a tokio runtime available; production paths
    /// should call `State::new`.
    pub fn new_without_initial_load(
        service: ShowcaseService,
        user_id: Uuid,
        is_admin: bool,
    ) -> Self {
        let snapshot_rx = service.subscribe_snapshot();
        let event_rx = service.subscribe_events();
        Self {
            service,
            user_id,
            is_admin,
            items: Vec::new(),
            selected: 0,
            snapshot_rx,
            event_rx,
            composing: false,
            editing_id: None,
            field: ComposerField::Title,
            title: new_single_line("Project name"),
            url: new_single_line("https://..."),
            tags: new_single_line("rust, cli, game"),
            description: new_multi_line("What is it? Why should we look?"),
            submitted: false,
            unread_count: 0,
            last_read_at: None,
            marker_read_at: None,
        }
    }

    pub fn user_id(&self) -> Uuid {
        self.user_id
    }

    pub fn is_admin(&self) -> bool {
        self.is_admin
    }

    pub fn list(&self) {
        self.service.list_task();
        self.refresh_unread_count();
    }

    pub fn refresh_unread_count(&self) {
        self.service.refresh_unread_count_task(self.user_id);
    }

    pub fn mark_read(&mut self) {
        self.marker_read_at = self.last_read_at;
        self.unread_count = 0;
        self.service.mark_read_task(self.user_id);
    }

    pub fn all_items(&self) -> &[ShowcaseFeedItem] {
        &self.items
    }

    pub fn unread_count(&self) -> i64 {
        self.unread_count
    }

    pub fn marker_read_at(&self) -> Option<DateTime<Utc>> {
        self.marker_read_at
    }

    pub fn selected_index(&self) -> usize {
        clamp_index(self.selected, self.items.len())
    }

    pub fn selected_item(&self) -> Option<&ShowcaseFeedItem> {
        self.items.get(self.selected_index())
    }

    pub fn move_selection(&mut self, delta: isize) {
        self.selected = move_index(self.selected_index(), delta, self.items.len());
    }

    pub fn selected_url(&self) -> Option<&str> {
        self.selected_item().map(|item| item.showcase.url.as_str())
    }

    pub fn selected_can_edit(&self) -> bool {
        self.selected_item()
            .map(|item| self.is_admin || item.showcase.user_id == self.user_id)
            .unwrap_or(false)
    }

    pub fn composing(&self) -> bool {
        self.composing
    }

    pub fn editing(&self) -> bool {
        self.editing_id.is_some()
    }

    pub(crate) fn active_field(&self) -> ComposerField {
        self.field
    }

    pub(crate) fn field_textarea(&self, field: ComposerField) -> &TextArea<'static> {
        match field {
            ComposerField::Title => &self.title,
            ComposerField::Url => &self.url,
            ComposerField::Tags => &self.tags,
            ComposerField::Description => &self.description,
        }
    }

    pub(crate) fn field_is_empty(&self, field: ComposerField) -> bool {
        let lines = self.field_textarea(field).lines();
        lines.len() == 1 && lines[0].is_empty()
    }

    pub fn refresh_composer_theme(&mut self) {
        for f in [
            ComposerField::Title,
            ComposerField::Url,
            ComposerField::Tags,
            ComposerField::Description,
        ] {
            let active = self.composing && f == self.field;
            self.apply_field_style(f, active);
        }
    }

    fn apply_field_style(&mut self, field: ComposerField, cursor_visible: bool) {
        let ta = match field {
            ComposerField::Title => &mut self.title,
            ComposerField::Url => &mut self.url,
            ComposerField::Tags => &mut self.tags,
            ComposerField::Description => &mut self.description,
        };
        composer::apply_themed_textarea_style(ta, cursor_visible);
    }

    pub fn start_composing(&mut self) {
        self.reset_composer();
        self.composing = true;
        self.editing_id = None;
        self.field = ComposerField::Title;
        self.refresh_composer_theme();
    }

    pub fn start_editing_selected(&mut self) -> bool {
        let Some(item) = self.selected_item() else {
            return false;
        };
        if !(self.is_admin || item.showcase.user_id == self.user_id) {
            return false;
        }
        let title = item.showcase.title.clone();
        let url = item.showcase.url.clone();
        let tags = item.showcase.tags.join(", ");
        let description = item.showcase.description.clone();
        let id = item.showcase.id;

        self.reset_composer();
        self.title = new_single_line("Project name");
        self.title.insert_str(title);
        self.url = new_single_line("https://...");
        self.url.insert_str(url);
        self.tags = new_single_line("rust, cli, game");
        self.tags.insert_str(tags);
        self.description = new_multi_line("What is it? Why should we look?");
        self.description.insert_str(description);

        self.composing = true;
        self.editing_id = Some(id);
        self.field = ComposerField::Title;
        self.refresh_composer_theme();
        true
    }

    pub fn stop_composing(&mut self) {
        self.composing = false;
        self.editing_id = None;
        self.reset_composer();
    }

    fn reset_composer(&mut self) {
        self.title = new_single_line("Project name");
        self.url = new_single_line("https://...");
        self.tags = new_single_line("rust, cli, game");
        self.description = new_multi_line("What is it? Why should we look?");
        self.refresh_composer_theme();
    }

    pub fn cycle_field(&mut self, forward: bool) {
        let next = if forward {
            self.field.next()
        } else {
            self.field.prev()
        };
        self.field = next;
        self.refresh_composer_theme();
    }

    pub fn delete_selected(&mut self) -> Option<Banner> {
        let item = self.selected_item()?;
        if !(self.is_admin || item.showcase.user_id == self.user_id) {
            return Some(Banner::error("not your showcase"));
        }
        self.service
            .delete_task(self.user_id, item.showcase.id, self.is_admin);
        None
    }

    pub fn submit(&mut self) -> Option<Banner> {
        let title = self.title.lines().join(" ").trim().to_string();
        let url = self.url.lines().join("").trim().to_string();
        let tags_raw = self.tags.lines().join(",");
        let description = self.description.lines().join("\n").trim().to_string();

        if title.is_empty() {
            self.field = ComposerField::Title;
            self.refresh_composer_theme();
            return Some(Banner::error("title required"));
        }
        if title.chars().count() > 120 {
            return Some(Banner::error("title too long (max 120)"));
        }
        if url.is_empty() {
            self.field = ComposerField::Url;
            self.refresh_composer_theme();
            return Some(Banner::error("url required"));
        }
        if !looks_like_url(&url) {
            self.field = ComposerField::Url;
            self.refresh_composer_theme();
            return Some(Banner::error("url must start with http:// or https://"));
        }
        if description.is_empty() {
            self.field = ComposerField::Description;
            self.refresh_composer_theme();
            return Some(Banner::error("description required"));
        }
        if description.chars().count() > 800 {
            return Some(Banner::error("description too long (max 800)"));
        }

        let tags = svc::parse_tags(&tags_raw);

        let params = ShowcaseParams {
            user_id: self.user_id,
            title,
            url,
            description,
            tags,
        };

        if let Some(id) = self.editing_id {
            self.service
                .update_task(self.user_id, id, params, self.is_admin);
        } else {
            self.service.create_task(self.user_id, params);
        }
        self.submitted = true;
        None
    }

    pub fn copy_selected_url(&self) -> Option<String> {
        self.selected_url().map(|url| url.trim().to_string())
    }

    pub fn tick(&mut self) -> Option<Banner> {
        self.drain_snapshot();
        self.drain_events()
    }

    fn drain_snapshot(&mut self) {
        if let Ok(true) = self.snapshot_rx.has_changed() {
            let snapshot = self.snapshot_rx.borrow_and_update().clone();
            self.items = snapshot.items;
            self.selected = clamp_index(self.selected, self.items.len());
        }
    }

    fn drain_events(&mut self) -> Option<Banner> {
        let mut banner = None;
        loop {
            match self.event_rx.try_recv() {
                Ok(event) => match event {
                    ShowcaseEvent::Created { user_id }
                        if self.user_id == user_id && self.submitted =>
                    {
                        self.submitted = false;
                        self.stop_composing();
                        banner = Some(Banner::success("Showcase shared!"));
                    }
                    ShowcaseEvent::Updated { user_id }
                        if self.user_id == user_id && self.submitted =>
                    {
                        self.submitted = false;
                        self.stop_composing();
                        banner = Some(Banner::success("Showcase updated."));
                    }
                    ShowcaseEvent::Deleted { user_id } if self.user_id == user_id => {
                        banner = Some(Banner::success("Showcase deleted."));
                    }
                    ShowcaseEvent::Failed { user_id, error } if self.user_id == user_id => {
                        self.submitted = false;
                        banner = Some(Banner::error(&format!("Failed: {error}")));
                    }
                    ShowcaseEvent::UnreadCountUpdated {
                        user_id,
                        unread_count,
                        last_read_at,
                    } if self.user_id == user_id => {
                        self.unread_count = unread_count;
                        self.last_read_at = last_read_at;
                    }
                    ShowcaseEvent::NewShowcasesAvailable {
                        user_id,
                        unread_count,
                    } if self.user_id == user_id => {
                        let increased = unread_count > self.unread_count;
                        self.unread_count = unread_count;
                        if increased {
                            let noun = if unread_count == 1 {
                                "showcase"
                            } else {
                                "showcases"
                            };
                            banner = Some(Banner::success(&format!("{unread_count} new {noun}")));
                        }
                    }
                    _ => {}
                },
                Err(broadcast::error::TryRecvError::Empty) => break,
                Err(e) => {
                    tracing::error!(%e, "failed to receive showcase event");
                    break;
                }
            }
        }
        banner
    }

    pub(crate) fn field_input(&mut self, field: ComposerField, input: ratatui_textarea::Input) {
        let ta = match field {
            ComposerField::Title => &mut self.title,
            ComposerField::Url => &mut self.url,
            ComposerField::Tags => &mut self.tags,
            ComposerField::Description => &mut self.description,
        };
        ta.input(input);
    }

    pub(crate) fn field_insert_char(&mut self, ch: char) {
        if ch == '\n' {
            self.field_newline();
            return;
        }
        let ta = match self.field {
            ComposerField::Title => &mut self.title,
            ComposerField::Url => &mut self.url,
            ComposerField::Tags => &mut self.tags,
            ComposerField::Description => &mut self.description,
        };
        ta.insert_char(ch);
    }

    pub(crate) fn field_delete_char(&mut self) {
        let ta = self.active_field_mut();
        ta.delete_char();
    }

    pub(crate) fn field_paste(&mut self) {
        let ta = self.active_field_mut();
        ta.paste();
    }

    pub(crate) fn field_undo(&mut self) {
        let ta = self.active_field_mut();
        ta.undo();
    }

    pub(crate) fn field_clear_line(&mut self) {
        let ta = self.active_field_mut();
        ta.delete_line_by_head();
    }

    pub(crate) fn field_newline(&mut self) {
        if matches!(self.field, ComposerField::Description) {
            self.description.insert_newline();
        }
    }

    fn active_field_mut(&mut self) -> &mut TextArea<'static> {
        match self.field {
            ComposerField::Title => &mut self.title,
            ComposerField::Url => &mut self.url,
            ComposerField::Tags => &mut self.tags,
            ComposerField::Description => &mut self.description,
        }
    }
}

fn new_single_line(placeholder: &str) -> TextArea<'static> {
    composer::new_themed_textarea(placeholder.to_string(), WrapMode::Glyph, false)
}

fn new_multi_line(placeholder: &str) -> TextArea<'static> {
    composer::new_themed_textarea(placeholder.to_string(), WrapMode::Word, false)
}

fn looks_like_url(s: &str) -> bool {
    let s = s.trim();
    s.starts_with("http://") || s.starts_with("https://")
}

fn clamp_index(index: usize, len: usize) -> usize {
    if len == 0 { 0 } else { index.min(len - 1) }
}

fn move_index(current: usize, delta: isize, len: usize) -> usize {
    if len == 0 {
        return 0;
    }
    (current as isize + delta).clamp(0, len as isize - 1) as usize
}

#[cfg(test)]
mod tests {
    use super::{ComposerField, clamp_index, looks_like_url, move_index};

    #[test]
    fn field_cycles_forward_and_back() {
        assert_eq!(ComposerField::Title.next(), ComposerField::Url);
        assert_eq!(ComposerField::Description.next(), ComposerField::Title);
        assert_eq!(ComposerField::Title.prev(), ComposerField::Description);
    }

    #[test]
    fn url_validation_requires_scheme() {
        assert!(looks_like_url("https://late.sh"));
        assert!(looks_like_url("http://example.com"));
        assert!(!looks_like_url("late.sh"));
        assert!(!looks_like_url("ftp://x"));
    }

    #[test]
    fn clamp_index_handles_empty_list() {
        assert_eq!(clamp_index(4, 0), 0);
        assert_eq!(clamp_index(9, 3), 2);
    }

    #[test]
    fn move_index_clamps_at_edges() {
        assert_eq!(move_index(0, -1, 5), 0);
        assert_eq!(move_index(4, 1, 5), 4);
        assert_eq!(move_index(2, 2, 5), 4);
    }
}
