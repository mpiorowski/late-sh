use chrono::{DateTime, Utc};
use late_core::models::work_profile::WorkProfileParams;
use ratatui_textarea::{TextArea, WrapMode};
use tokio::sync::{broadcast, watch};
use uuid::Uuid;

use crate::app::common::{composer, primitives::Banner};

use super::svc::{self, WorkEvent, WorkFeedItem, WorkService, WorkSnapshot};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ComposerField {
    Headline,
    Status,
    Type,
    Location,
    Links,
    Skills,
    Includes,
    Summary,
}

impl ComposerField {
    pub(crate) fn next(self) -> Self {
        match self {
            Self::Headline => Self::Status,
            Self::Status => Self::Type,
            Self::Type => Self::Location,
            Self::Location => Self::Links,
            Self::Links => Self::Skills,
            Self::Skills => Self::Includes,
            Self::Includes => Self::Summary,
            Self::Summary => Self::Headline,
        }
    }

    pub(crate) fn prev(self) -> Self {
        match self {
            Self::Headline => Self::Summary,
            Self::Status => Self::Headline,
            Self::Type => Self::Status,
            Self::Location => Self::Type,
            Self::Links => Self::Location,
            Self::Skills => Self::Links,
            Self::Includes => Self::Skills,
            Self::Summary => Self::Includes,
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Headline => "Headline",
            Self::Status => "Status",
            Self::Type => "Type",
            Self::Location => "Location",
            Self::Links => "Links",
            Self::Skills => "Skills",
            Self::Includes => "Include",
            Self::Summary => "Summary",
        }
    }

    pub(crate) fn placeholder(self) -> &'static str {
        match self {
            Self::Headline => "Rust backend engineer",
            Self::Status => "open | casual | not-looking",
            Self::Type => "contract, full-time, freelance",
            Self::Location => "EU remote, Warsaw, US overlap",
            Self::Links => "https://github.com/you, https://cv.example",
            Self::Skills => "rust, postgres, axum",
            Self::Includes => "bio, fetch, showcases",
            Self::Summary => "What work are you looking for?",
        }
    }
}

pub struct State {
    service: WorkService,
    user_id: Uuid,
    is_admin: bool,
    items: Vec<WorkFeedItem>,
    selected: usize,
    snapshot_rx: watch::Receiver<WorkSnapshot>,
    event_rx: broadcast::Receiver<WorkEvent>,
    composing: bool,
    editing_id: Option<Uuid>,
    editing_slug: Option<String>,
    field: ComposerField,
    headline: TextArea<'static>,
    status: TextArea<'static>,
    work_type: TextArea<'static>,
    location: TextArea<'static>,
    links: TextArea<'static>,
    skills: TextArea<'static>,
    includes: TextArea<'static>,
    summary: TextArea<'static>,
    submitted: bool,
    unread_count: i64,
    last_read_at: Option<DateTime<Utc>>,
    marker_read_at: Option<DateTime<Utc>>,
}

impl State {
    pub fn new(service: WorkService, user_id: Uuid, is_admin: bool) -> Self {
        let state = Self::new_without_initial_load(service, user_id, is_admin);
        state.list();
        state.refresh_unread_count();
        state
    }

    pub fn new_without_initial_load(service: WorkService, user_id: Uuid, is_admin: bool) -> Self {
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
            editing_slug: None,
            field: ComposerField::Headline,
            headline: new_single_line(ComposerField::Headline.placeholder()),
            status: new_single_line(ComposerField::Status.placeholder()),
            work_type: new_single_line(ComposerField::Type.placeholder()),
            location: new_single_line(ComposerField::Location.placeholder()),
            links: new_single_line(ComposerField::Links.placeholder()),
            skills: new_single_line(ComposerField::Skills.placeholder()),
            includes: new_single_line(ComposerField::Includes.placeholder()),
            summary: new_multi_line(ComposerField::Summary.placeholder()),
            submitted: false,
            unread_count: 0,
            last_read_at: None,
            marker_read_at: None,
        }
    }

    pub fn is_admin(&self) -> bool {
        self.is_admin
    }

    pub fn set_is_admin(&mut self, is_admin: bool) {
        self.is_admin = is_admin;
    }

    pub fn list(&self) {
        self.service.list_task();
        self.refresh_unread_count();
    }

    pub fn refresh_unread_count(&self) {
        self.service.refresh_unread_count_task(self.user_id);
    }

    pub fn mark_read(&mut self) {
        self.marker_read_at = Some(Utc::now());
        self.unread_count = 0;
        self.service.mark_read_task(self.user_id);
    }

    pub fn all_items(&self) -> &[WorkFeedItem] {
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

    pub fn selected_item(&self) -> Option<&WorkFeedItem> {
        self.items.get(self.selected_index())
    }

    pub fn move_selection(&mut self, delta: isize) {
        self.selected = move_index(self.selected_index(), delta, self.items.len());
    }

    pub fn selected_can_edit(&self) -> bool {
        self.selected_item()
            .map(|item| self.is_admin || item.profile.user_id == self.user_id)
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
            ComposerField::Headline => &self.headline,
            ComposerField::Status => &self.status,
            ComposerField::Type => &self.work_type,
            ComposerField::Location => &self.location,
            ComposerField::Links => &self.links,
            ComposerField::Skills => &self.skills,
            ComposerField::Includes => &self.includes,
            ComposerField::Summary => &self.summary,
        }
    }

    pub(crate) fn field_is_empty(&self, field: ComposerField) -> bool {
        let lines = self.field_textarea(field).lines();
        lines.len() == 1 && lines[0].is_empty()
    }

    pub fn refresh_composer_theme(&mut self) {
        for field in [
            ComposerField::Headline,
            ComposerField::Status,
            ComposerField::Type,
            ComposerField::Location,
            ComposerField::Links,
            ComposerField::Skills,
            ComposerField::Includes,
            ComposerField::Summary,
        ] {
            let active = self.composing && field == self.field;
            self.apply_field_style(field, active);
        }
    }

    fn apply_field_style(&mut self, field: ComposerField, cursor_visible: bool) {
        composer::apply_themed_textarea_style(self.field_textarea_mut(field), cursor_visible);
    }

    pub fn start_composing(&mut self) {
        if let Some((id, slug)) = self.own_profile_id_slug() {
            let _ = self.start_editing_profile(id, slug);
            return;
        }
        self.reset_composer();
        self.composing = true;
        self.editing_id = None;
        self.editing_slug = None;
        self.field = ComposerField::Headline;
        self.refresh_composer_theme();
    }

    pub fn start_editing_selected(&mut self) -> bool {
        let Some(item) = self.selected_item() else {
            return false;
        };
        if !(self.is_admin || item.profile.user_id == self.user_id) {
            return false;
        }
        self.start_editing_profile(item.profile.id, item.profile.slug.clone())
    }

    fn start_editing_profile(&mut self, id: Uuid, slug: String) -> bool {
        let Some(item) = self.items.iter().find(|item| item.profile.id == id) else {
            return false;
        };
        let profile = item.profile.clone();
        self.reset_composer();
        self.headline.insert_str(profile.headline);
        self.status.insert_str(profile.status);
        self.work_type.insert_str(profile.work_type);
        self.location.insert_str(profile.location);
        self.links.insert_str(profile.links.join(", "));
        self.skills.insert_str(profile.skills.join(", "));
        let includes = include_flags_text(
            profile.include_bio,
            profile.include_late_fetch,
            profile.include_showcases,
        );
        self.includes.insert_str(includes);
        self.summary.insert_str(profile.summary);
        self.composing = true;
        self.editing_id = Some(id);
        self.editing_slug = Some(slug);
        self.field = ComposerField::Headline;
        self.refresh_composer_theme();
        true
    }

    pub fn stop_composing(&mut self) {
        self.composing = false;
        self.editing_id = None;
        self.editing_slug = None;
        self.reset_composer();
    }

    fn reset_composer(&mut self) {
        self.headline = new_single_line(ComposerField::Headline.placeholder());
        self.status = new_single_line(ComposerField::Status.placeholder());
        self.work_type = new_single_line(ComposerField::Type.placeholder());
        self.location = new_single_line(ComposerField::Location.placeholder());
        self.links = new_single_line(ComposerField::Links.placeholder());
        self.skills = new_single_line(ComposerField::Skills.placeholder());
        self.includes = new_single_line(ComposerField::Includes.placeholder());
        self.summary = new_multi_line(ComposerField::Summary.placeholder());
        self.refresh_composer_theme();
    }

    pub fn cycle_field(&mut self, forward: bool) {
        self.field = if forward {
            self.field.next()
        } else {
            self.field.prev()
        };
        self.refresh_composer_theme();
    }

    pub fn delete_selected(&mut self) -> Option<Banner> {
        let item = self.selected_item()?;
        if !(self.is_admin || item.profile.user_id == self.user_id) {
            return Some(Banner::error("not your work profile"));
        }
        self.service
            .delete_task(self.user_id, item.profile.id, self.is_admin);
        None
    }

    pub fn submit(&mut self) -> Option<Banner> {
        let headline = self.headline.lines().join(" ").trim().to_string();
        let status = normalize_status(&self.status.lines().join(" "));
        let work_type = self.work_type.lines().join(" ").trim().to_string();
        let location = self.location.lines().join(" ").trim().to_string();
        let links = svc::parse_links(&self.links.lines().join(","));
        let skills = svc::parse_words(&self.skills.lines().join(","), 12);
        let (include_bio, include_late_fetch, include_showcases) =
            parse_includes(&self.includes.lines().join(","));
        let summary = self.summary.lines().join("\n").trim().to_string();

        if headline.is_empty() {
            self.field = ComposerField::Headline;
            self.refresh_composer_theme();
            return Some(Banner::error("headline required"));
        }
        if headline.chars().count() > 120 {
            return Some(Banner::error("headline too long (max 120)"));
        }
        if status.is_none() {
            self.field = ComposerField::Status;
            self.refresh_composer_theme();
            return Some(Banner::error("status must be open, casual, or not-looking"));
        }
        if work_type.is_empty() {
            self.field = ComposerField::Type;
            self.refresh_composer_theme();
            return Some(Banner::error("work type required"));
        }
        if location.is_empty() {
            self.field = ComposerField::Location;
            self.refresh_composer_theme();
            return Some(Banner::error("location required"));
        }
        if links.is_empty() {
            self.field = ComposerField::Links;
            self.refresh_composer_theme();
            return Some(Banner::error("at least one http(s) link required"));
        }
        if summary.is_empty() {
            self.field = ComposerField::Summary;
            self.refresh_composer_theme();
            return Some(Banner::error("summary required"));
        }
        if summary.chars().count() > 1000 {
            return Some(Banner::error("summary too long (max 1000)"));
        }

        let params = WorkProfileParams {
            user_id: self.user_id,
            slug: self
                .editing_slug
                .clone()
                .unwrap_or_else(generate_public_slug),
            headline,
            status: status.expect("status validated above").to_string(),
            work_type,
            location,
            links,
            skills,
            summary,
            include_bio,
            include_late_fetch,
            include_showcases,
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

    pub fn copy_selected_profile_url(&self, base_url: &str) -> Option<String> {
        let item = self.selected_item()?;
        Some(profile_url(base_url, &item.profile.slug))
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
                    WorkEvent::Created { user_id } if self.user_id == user_id && self.submitted => {
                        self.submitted = false;
                        self.stop_composing();
                        banner = Some(Banner::success("Work profile shared!"));
                    }
                    WorkEvent::Updated { user_id } if self.user_id == user_id && self.submitted => {
                        self.submitted = false;
                        self.stop_composing();
                        banner = Some(Banner::success("Work profile updated."));
                    }
                    WorkEvent::Deleted { user_id } if self.user_id == user_id => {
                        banner = Some(Banner::success("Work profile deleted."));
                    }
                    WorkEvent::Failed { user_id, error } if self.user_id == user_id => {
                        self.submitted = false;
                        banner = Some(Banner::error(&format!("Failed: {error}")));
                    }
                    WorkEvent::UnreadCountUpdated {
                        user_id,
                        unread_count,
                        last_read_at,
                    } if self.user_id == user_id => {
                        self.unread_count = unread_count;
                        self.last_read_at = last_read_at;
                        if unread_count == 0 {
                            self.marker_read_at = last_read_at;
                        }
                    }
                    WorkEvent::NewWorkProfilesAvailable {
                        user_id,
                        unread_count,
                    } if self.user_id == user_id => {
                        let increased = unread_count > self.unread_count;
                        self.unread_count = unread_count;
                        if increased {
                            let noun = if unread_count == 1 {
                                "work profile"
                            } else {
                                "work profiles"
                            };
                            banner = Some(Banner::success(&format!("{unread_count} new {noun}")));
                        }
                    }
                    _ => {}
                },
                Err(broadcast::error::TryRecvError::Empty) => break,
                Err(e) => {
                    tracing::error!(%e, "failed to receive work event");
                    break;
                }
            }
        }
        banner
    }

    pub(crate) fn field_input(&mut self, field: ComposerField, input: ratatui_textarea::Input) {
        self.field_textarea_mut(field).input(input);
    }

    pub(crate) fn field_insert_char(&mut self, ch: char) {
        if ch == '\n' {
            self.field_newline();
            return;
        }
        self.field_textarea_mut(self.field).insert_char(ch);
    }

    pub(crate) fn field_delete_char(&mut self) {
        self.field_textarea_mut(self.field).delete_char();
    }

    pub(crate) fn field_paste(&mut self) {
        self.field_textarea_mut(self.field).paste();
    }

    pub(crate) fn field_undo(&mut self) {
        self.field_textarea_mut(self.field).undo();
    }

    pub(crate) fn field_clear_line(&mut self) {
        self.field_textarea_mut(self.field).delete_line_by_head();
    }

    pub(crate) fn field_newline(&mut self) {
        if matches!(self.field, ComposerField::Summary) {
            self.summary.insert_newline();
        }
    }

    fn field_textarea_mut(&mut self, field: ComposerField) -> &mut TextArea<'static> {
        match field {
            ComposerField::Headline => &mut self.headline,
            ComposerField::Status => &mut self.status,
            ComposerField::Type => &mut self.work_type,
            ComposerField::Location => &mut self.location,
            ComposerField::Links => &mut self.links,
            ComposerField::Skills => &mut self.skills,
            ComposerField::Includes => &mut self.includes,
            ComposerField::Summary => &mut self.summary,
        }
    }

    fn own_profile_id_slug(&self) -> Option<(Uuid, String)> {
        self.items
            .iter()
            .find(|item| item.profile.user_id == self.user_id)
            .map(|item| (item.profile.id, item.profile.slug.clone()))
    }
}

fn new_single_line(placeholder: &str) -> TextArea<'static> {
    composer::new_themed_textarea(placeholder.to_string(), WrapMode::Glyph, false)
}

fn new_multi_line(placeholder: &str) -> TextArea<'static> {
    composer::new_themed_textarea(placeholder.to_string(), WrapMode::Word, false)
}

fn normalize_status(input: &str) -> Option<&'static str> {
    match input.trim().to_ascii_lowercase().replace('_', "-").as_str() {
        "open" | "yes" | "available" => Some("open"),
        "casual" | "maybe" | "listening" => Some("casual"),
        "not-looking" | "not looking" | "closed" | "no" => Some("not-looking"),
        _ => None,
    }
}

pub fn status_label(status: &str) -> &'static str {
    match status {
        "open" => "open",
        "casual" => "casual",
        "not-looking" => "not looking",
        _ => "unknown",
    }
}

fn parse_includes(input: &str) -> (bool, bool, bool) {
    let raw = input.trim();
    if raw.is_empty() {
        return (true, true, true);
    }
    let lower = raw.to_ascii_lowercase();
    (
        lower.contains("bio"),
        lower.contains("fetch") || lower.contains("late.fetch"),
        lower.contains("showcase"),
    )
}

fn include_flags_text(bio: bool, fetch: bool, showcases: bool) -> String {
    let mut out = Vec::new();
    if bio {
        out.push("bio");
    }
    if fetch {
        out.push("fetch");
    }
    if showcases {
        out.push("showcases");
    }
    out.join(", ")
}

fn generate_public_slug() -> String {
    let id = Uuid::now_v7().simple().to_string();
    format!("w_{}", &id[..12])
}

pub(crate) fn profile_url(base_url: &str, slug: &str) -> String {
    format!("{}/profiles/{slug}", base_url.trim_end_matches('/'))
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
    use super::{ComposerField, normalize_status, parse_includes};

    #[test]
    fn field_cycles_forward_and_back() {
        assert_eq!(ComposerField::Headline.next(), ComposerField::Status);
        assert_eq!(ComposerField::Summary.next(), ComposerField::Headline);
        assert_eq!(ComposerField::Headline.prev(), ComposerField::Summary);
    }

    #[test]
    fn status_normalization_accepts_aliases() {
        assert_eq!(normalize_status("available"), Some("open"));
        assert_eq!(normalize_status("maybe"), Some("casual"));
        assert_eq!(normalize_status("not looking"), Some("not-looking"));
        assert_eq!(normalize_status("busy"), None);
    }

    #[test]
    fn includes_default_to_everything() {
        assert_eq!(parse_includes(""), (true, true, true));
        assert_eq!(parse_includes("bio, showcases"), (true, false, true));
    }
}
