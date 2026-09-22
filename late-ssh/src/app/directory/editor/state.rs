use std::cell::Cell;

use late_core::models::{
    profile::Profile,
    showcase::ShowcaseParams,
    work_profile::{WorkProfile, WorkProfileParams, WorkStatus, WorkType},
};
use late_core::vocab;
use ratatui::layout::Rect;
use ratatui_textarea::{CursorMove, TextArea, WrapMode};
use uuid::Uuid;

use crate::app::chat::{showcase::svc::ShowcaseFeedItem, work::svc};
use crate::app::common::composer::{new_themed_textarea, set_themed_textarea_cursor_visible};

/// The card column caps (migration 041) and the profile's (settings modal).
pub(crate) const HEADLINE_MAX: usize = 120;
pub(crate) const LOCATION_MAX: usize = 120;
pub(crate) const CONTACT_MAX: usize = 200;
pub(crate) const LINKS_MAX: usize = 600;
pub(crate) const SUMMARY_MAX: usize = 1000;
pub(crate) const BIO_MAX: usize = 1000;
pub(crate) const SYSTEM_FIELD_MAX: usize = 48;
pub(crate) const TITLE_MAX: usize = 120;
pub(crate) const URL_MAX: usize = 2000;
pub(crate) const TAGS_MAX: usize = 200;
pub(crate) const DESCRIPTION_MAX: usize = 800;
/// `skills` and `skills_tags` both cap at 12 (migrations 041 and 192), the
/// vocabulary's cap.
pub(crate) const SKILLS_LIMIT: usize = vocab::TAG_LIMIT;

/// The most rows any page draws; the click map is sized to it.
pub(crate) const MAX_ROWS: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Page {
    Card,
    About,
    Projects,
}

impl Page {
    pub(crate) const fn title(self) -> &'static str {
        match self {
            Self::Card => "card",
            Self::About => "about",
            Self::Projects => "projects",
        }
    }
}

/// Whose profile the modal is editing, which decides the pages it shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Scope {
    /// The viewer's own: every page.
    Own,
    /// A moderator on someone else's work card: the card page alone.
    CardOf { owner: Uuid, username: String },
    /// A moderator on someone else's project: that project's form alone.
    ProjectOf { owner: Uuid, username: String },
}

impl Scope {
    pub(crate) fn pages(&self) -> &'static [Page] {
        match self {
            Self::Own => &[Page::Card, Page::About, Page::Projects],
            Self::CardOf { .. } => &[Page::Card],
            Self::ProjectOf { .. } => &[Page::Projects],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Field {
    Headline,
    Status,
    Type,
    Location,
    Contact,
    Links,
    Skills,
    Summary,
    Bio,
    Ide,
    Terminal,
    Os,
    Langs,
    Title,
    Url,
    Tags,
    Description,
}

/// How a row is edited: typed on one line, typed over several, cycled, or
/// picked from the tag vocabulary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FieldKind {
    Text,
    Multi,
    Choice,
    Tags,
}

pub(crate) const CARD_FIELDS: [Field; 8] = [
    Field::Headline,
    Field::Status,
    Field::Type,
    Field::Location,
    Field::Contact,
    Field::Links,
    Field::Skills,
    Field::Summary,
];
pub(crate) const ABOUT_FIELDS: [Field; 5] = [
    Field::Bio,
    Field::Ide,
    Field::Terminal,
    Field::Os,
    Field::Langs,
];
pub(crate) const PROJECT_FIELDS: [Field; 4] =
    [Field::Title, Field::Url, Field::Tags, Field::Description];

impl Field {
    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Headline => "headline",
            Self::Status => "status",
            Self::Type => "type",
            Self::Location => "location",
            Self::Contact => "contact",
            Self::Links => "links",
            Self::Skills => "skills",
            Self::Summary => "summary",
            Self::Bio => "bio",
            Self::Ide => "ide",
            Self::Terminal => "terminal",
            Self::Os => "os",
            Self::Langs => "langs",
            Self::Title => "title",
            Self::Url => "url",
            Self::Tags => "tags",
            Self::Description => "description",
        }
    }

    pub(crate) const fn placeholder(self) -> &'static str {
        match self {
            Self::Headline => "Rust backend engineer",
            Self::Status | Self::Type => "",
            Self::Location => "EU remote, Warsaw, US overlap",
            Self::Contact => "email, @handle, or DM on late.sh",
            Self::Links => "https://github.com/you, https://cv.example",
            Self::Skills => "Enter picks tags from the list",
            Self::Summary => "What work are you looking for? (Alt+Enter for a new line)",
            Self::Bio => "A few lines about you, Markdown welcome (Alt+Enter for a new line)",
            Self::Ide => "nvim, vscode",
            Self::Terminal => "alacritty",
            Self::Os => "nixos 26.11",
            Self::Langs => "Enter picks languages from the list",
            Self::Title => "Project name",
            Self::Url => "https://...",
            Self::Tags => "rust, cli, game",
            Self::Description => "What is it? Why should we look? (Alt+Enter for a new line)",
        }
    }

    pub(crate) const fn kind(self) -> FieldKind {
        match self {
            Self::Status | Self::Type => FieldKind::Choice,
            Self::Skills | Self::Langs => FieldKind::Tags,
            Self::Summary | Self::Bio | Self::Description => FieldKind::Multi,
            Self::Headline
            | Self::Location
            | Self::Contact
            | Self::Links
            | Self::Ide
            | Self::Terminal
            | Self::Os
            | Self::Title
            | Self::Url
            | Self::Tags => FieldKind::Text,
        }
    }

    pub(crate) const fn max_len(self) -> usize {
        match self {
            Self::Headline => HEADLINE_MAX,
            Self::Status | Self::Type | Self::Skills | Self::Langs => 0,
            Self::Location => LOCATION_MAX,
            Self::Contact => CONTACT_MAX,
            Self::Links => LINKS_MAX,
            Self::Summary => SUMMARY_MAX,
            Self::Bio => BIO_MAX,
            Self::Ide | Self::Terminal | Self::Os => SYSTEM_FIELD_MAX,
            Self::Title => TITLE_MAX,
            Self::Url => URL_MAX,
            Self::Tags => TAGS_MAX,
            Self::Description => DESCRIPTION_MAX,
        }
    }

    /// Rows the field takes on screen: a typed line, two for a tag list,
    /// or a short block for the long fields.
    pub(crate) const fn height(self) -> u16 {
        match self {
            Self::Skills | Self::Langs => 2,
            Self::Summary | Self::Description => 4,
            Self::Bio => 5,
            Self::Headline
            | Self::Status
            | Self::Type
            | Self::Location
            | Self::Contact
            | Self::Links
            | Self::Ide
            | Self::Terminal
            | Self::Os
            | Self::Title
            | Self::Url
            | Self::Tags => 1,
        }
    }
}

/// A project as the projects page lists it: what the person can pick to
/// edit or delete. Built from the showcase feed at draw and input time so
/// the modal never holds a stale copy.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ProjectRow {
    pub(crate) id: Uuid,
    pub(crate) title: String,
    pub(crate) url: String,
    pub(crate) tags: Vec<String>,
    pub(crate) description: String,
    pub(crate) created: chrono::DateTime<chrono::Utc>,
}

impl ProjectRow {
    pub(crate) fn from_item(item: &ShowcaseFeedItem) -> Self {
        Self {
            id: item.showcase.id,
            title: item.showcase.title.clone(),
            url: item.showcase.url.clone(),
            tags: item.showcase.tags.clone(),
            description: item.showcase.description.clone(),
            created: item.showcase.created,
        }
    }
}

/// The projects page lists `owner`'s projects in feed order (newest first).
pub(crate) fn projects_of(items: &[ShowcaseFeedItem], owner: Uuid) -> Vec<ProjectRow> {
    items
        .iter()
        .filter(|item| item.showcase.user_id == owner)
        .map(ProjectRow::from_item)
        .collect()
}

/// The card page's values, compared against the values at open to know
/// whether closing loses anything.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CardValues {
    pub(crate) headline: String,
    pub(crate) status: WorkStatus,
    pub(crate) work_type: WorkType,
    pub(crate) location: String,
    pub(crate) contact: String,
    pub(crate) links: String,
    /// Canonical tags from the vocabulary, as the picker left them.
    pub(crate) skills: Vec<String>,
    pub(crate) summary: String,
}

impl Default for CardValues {
    fn default() -> Self {
        Self {
            headline: String::new(),
            status: WorkStatus::Open,
            work_type: WorkType::Any,
            location: String::new(),
            contact: String::new(),
            links: String::new(),
            skills: Vec::new(),
            summary: String::new(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct AboutValues {
    pub(crate) bio: String,
    pub(crate) ide: String,
    pub(crate) terminal: String,
    pub(crate) os: String,
    /// Canonical language tags, as the picker left them.
    pub(crate) langs: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ProjectValues {
    pub(crate) title: String,
    pub(crate) url: String,
    pub(crate) tags: String,
    pub(crate) description: String,
}

/// One project being written: a new one, or an existing one by id.
pub(crate) struct ProjectDraft {
    pub(crate) editing: Option<Uuid>,
    /// The list row to return to when the form closes.
    list_index: usize,
    title: TextArea<'static>,
    url: TextArea<'static>,
    tags: TextArea<'static>,
    description: TextArea<'static>,
    baseline: ProjectValues,
}

/// What the projects page is showing: the list, or one project's form.
pub(crate) enum ProjectsView {
    List { selected: usize },
    Form(ProjectDraft),
}

/// What a save writes, in the order the input layer dispatches it. Pure
/// data: the modal validates and builds, the app hands each to its service.
#[derive(Clone, Debug)]
pub(crate) enum Save {
    Card {
        params: WorkProfileParams,
        editing: Option<Uuid>,
    },
    About(AboutValues),
    Project {
        params: ShowcaseParams,
        editing: Option<Uuid>,
    },
}

/// What the input layer does after Esc: nothing visible, ask first, leave
/// the project form for the list, or close the modal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EscapeOutcome {
    Stayed,
    AskedToDiscard,
    LeftProjectForm,
    Closed,
}

pub(crate) struct EditorState {
    open: bool,
    scope: Scope,
    viewer: Uuid,
    page: Page,
    row: usize,
    editing: bool,
    confirm_discard: bool,
    error: Option<(Field, String)>,
    // card
    card_editing: Option<Uuid>,
    card_slug: Option<String>,
    status: WorkStatus,
    work_type: WorkType,
    headline: TextArea<'static>,
    location: TextArea<'static>,
    contact: TextArea<'static>,
    links: TextArea<'static>,
    skills: Vec<String>,
    summary: TextArea<'static>,
    card_baseline: CardValues,
    // about
    bio: TextArea<'static>,
    ide: TextArea<'static>,
    terminal: TextArea<'static>,
    os: TextArea<'static>,
    langs: Vec<String>,
    about_baseline: AboutValues,
    // projects
    projects_view: ProjectsView,
    /// Screen rects of the rows on screen this frame, each with the index it
    /// stands for, so a click lands on the right row of a scrolled list.
    row_rects: Cell<[Option<(usize, Rect)>; MAX_ROWS]>,
}

fn text_input(field: Field) -> TextArea<'static> {
    let wrap = match field.kind() {
        FieldKind::Multi => WrapMode::Word,
        FieldKind::Text | FieldKind::Choice | FieldKind::Tags => WrapMode::None,
    };
    new_themed_textarea(field.placeholder(), wrap, false)
}

fn seeded(field: Field, text: &str) -> TextArea<'static> {
    let mut ta = text_input(field);
    if !text.trim().is_empty() {
        ta.insert_str(text.trim());
    }
    ta.move_cursor(CursorMove::End);
    ta
}

fn joined(ta: &TextArea<'static>, sep: &str) -> String {
    ta.lines().join(sep).trim().to_string()
}

impl ProjectDraft {
    fn new(list_index: usize) -> Self {
        Self {
            editing: None,
            list_index,
            title: text_input(Field::Title),
            url: text_input(Field::Url),
            tags: text_input(Field::Tags),
            description: text_input(Field::Description),
            baseline: ProjectValues::default(),
        }
    }

    fn from_row(row: &ProjectRow, list_index: usize) -> Self {
        let mut draft = Self {
            editing: Some(row.id),
            list_index,
            title: seeded(Field::Title, &row.title),
            url: seeded(Field::Url, &row.url),
            tags: seeded(Field::Tags, &row.tags.join(", ")),
            description: seeded(Field::Description, &row.description),
            baseline: ProjectValues::default(),
        };
        draft.baseline = draft.values();
        draft
    }

    pub(crate) fn values(&self) -> ProjectValues {
        ProjectValues {
            title: joined(&self.title, " "),
            url: joined(&self.url, ""),
            tags: joined(&self.tags, ","),
            description: joined(&self.description, "\n"),
        }
    }

    fn dirty(&self) -> bool {
        self.values() != self.baseline
    }

    fn field(&self, field: Field) -> Option<&TextArea<'static>> {
        match field {
            Field::Title => Some(&self.title),
            Field::Url => Some(&self.url),
            Field::Tags => Some(&self.tags),
            Field::Description => Some(&self.description),
            Field::Headline
            | Field::Status
            | Field::Type
            | Field::Location
            | Field::Contact
            | Field::Links
            | Field::Skills
            | Field::Summary
            | Field::Bio
            | Field::Ide
            | Field::Terminal
            | Field::Os
            | Field::Langs => None,
        }
    }

    fn field_mut(&mut self, field: Field) -> Option<&mut TextArea<'static>> {
        match field {
            Field::Title => Some(&mut self.title),
            Field::Url => Some(&mut self.url),
            Field::Tags => Some(&mut self.tags),
            Field::Description => Some(&mut self.description),
            Field::Headline
            | Field::Status
            | Field::Type
            | Field::Location
            | Field::Contact
            | Field::Links
            | Field::Skills
            | Field::Summary
            | Field::Bio
            | Field::Ide
            | Field::Terminal
            | Field::Os
            | Field::Langs => None,
        }
    }
}

impl Default for EditorState {
    fn default() -> Self {
        Self {
            open: false,
            scope: Scope::Own,
            viewer: Uuid::nil(),
            page: Page::Card,
            row: 0,
            editing: false,
            confirm_discard: false,
            error: None,
            card_editing: None,
            card_slug: None,
            status: WorkStatus::Open,
            work_type: WorkType::Any,
            headline: text_input(Field::Headline),
            location: text_input(Field::Location),
            contact: text_input(Field::Contact),
            links: text_input(Field::Links),
            skills: Vec::new(),
            summary: text_input(Field::Summary),
            card_baseline: CardValues::default(),
            bio: text_input(Field::Bio),
            ide: text_input(Field::Ide),
            terminal: text_input(Field::Terminal),
            os: text_input(Field::Os),
            langs: Vec::new(),
            about_baseline: AboutValues::default(),
            projects_view: ProjectsView::List { selected: 0 },
            row_rects: Cell::new([None; MAX_ROWS]),
        }
    }
}

impl EditorState {
    pub(crate) fn is_open(&self) -> bool {
        self.open
    }

    pub(crate) fn scope(&self) -> &Scope {
        &self.scope
    }

    pub(crate) fn page(&self) -> Page {
        self.page
    }

    pub(crate) fn row(&self) -> usize {
        self.row
    }

    pub(crate) fn editing(&self) -> bool {
        self.editing
    }

    pub(crate) fn confirm_discard(&self) -> bool {
        self.confirm_discard
    }

    pub(crate) fn error(&self) -> Option<(Field, &str)> {
        self.error
            .as_ref()
            .map(|(field, message)| (*field, message.as_str()))
    }

    pub(crate) fn status(&self) -> WorkStatus {
        self.status
    }

    pub(crate) fn work_type(&self) -> WorkType {
        self.work_type
    }

    pub(crate) fn projects_view(&self) -> &ProjectsView {
        &self.projects_view
    }

    /// The owner of whatever is being edited: the viewer, or the person a
    /// moderator is editing.
    pub(crate) fn owner(&self) -> Uuid {
        match &self.scope {
            Scope::Own => self.viewer,
            Scope::CardOf { owner, .. } | Scope::ProjectOf { owner, .. } => *owner,
        }
    }

    /// Open on the viewer's own profile. `card` is their card when they have
    /// one, `profile` their settings profile; `page` is where to land.
    pub(crate) fn open_own(
        &mut self,
        viewer: Uuid,
        card: Option<&WorkProfile>,
        profile: &Profile,
        page: Page,
    ) {
        self.reset();
        self.viewer = viewer;
        self.scope = Scope::Own;
        self.seed_card(card);
        self.seed_about(profile);
        self.page = page;
        self.open = true;
    }

    /// Open on the viewer's own projects page with a fresh project form.
    pub(crate) fn open_own_new_project(
        &mut self,
        viewer: Uuid,
        card: Option<&WorkProfile>,
        profile: &Profile,
    ) {
        self.open_own(viewer, card, profile, Page::Projects);
        self.start_new_project();
    }

    /// Open on the viewer's own projects page with `project`'s form.
    pub(crate) fn open_own_project(
        &mut self,
        viewer: Uuid,
        card: Option<&WorkProfile>,
        profile: &Profile,
        project: &ProjectRow,
    ) {
        self.open_own(viewer, card, profile, Page::Projects);
        self.projects_view = ProjectsView::Form(ProjectDraft::from_row(project, 0));
        self.sync_cursors();
    }

    /// A moderator opening someone else's card.
    pub(crate) fn open_card_of(
        &mut self,
        viewer: Uuid,
        owner: Uuid,
        username: String,
        card: &WorkProfile,
    ) {
        self.reset();
        self.viewer = viewer;
        self.scope = Scope::CardOf { owner, username };
        self.seed_card(Some(card));
        self.page = Page::Card;
        self.open = true;
    }

    /// A moderator opening someone else's project.
    pub(crate) fn open_project_of(
        &mut self,
        viewer: Uuid,
        owner: Uuid,
        username: String,
        project: &ProjectRow,
    ) {
        self.reset();
        self.viewer = viewer;
        self.scope = Scope::ProjectOf { owner, username };
        self.page = Page::Projects;
        self.projects_view = ProjectsView::Form(ProjectDraft::from_row(project, 0));
        self.open = true;
        self.sync_cursors();
    }

    pub(crate) fn close(&mut self) {
        self.reset();
    }

    fn reset(&mut self) {
        *self = Self::default();
    }

    fn seed_card(&mut self, card: Option<&WorkProfile>) {
        if let Some(card) = card {
            self.card_editing = Some(card.id);
            self.card_slug = Some(card.slug.clone());
            self.status = card.status;
            self.work_type = card.work_type;
            self.headline = seeded(Field::Headline, &card.headline);
            self.location = seeded(Field::Location, &card.location);
            self.contact = seeded(Field::Contact, &card.contact);
            self.links = seeded(Field::Links, &card.links.join(", "));
            // A card written before the picker may hold free text; only
            // what the vocabulary knows comes back.
            self.skills = vocab::normalize(&card.skills, SKILLS_LIMIT).tags;
            self.summary = seeded(Field::Summary, &card.summary);
        }
        self.card_baseline = self.card_values();
    }

    fn seed_about(&mut self, profile: &Profile) {
        self.bio = seeded(Field::Bio, &profile.bio);
        self.ide = seeded(Field::Ide, profile.ide.as_deref().unwrap_or(""));
        self.terminal = seeded(Field::Terminal, profile.terminal.as_deref().unwrap_or(""));
        self.os = seeded(Field::Os, profile.os.as_deref().unwrap_or(""));
        self.langs = profile.langs.clone();
        self.about_baseline = self.about_values();
    }

    pub(crate) fn card_values(&self) -> CardValues {
        CardValues {
            headline: joined(&self.headline, " "),
            status: self.status,
            work_type: self.work_type,
            location: joined(&self.location, " "),
            contact: joined(&self.contact, " "),
            links: joined(&self.links, ","),
            skills: self.skills.clone(),
            summary: joined(&self.summary, "\n"),
        }
    }

    pub(crate) fn about_values(&self) -> AboutValues {
        AboutValues {
            bio: joined(&self.bio, "\n"),
            ide: joined(&self.ide, " "),
            terminal: joined(&self.terminal, " "),
            os: joined(&self.os, " "),
            langs: self.langs.clone(),
        }
    }

    fn card_dirty(&self) -> bool {
        self.card_values() != self.card_baseline
    }

    fn about_dirty(&self) -> bool {
        self.about_values() != self.about_baseline
    }

    fn project_form_dirty(&self) -> bool {
        match &self.projects_view {
            ProjectsView::Form(draft) => draft.dirty(),
            ProjectsView::List { .. } => false,
        }
    }

    /// Anything typed and not yet saved, on any page.
    pub(crate) fn dirty(&self) -> bool {
        self.card_dirty() || self.about_dirty() || self.project_form_dirty()
    }

    /// A tag row's list: the card's skills or the profile's langs.
    pub(crate) fn tags(&self, field: Field) -> &[String] {
        match field {
            Field::Skills => &self.skills,
            Field::Langs => &self.langs,
            Field::Headline
            | Field::Status
            | Field::Type
            | Field::Location
            | Field::Contact
            | Field::Links
            | Field::Summary
            | Field::Bio
            | Field::Ide
            | Field::Terminal
            | Field::Os
            | Field::Title
            | Field::Url
            | Field::Tags
            | Field::Description => panic!("{} is not a tag row", field.label()),
        }
    }

    /// What the picker chose for a tag row.
    pub(crate) fn set_tags(&mut self, field: Field, tags: Vec<String>) {
        self.error = None;
        match field {
            Field::Skills => self.skills = tags,
            Field::Langs => self.langs = tags,
            Field::Headline
            | Field::Status
            | Field::Type
            | Field::Location
            | Field::Contact
            | Field::Links
            | Field::Summary
            | Field::Bio
            | Field::Ide
            | Field::Terminal
            | Field::Os
            | Field::Title
            | Field::Url
            | Field::Tags
            | Field::Description => panic!("{} is not a tag row", field.label()),
        }
    }

    // Rows

    /// The fields of the current page, or none when the projects page is
    /// showing its list (whose rows are projects, not fields).
    pub(crate) fn fields(&self) -> &'static [Field] {
        match (self.page, &self.projects_view) {
            (Page::Card, _) => &CARD_FIELDS,
            (Page::About, _) => &ABOUT_FIELDS,
            (Page::Projects, ProjectsView::Form(_)) => &PROJECT_FIELDS,
            (Page::Projects, ProjectsView::List { .. }) => &[],
        }
    }

    pub(crate) fn active_field(&self) -> Option<Field> {
        self.fields().get(self.row).copied()
    }

    pub(crate) fn field(&self, field: Field) -> &TextArea<'static> {
        match field {
            Field::Headline => &self.headline,
            Field::Location => &self.location,
            Field::Contact => &self.contact,
            Field::Links => &self.links,
            Field::Summary => &self.summary,
            Field::Bio => &self.bio,
            Field::Ide => &self.ide,
            Field::Terminal => &self.terminal,
            Field::Os => &self.os,
            Field::Title | Field::Url | Field::Tags | Field::Description => match &self
                .projects_view
            {
                ProjectsView::Form(draft) => draft.field(field).expect("project field"),
                ProjectsView::List { .. } => panic!("project field read without a project form"),
            },
            Field::Status | Field::Type => panic!("choice fields have no text"),
            Field::Skills | Field::Langs => panic!("tag fields have no text"),
        }
    }

    pub(crate) fn field_mut(&mut self, field: Field) -> &mut TextArea<'static> {
        match field {
            Field::Headline => &mut self.headline,
            Field::Location => &mut self.location,
            Field::Contact => &mut self.contact,
            Field::Links => &mut self.links,
            Field::Summary => &mut self.summary,
            Field::Bio => &mut self.bio,
            Field::Ide => &mut self.ide,
            Field::Terminal => &mut self.terminal,
            Field::Os => &mut self.os,
            Field::Title | Field::Url | Field::Tags | Field::Description => match &mut self
                .projects_view
            {
                ProjectsView::Form(draft) => draft.field_mut(field).expect("project field"),
                ProjectsView::List { .. } => panic!("project field written without a project form"),
            },
            Field::Status | Field::Type => panic!("choice fields have no text"),
            Field::Skills | Field::Langs => panic!("tag fields have no text"),
        }
    }

    /// The text of a field as the row shows it when not being edited.
    pub(crate) fn field_text(&self, field: Field) -> String {
        match field.kind() {
            FieldKind::Choice => match field {
                Field::Status => self.status.label().to_string(),
                Field::Type => self.work_type.label().to_string(),
                _ => unreachable!("only status and type are choices"),
            },
            FieldKind::Text => joined(self.field(field), " "),
            FieldKind::Multi => joined(self.field(field), "\n"),
            FieldKind::Tags => self.tags(field).join(" · "),
        }
    }

    pub(crate) fn move_row(&mut self, delta: isize) {
        let len = self.row_count();
        if len == 0 {
            self.row = 0;
            return;
        }
        let clamped = self.row.min(len - 1) as isize;
        self.row = (clamped + delta).clamp(0, len as isize - 1) as usize;
        self.sync_cursors();
    }

    pub(crate) fn set_row(&mut self, row: usize) {
        let len = self.row_count();
        self.row = if len == 0 { 0 } else { row.min(len - 1) };
        self.sync_cursors();
    }

    fn row_count(&self) -> usize {
        self.fields().len()
    }

    /// Start typing into the active row; a choice row cycles instead, and a
    /// tag row is the input layer's to open the picker on.
    pub(crate) fn start_editing(&mut self) {
        let Some(field) = self.active_field() else {
            return;
        };
        self.error = None;
        match field.kind() {
            FieldKind::Choice => self.cycle_choice(true),
            FieldKind::Tags => {}
            FieldKind::Text | FieldKind::Multi => {
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

    /// Commit the row being typed and move to the next one, the way Tab
    /// walks a form; from the last row it just stops editing.
    pub(crate) fn commit_and_advance(&mut self, forward: bool) {
        self.editing = false;
        let len = self.row_count();
        if len == 0 {
            return;
        }
        let last = len - 1;
        let next = if forward {
            if self.row >= last {
                None
            } else {
                Some(self.row + 1)
            }
        } else if self.row == 0 {
            None
        } else {
            Some(self.row - 1)
        };
        match next {
            Some(row) => {
                self.row = row;
                match self.active_field().map(Field::kind) {
                    Some(FieldKind::Text | FieldKind::Multi) => self.start_editing(),
                    // A choice row cycles, and a tag row opens its picker,
                    // on an explicit Enter, never from being landed on.
                    Some(FieldKind::Choice | FieldKind::Tags) | None => self.sync_cursors(),
                }
            }
            None => self.sync_cursors(),
        }
    }

    /// `←/→` on a status or type row.
    pub(crate) fn cycle_choice(&mut self, forward: bool) {
        let Some(field) = self.active_field() else {
            return;
        };
        match field {
            Field::Status => {
                self.status = if forward {
                    self.status.next()
                } else {
                    self.status.prev()
                };
            }
            Field::Type => {
                self.work_type = if forward {
                    self.work_type.next()
                } else {
                    self.work_type.prev()
                };
            }
            Field::Headline
            | Field::Location
            | Field::Contact
            | Field::Links
            | Field::Skills
            | Field::Summary
            | Field::Bio
            | Field::Ide
            | Field::Terminal
            | Field::Os
            | Field::Langs
            | Field::Title
            | Field::Url
            | Field::Tags
            | Field::Description => {}
        }
    }

    /// Only the row being typed shows a cursor.
    fn sync_cursors(&mut self) {
        let active = if self.editing {
            self.active_field()
        } else {
            None
        };
        for field in CARD_FIELDS.into_iter().chain(ABOUT_FIELDS) {
            if matches!(field.kind(), FieldKind::Text | FieldKind::Multi) {
                let visible = active == Some(field);
                set_themed_textarea_cursor_visible(self.field_mut(field), visible);
            }
        }
        if let ProjectsView::Form(draft) = &mut self.projects_view {
            for field in PROJECT_FIELDS {
                let visible = active == Some(field);
                set_themed_textarea_cursor_visible(
                    draft.field_mut(field).expect("project field"),
                    visible,
                );
            }
        }
    }

    /// Re-apply the theme to every text field after the palette changes.
    pub(crate) fn refresh_theme(&mut self) {
        let active = if self.editing {
            self.active_field()
        } else {
            None
        };
        for field in CARD_FIELDS.into_iter().chain(ABOUT_FIELDS) {
            if matches!(field.kind(), FieldKind::Text | FieldKind::Multi) {
                let visible = active == Some(field);
                crate::app::common::composer::apply_themed_textarea_style(
                    self.field_mut(field),
                    visible,
                );
            }
        }
        if let ProjectsView::Form(draft) = &mut self.projects_view {
            for field in PROJECT_FIELDS {
                let visible = active == Some(field);
                crate::app::common::composer::apply_themed_textarea_style(
                    draft.field_mut(field).expect("project field"),
                    visible,
                );
            }
        }
    }

    // Pages

    pub(crate) fn switch_page(&mut self, forward: bool) {
        let pages = self.scope.pages();
        if pages.len() < 2 {
            return;
        }
        let idx = pages
            .iter()
            .position(|page| *page == self.page)
            .unwrap_or(0);
        let next = if forward {
            (idx + 1) % pages.len()
        } else {
            (idx + pages.len() - 1) % pages.len()
        };
        self.page = pages[next];
        self.row = 0;
        self.editing = false;
        self.error = None;
        self.sync_cursors();
    }

    // Projects page

    pub(crate) fn project_selected(&self) -> usize {
        match &self.projects_view {
            ProjectsView::List { selected } => *selected,
            ProjectsView::Form(_) => 0,
        }
    }

    pub(crate) fn move_project_selection(&mut self, delta: isize, len: usize) {
        if let ProjectsView::List { selected } = &mut self.projects_view {
            if len == 0 {
                *selected = 0;
                return;
            }
            let clamped = (*selected).min(len - 1) as isize;
            *selected = (clamped + delta).clamp(0, len as isize - 1) as usize;
        }
    }

    pub(crate) fn set_project_selection(&mut self, index: usize, len: usize) {
        if let ProjectsView::List { selected } = &mut self.projects_view {
            *selected = if len == 0 { 0 } else { index.min(len - 1) };
        }
    }

    /// A new project lands at the top of the list, so the form returns there.
    pub(crate) fn start_new_project(&mut self) {
        self.projects_view = ProjectsView::Form(ProjectDraft::new(0));
        self.row = 0;
        self.error = None;
        self.start_editing();
    }

    pub(crate) fn start_editing_project(&mut self, project: &ProjectRow) {
        let list_index = self.project_selected();
        self.projects_view = ProjectsView::Form(ProjectDraft::from_row(project, list_index));
        self.row = 0;
        self.editing = false;
        self.error = None;
        self.sync_cursors();
    }

    /// Back from a project's form to the list row it was opened from.
    fn leave_project_form(&mut self) {
        let selected = match &self.projects_view {
            ProjectsView::Form(draft) => draft.list_index,
            ProjectsView::List { selected } => *selected,
        };
        self.projects_view = ProjectsView::List { selected };
        self.row = 0;
        self.editing = false;
        self.error = None;
    }

    // Esc and save

    /// Esc, in order: a typed row stops; a pending question is withdrawn; a
    /// project form goes back to its list; the modal asks before losing
    /// unsaved work, and closes when there is none.
    pub(crate) fn escape(&mut self) -> EscapeOutcome {
        if self.editing {
            self.stop_editing();
            return EscapeOutcome::Stayed;
        }
        if self.confirm_discard {
            self.confirm_discard = false;
            return EscapeOutcome::Stayed;
        }
        if self.scope == Scope::Own && matches!(self.projects_view, ProjectsView::Form(_)) {
            if self.project_form_dirty() {
                self.confirm_discard = true;
                return EscapeOutcome::AskedToDiscard;
            }
            self.leave_project_form();
            return EscapeOutcome::LeftProjectForm;
        }
        if self.dirty() {
            self.confirm_discard = true;
            return EscapeOutcome::AskedToDiscard;
        }
        self.close();
        EscapeOutcome::Closed
    }

    /// `y` on the discard question: drop the project draft for the list, or
    /// drop everything and close.
    pub(crate) fn confirm_discard_yes(&mut self) -> EscapeOutcome {
        self.confirm_discard = false;
        if self.scope == Scope::Own
            && matches!(self.projects_view, ProjectsView::Form(_))
            && self.project_form_dirty()
        {
            self.leave_project_form();
            return EscapeOutcome::LeftProjectForm;
        }
        self.close();
        EscapeOutcome::Closed
    }

    pub(crate) fn confirm_discard_no(&mut self) {
        self.confirm_discard = false;
    }

    /// Ctrl+S. On a project form: that project alone, and the form returns
    /// to the list (or the modal closes, for a moderator). Anywhere else:
    /// every dirty page, then the modal closes. A field that fails
    /// validation is named in the error line and becomes the active row,
    /// and nothing is saved.
    pub(crate) fn save(&mut self) -> Result<Vec<Save>, ()> {
        self.editing = false;
        self.error = None;
        let owner = self.owner();

        if let ProjectsView::Form(draft) = &self.projects_view {
            let values = draft.values();
            let editing = draft.editing;
            match validate_project(&values) {
                Ok(params) => {
                    let params = ShowcaseParams {
                        user_id: owner,
                        ..params
                    };
                    let save = Save::Project { params, editing };
                    match self.scope {
                        Scope::Own => self.leave_project_form(),
                        Scope::CardOf { .. } | Scope::ProjectOf { .. } => self.close(),
                    }
                    self.sync_cursors();
                    return Ok(vec![save]);
                }
                Err((field, message)) => {
                    self.fail_on(field, message);
                    return Err(());
                }
            }
        }

        let mut saves = Vec::new();
        if self.card_dirty() || (self.card_editing.is_none() && self.page == Page::Card) {
            let values = self.card_values();
            match validate_card(&values) {
                Ok(parsed) => {
                    let slug = self
                        .card_slug
                        .clone()
                        .unwrap_or_else(svc::generate_public_slug);
                    saves.push(Save::Card {
                        params: WorkProfileParams {
                            user_id: owner,
                            slug,
                            ..parsed
                        },
                        editing: self.card_editing,
                    });
                }
                Err((field, message)) => {
                    self.page = Page::Card;
                    self.fail_on(field, message);
                    return Err(());
                }
            }
        }
        if self.about_dirty() {
            saves.push(Save::About(self.about_values()));
        }
        self.close();
        Ok(saves)
    }

    fn fail_on(&mut self, field: Field, message: &'static str) {
        self.row = self
            .fields()
            .iter()
            .position(|candidate| *candidate == field)
            .unwrap_or(0);
        self.error = Some((field, message.to_string()));
        self.sync_cursors();
    }

    // Click map

    /// Record that `row` is drawn at `rect`. The map holds one entry per
    /// row on screen; a ninth is a draw bug and is dropped.
    pub(crate) fn record_row_rect(&self, row: usize, rect: Rect) {
        let mut rects = self.row_rects.get();
        if let Some(slot) = rects.iter_mut().find(|slot| slot.is_none()) {
            *slot = Some((row, rect));
        }
        self.row_rects.set(rects);
    }

    pub(crate) fn clear_row_rects(&self) {
        self.row_rects.set([None; MAX_ROWS]);
    }

    pub(crate) fn row_at(&self, x: u16, y: u16) -> Option<usize> {
        self.row_rects.get().iter().find_map(|slot| match slot {
            Some((row, r)) if x >= r.x && x < r.x + r.width && y >= r.y && y < r.y + r.height => {
                Some(*row)
            }
            Some(_) | None => None,
        })
    }
}

/// The card's rules. `user_id` and `slug` are filled by the caller.
pub(crate) fn validate_card(
    values: &CardValues,
) -> Result<WorkProfileParams, (Field, &'static str)> {
    if values.headline.is_empty() {
        return Err((Field::Headline, "headline required"));
    }
    if values.headline.chars().count() > HEADLINE_MAX {
        return Err((Field::Headline, "headline too long (max 120)"));
    }
    if values.location.is_empty() {
        return Err((Field::Location, "location required (remote counts)"));
    }
    if values.location.chars().count() > LOCATION_MAX {
        return Err((Field::Location, "location too long (max 120)"));
    }
    if values.contact.chars().count() > CONTACT_MAX {
        return Err((Field::Contact, "contact too long (max 200)"));
    }
    let links = svc::parse_links(&values.links);
    if links.is_empty() {
        return Err((Field::Links, "at least one http(s) link required"));
    }
    if values.summary.is_empty() {
        return Err((Field::Summary, "summary required"));
    }
    if values.summary.chars().count() > SUMMARY_MAX {
        return Err((Field::Summary, "summary too long (max 1000)"));
    }
    // The picker hands over canonical tags; folding once more is the
    // boundary, so both columns hold the vocabulary and nothing else.
    let skills = vocab::normalize(&values.skills, SKILLS_LIMIT).tags;
    let skills_tags = skills.clone();
    Ok(WorkProfileParams {
        user_id: Uuid::nil(),
        slug: String::new(),
        headline: values.headline.clone(),
        status: values.status,
        work_type: values.work_type,
        location: values.location.clone(),
        contact: values.contact.clone(),
        links,
        skills,
        skills_tags,
        summary: values.summary.clone(),
    })
}

/// A project's rules. `user_id` is filled by the caller.
pub(crate) fn validate_project(
    values: &ProjectValues,
) -> Result<ShowcaseParams, (Field, &'static str)> {
    if values.title.is_empty() {
        return Err((Field::Title, "title required"));
    }
    if values.title.chars().count() > TITLE_MAX {
        return Err((Field::Title, "title too long (max 120)"));
    }
    if values.url.is_empty() {
        return Err((Field::Url, "url required"));
    }
    if !svc::looks_like_url(&values.url) {
        return Err((Field::Url, "url must start with http:// or https://"));
    }
    if values.description.is_empty() {
        return Err((Field::Description, "description required"));
    }
    if values.description.chars().count() > DESCRIPTION_MAX {
        return Err((Field::Description, "description too long (max 800)"));
    }
    Ok(ShowcaseParams {
        user_id: Uuid::nil(),
        title: values.title.clone(),
        url: values.url.clone(),
        description: values.description.clone(),
        tags: crate::app::chat::showcase::svc::parse_tags(&values.tags),
    })
}

/// The about page's values applied onto the settings profile, which keeps
/// everything the page does not touch (notifications, theme, rails).
pub(crate) fn about_onto_profile(
    values: &AboutValues,
    profile: &Profile,
) -> late_core::models::profile::ProfileParams {
    let opt = |s: &str| {
        let s = s.trim();
        (!s.is_empty()).then(|| s.to_string())
    };
    let mut params = crate::app::profile::state::profile_params_from_profile(profile);
    params.bio = values.bio.clone();
    params.ide = opt(&values.ide);
    params.terminal = opt(&values.terminal);
    params.os = opt(&values.os);
    params.langs = values.langs.clone();
    params
}
