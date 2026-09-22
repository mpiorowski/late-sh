//! The post form: a posting written on the shelf itself, by anyone, with
//! the same fields the press reads into a card. Pure: rows, typing,
//! validation into a `NewPosting`. The keys are in `input.rs`, the modal
//! in `ui.rs`, the write in `svc.rs`.

use late_core::models::job_posting::{NewPosting, RemoteKind};
use late_core::vocab;
use ratatui_textarea::{CursorMove, TextArea, WrapMode};
use uuid::Uuid;

use crate::app::chat::work::svc::looks_like_url;
use crate::app::common::composer::{
    apply_themed_textarea_style, new_themed_textarea, set_themed_textarea_cursor_visible,
};

pub(crate) const COMPANY_MAX: usize = 80;
pub(crate) const TITLE_MAX: usize = 120;
pub(crate) const LINK_MAX: usize = 2000;
pub(crate) const REGIONS_MAX: usize = 120;
pub(crate) const PAY_MAX: usize = 80;
/// The card's excerpt cap, the same the press cuts a read at.
pub(crate) const EXCERPT_MAX: usize = 400;
/// Regions a posting names at most; the card prints them on one line.
pub(crate) const REGIONS_LIMIT: usize = 6;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PostField {
    Company,
    Title,
    Link,
    Scope,
    Regions,
    Tags,
    Pay,
    Excerpt,
}

pub(crate) const POST_FIELDS: [PostField; 8] = [
    PostField::Company,
    PostField::Title,
    PostField::Link,
    PostField::Scope,
    PostField::Regions,
    PostField::Tags,
    PostField::Pay,
    PostField::Excerpt,
];

/// How a row is edited: typed on one line, typed over several, cycled,
/// or picked from the tag vocabulary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PostKind {
    Text,
    Multi,
    Choice,
    Tags,
}

impl PostField {
    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Company => "company",
            Self::Title => "role",
            Self::Link => "link",
            Self::Scope => "where",
            Self::Regions => "regions",
            Self::Tags => "stack",
            Self::Pay => "pay",
            Self::Excerpt => "excerpt",
        }
    }

    pub(crate) const fn placeholder(self) -> &'static str {
        match self {
            Self::Company => "Acme",
            Self::Title => "Senior Rust Engineer",
            Self::Link => "https://acme.example/jobs/rust (where to apply)",
            Self::Scope => "",
            Self::Regions => "EU, US, UTC-3 to UTC+3 (empty when worldwide)",
            Self::Tags => "Enter picks tags from the list",
            Self::Pay => "€90k to €120k, or empty",
            Self::Excerpt => {
                "What the company does and what the role is, in the third person (Alt+Enter for a new line)"
            }
        }
    }

    pub(crate) const fn kind(self) -> PostKind {
        match self {
            Self::Scope => PostKind::Choice,
            Self::Tags => PostKind::Tags,
            Self::Excerpt => PostKind::Multi,
            Self::Company | Self::Title | Self::Link | Self::Regions | Self::Pay => PostKind::Text,
        }
    }

    pub(crate) const fn max_len(self) -> usize {
        match self {
            Self::Company => COMPANY_MAX,
            Self::Title => TITLE_MAX,
            Self::Link => LINK_MAX,
            Self::Scope | Self::Tags => 0,
            Self::Regions => REGIONS_MAX,
            Self::Pay => PAY_MAX,
            Self::Excerpt => EXCERPT_MAX,
        }
    }

    /// Rows the field takes on screen.
    pub(crate) const fn height(self) -> u16 {
        match self {
            Self::Excerpt => 4,
            Self::Tags => 2,
            Self::Company | Self::Title | Self::Link | Self::Scope | Self::Regions | Self::Pay => 1,
        }
    }
}

/// The scope row's three choices, in cycling order.
pub(crate) fn next_scope(kind: RemoteKind, forward: bool) -> RemoteKind {
    match (kind, forward) {
        (RemoteKind::Worldwide, true) | (RemoteKind::Hybrid, false) => RemoteKind::Regions,
        (RemoteKind::Regions, true) | (RemoteKind::Worldwide, false) => RemoteKind::Hybrid,
        (RemoteKind::Hybrid, true) | (RemoteKind::Regions, false) => RemoteKind::Worldwide,
    }
}

/// What the scope row prints for each choice.
pub(crate) const fn scope_choice_label(kind: RemoteKind) -> &'static str {
    match kind {
        RemoteKind::Worldwide => "remote, worldwide",
        RemoteKind::Regions => "remote, within regions",
        RemoteKind::Hybrid => "hybrid, office days",
    }
}

/// What the form says went wrong: under a row, or about the save.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PostError {
    pub(crate) field: Option<PostField>,
    pub(crate) message: String,
}

/// The typed values, for validation.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct PostValues {
    pub(crate) company: String,
    pub(crate) title: String,
    pub(crate) link: String,
    pub(crate) scope: Option<RemoteKind>,
    pub(crate) regions: String,
    pub(crate) tags: Vec<String>,
    pub(crate) pay: String,
    pub(crate) excerpt: String,
}

pub(crate) struct PostForm {
    open: bool,
    row: usize,
    editing: bool,
    /// A save in flight: keys wait for the answer.
    pending: bool,
    error: Option<PostError>,
    scope: RemoteKind,
    tags: Vec<String>,
    company: TextArea<'static>,
    title: TextArea<'static>,
    link: TextArea<'static>,
    regions: TextArea<'static>,
    pay: TextArea<'static>,
    excerpt: TextArea<'static>,
}

fn text_input(field: PostField) -> TextArea<'static> {
    let wrap = match field.kind() {
        PostKind::Multi => WrapMode::Word,
        PostKind::Text | PostKind::Choice | PostKind::Tags => WrapMode::None,
    };
    new_themed_textarea(field.placeholder(), wrap, false)
}

fn joined(ta: &TextArea<'static>, sep: &str) -> String {
    ta.lines().join(sep).trim().to_string()
}

impl Default for PostForm {
    fn default() -> Self {
        Self {
            open: false,
            row: 0,
            editing: false,
            pending: false,
            error: None,
            scope: RemoteKind::Worldwide,
            tags: Vec::new(),
            company: text_input(PostField::Company),
            title: text_input(PostField::Title),
            link: text_input(PostField::Link),
            regions: text_input(PostField::Regions),
            pay: text_input(PostField::Pay),
            excerpt: text_input(PostField::Excerpt),
        }
    }
}

impl PostForm {
    /// A fresh form on the first row, typing into it.
    pub(crate) fn open(&mut self) {
        *self = Self::default();
        self.open = true;
        self.start_editing();
    }

    pub(crate) fn close(&mut self) {
        *self = Self::default();
    }

    pub(crate) fn is_open(&self) -> bool {
        self.open
    }

    pub(crate) fn row(&self) -> usize {
        self.row
    }

    pub(crate) fn editing(&self) -> bool {
        self.editing
    }

    pub(crate) fn pending(&self) -> bool {
        self.pending
    }

    pub(crate) fn error(&self) -> Option<&PostError> {
        self.error.as_ref()
    }

    pub(crate) fn scope(&self) -> RemoteKind {
        self.scope
    }

    pub(crate) fn tags(&self) -> &[String] {
        &self.tags
    }

    /// What the picker chose for the stack row.
    pub(crate) fn set_tags(&mut self, tags: Vec<String>) {
        self.tags = tags;
        self.error = None;
    }

    pub(crate) fn active_field(&self) -> PostField {
        POST_FIELDS[self.row.min(POST_FIELDS.len() - 1)]
    }

    pub(crate) fn field(&self, field: PostField) -> &TextArea<'static> {
        match field {
            PostField::Company => &self.company,
            PostField::Title => &self.title,
            PostField::Link => &self.link,
            PostField::Regions => &self.regions,
            PostField::Pay => &self.pay,
            PostField::Excerpt => &self.excerpt,
            PostField::Scope | PostField::Tags => panic!("{} has no text", field.label()),
        }
    }

    pub(crate) fn field_mut(&mut self, field: PostField) -> &mut TextArea<'static> {
        match field {
            PostField::Company => &mut self.company,
            PostField::Title => &mut self.title,
            PostField::Link => &mut self.link,
            PostField::Regions => &mut self.regions,
            PostField::Pay => &mut self.pay,
            PostField::Excerpt => &mut self.excerpt,
            PostField::Scope | PostField::Tags => panic!("{} has no text", field.label()),
        }
    }

    /// The text of a row as it shows when not being typed into.
    pub(crate) fn field_text(&self, field: PostField) -> String {
        match field.kind() {
            PostKind::Choice => scope_choice_label(self.scope).to_string(),
            PostKind::Tags => self.tags.join(" · "),
            PostKind::Text => joined(self.field(field), " "),
            PostKind::Multi => joined(self.field(field), "\n"),
        }
    }

    pub(crate) fn move_row(&mut self, delta: isize) {
        let last = POST_FIELDS.len() as isize - 1;
        self.row = (self.row as isize + delta).clamp(0, last) as usize;
        self.sync_cursors();
    }

    /// Enter on a row: type into it, or cycle a choice. A tag row is the
    /// input layer's to open the picker on.
    pub(crate) fn start_editing(&mut self) {
        self.error = None;
        let field = self.active_field();
        match field.kind() {
            PostKind::Choice => self.cycle_scope(true),
            PostKind::Tags => {}
            PostKind::Text | PostKind::Multi => {
                self.editing = true;
                self.field_mut(field).move_cursor(CursorMove::End);
                self.sync_cursors();
            }
        }
    }

    pub(crate) fn stop_editing(&mut self) {
        self.editing = false;
        self.sync_cursors();
    }

    /// Commit the row being typed and step to the next, the way Tab walks
    /// a form; from the last row it just stops.
    pub(crate) fn commit_and_advance(&mut self, forward: bool) {
        self.editing = false;
        let last = POST_FIELDS.len() - 1;
        let next = if forward {
            (self.row < last).then_some(self.row + 1)
        } else {
            (self.row > 0).then_some(self.row - 1)
        };
        match next {
            Some(row) => {
                self.row = row;
                match self.active_field().kind() {
                    PostKind::Text | PostKind::Multi => self.start_editing(),
                    PostKind::Choice | PostKind::Tags => self.sync_cursors(),
                }
            }
            None => self.sync_cursors(),
        }
    }

    /// `←/→` on the scope row.
    pub(crate) fn cycle_scope(&mut self, forward: bool) {
        if self.active_field() == PostField::Scope {
            self.scope = next_scope(self.scope, forward);
        }
    }

    /// Esc: a row being typed stops; otherwise the form closes. Returns
    /// whether it closed.
    pub(crate) fn escape(&mut self) -> bool {
        if self.editing {
            self.stop_editing();
            return false;
        }
        self.close();
        true
    }

    pub(crate) fn values(&self) -> PostValues {
        PostValues {
            company: joined(&self.company, " "),
            title: joined(&self.title, " "),
            link: joined(&self.link, " "),
            scope: Some(self.scope),
            regions: joined(&self.regions, ","),
            tags: self.tags.clone(),
            pay: joined(&self.pay, " "),
            excerpt: joined(&self.excerpt, "\n"),
        }
    }

    /// Ctrl+S: the posting when the form holds one, with the save marked
    /// in flight; on a bad row the cursor lands there and says why.
    pub(crate) fn submit(&mut self, by: Uuid) -> Option<NewPosting> {
        self.editing = false;
        match validate_post(&self.values(), by) {
            Ok(posting) => {
                self.error = None;
                self.pending = true;
                self.sync_cursors();
                Some(posting)
            }
            Err((field, message)) => {
                self.row = POST_FIELDS
                    .iter()
                    .position(|known| *known == field)
                    .unwrap_or(0);
                self.error = Some(PostError {
                    field: Some(field),
                    message: message.to_string(),
                });
                self.sync_cursors();
                None
            }
        }
    }

    /// The save answered without a row: the form stays with the reason.
    pub(crate) fn settle(&mut self, message: Option<&str>) {
        self.pending = false;
        self.error = message.map(|message| PostError {
            field: None,
            message: message.to_string(),
        });
    }

    fn sync_cursors(&mut self) {
        let active = self.editing.then(|| self.active_field());
        for field in POST_FIELDS {
            if matches!(field.kind(), PostKind::Text | PostKind::Multi) {
                let visible = active == Some(field);
                set_themed_textarea_cursor_visible(self.field_mut(field), visible);
            }
        }
    }

    /// Re-apply the theme to every text row after the palette changes.
    pub(crate) fn refresh_theme(&mut self) {
        let active = self.editing.then(|| self.active_field());
        for field in POST_FIELDS {
            if matches!(field.kind(), PostKind::Text | PostKind::Multi) {
                let visible = active == Some(field);
                apply_themed_textarea_style(self.field_mut(field), visible);
            }
        }
    }
}

/// The form's rules, in row order. Regions are split on commas; a scope
/// within regions or hybrid needs at least one, worldwide keeps none.
pub(crate) fn validate_post(
    values: &PostValues,
    by: Uuid,
) -> Result<NewPosting, (PostField, &'static str)> {
    let company = values.company.trim();
    if company.is_empty() {
        return Err((PostField::Company, "company required"));
    }
    if company.chars().count() > COMPANY_MAX {
        return Err((PostField::Company, "company too long (max 80)"));
    }
    let title = values.title.trim();
    if title.is_empty() {
        return Err((PostField::Title, "role required"));
    }
    if title.chars().count() > TITLE_MAX {
        return Err((PostField::Title, "role too long (max 120)"));
    }
    let link = values.link.trim();
    if !looks_like_url(link) {
        return Err((PostField::Link, "link must start with http:// or https://"));
    }
    if link.chars().count() > LINK_MAX {
        return Err((PostField::Link, "link too long"));
    }
    let Some(scope) = values.scope else {
        return Err((PostField::Scope, "pick where the role is done"));
    };
    let regions: Vec<String> = values
        .regions
        .split(',')
        .map(str::trim)
        .filter(|region| !region.is_empty())
        .take(REGIONS_LIMIT)
        .map(ToString::to_string)
        .collect();
    let regions = match scope {
        RemoteKind::Worldwide => Vec::new(),
        RemoteKind::Regions | RemoteKind::Hybrid => {
            if regions.is_empty() {
                return Err((
                    PostField::Regions,
                    "name the countries, continents, or time zones, or pick worldwide",
                ));
            }
            regions
        }
    };
    let tags = vocab::normalize(&values.tags, vocab::TAG_LIMIT).tags;
    let pay = values.pay.trim();
    if pay.chars().count() > PAY_MAX {
        return Err((PostField::Pay, "pay too long (max 80)"));
    }
    let excerpt = values
        .excerpt
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if excerpt.is_empty() {
        return Err((PostField::Excerpt, "excerpt required"));
    }
    if excerpt.chars().count() > EXCERPT_MAX {
        return Err((PostField::Excerpt, "excerpt too long (max 400)"));
    }
    Ok(NewPosting {
        posted_by: by,
        url: link.to_string(),
        company: company.to_string(),
        title: title.to_string(),
        remote_kind: scope,
        regions,
        tags,
        pay: pay.to_string(),
        excerpt,
    })
}
