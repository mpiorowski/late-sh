use anyhow::Result;
use late_core::db::Db;
use late_core::models::pinstar_diagram::PinstarDiagram;
use tokio_postgres::Client;
use uuid::Uuid;

pub const INVITE_TOKEN_MAX_LEN: usize = 128;

#[derive(Debug, Clone)]
pub struct DiagramEntry {
    pub id: Uuid,
    pub title: String,
    pub is_owner: bool,
    pub role: String,
    pub updated: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BrowserTab {
    MyDiagrams,
    SharedWithMe,
}

impl BrowserTab {
    pub fn next(self) -> Self {
        match self {
            BrowserTab::MyDiagrams => BrowserTab::SharedWithMe,
            BrowserTab::SharedWithMe => BrowserTab::MyDiagrams,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BrowserMode {
    /// Showing the diagram list
    List,
    /// Accepting an invite token
    AcceptInvite,
    /// Confirming diagram deletion
    ConfirmDelete,
    /// Renaming a diagram
    RenameInput,
    /// Creating a new diagram with format picker
    CreateDiagram,
    /// Showing generated invite token
    GenerateInvite,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NewDiagramField {
    Name,
    Format,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DiagramFormat {
    Canvas,
}

impl DiagramFormat {
    pub fn label(self) -> &'static str {
        match self {
            DiagramFormat::Canvas => "Canvas",
        }
    }

    pub fn db_format(self) -> &'static str {
        match self {
            DiagramFormat::Canvas => "canvas",
        }
    }

    pub fn all() -> &'static [DiagramFormat] {
        &[DiagramFormat::Canvas]
    }

    pub fn from_index(i: usize) -> Self {
        Self::all()[i % Self::all().len()]
    }
}

#[derive(Debug, Clone)]
pub enum BrowserAction {
    Create { title: String },
    Open(Uuid, String), // id, role
    AcceptInvite(String),
    GenerateInvite(Uuid),
    Delete(Uuid),
    Rename(Uuid, String),
}

#[derive(Debug, Clone)]
pub enum BrowserActionResult {
    Open { id: Uuid, role: String },
    InviteCreated { token: String },
    Deleted { id: Uuid },
    Renamed,
}

pub struct DiagramBrowser {
    pub entries: Vec<DiagramEntry>,
    pub selected: usize,
    pub tab: BrowserTab,
    pub mode: BrowserMode,
    pub invite_token_input: String,
    pub delete_target_id: Option<Uuid>,
    pub rename_input: String,
    pub new_diagram_name: String,
    pub new_diagram_format: usize, // index into DiagramFormat::all()
    pub new_diagram_field: NewDiagramField,
    pub pending_action: Option<BrowserAction>,
    pub loading: bool,
    pub error: Option<String>,
    pub last_click: Option<(u16, u16, std::time::Instant)>,
    pub generated_invite_token: Option<String>,
}

impl Default for DiagramBrowser {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            selected: 0,
            tab: BrowserTab::MyDiagrams,
            mode: BrowserMode::List,
            invite_token_input: String::new(),
            delete_target_id: None,
            rename_input: String::new(),
            new_diagram_name: String::from("Untitled Diagram"),
            new_diagram_format: 0,
            new_diagram_field: NewDiagramField::Name,
            pending_action: None,
            loading: false,
            error: None,
            last_click: None,
            generated_invite_token: None,
        }
    }
}

impl DiagramBrowser {
    fn entry_visible_on_tab(&self, entry: &DiagramEntry) -> bool {
        match self.tab {
            BrowserTab::MyDiagrams => entry.is_owner,
            BrowserTab::SharedWithMe => !entry.is_owner,
        }
    }

    pub fn visible_entries(&self) -> Vec<&DiagramEntry> {
        self.entries
            .iter()
            .filter(|entry| self.entry_visible_on_tab(entry))
            .collect()
    }

    pub fn visible_len(&self) -> usize {
        self.entries
            .iter()
            .filter(|entry| self.entry_visible_on_tab(entry))
            .count()
    }

    pub fn selected_entry(&self) -> Option<&DiagramEntry> {
        self.visible_entries().into_iter().nth(self.selected)
    }

    pub fn clamp_selection(&mut self) {
        let len = self.visible_len();
        if len == 0 {
            self.selected = 0;
        } else {
            self.selected = self.selected.min(len - 1);
        }
    }

    pub fn move_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    pub fn move_down(&mut self) {
        let len = self.visible_len();
        if len > 0 {
            self.selected = self.selected.saturating_add(1);
        }
    }

    pub fn switch_tab(&mut self) {
        self.tab = self.tab.next();
        self.selected = 0;
    }

    pub fn push_invite_token_char(&mut self, ch: char) -> bool {
        if ch.is_control()
            || ch == '\u{7f}'
            || self.invite_token_input.chars().count() >= INVITE_TOKEN_MAX_LEN
        {
            return false;
        }
        self.invite_token_input.push(ch);
        true
    }
}

/// Load diagram list from DB. Called from a tokio task.
pub async fn load_diagram_list(db: &Db, user_id: Uuid) -> Result<Vec<DiagramEntry>> {
    let client = db.get().await?;
    load_diagram_list_with_client(&client, user_id).await
}

pub async fn load_diagram_list_with_client(
    client: &Client,
    user_id: Uuid,
) -> Result<Vec<DiagramEntry>> {
    let mut entries = Vec::new();

    // Owned diagrams
    let owned = PinstarDiagram::find_by_owner(client, user_id).await?;
    for d in owned {
        entries.push(DiagramEntry {
            id: d.id,
            title: d.title,
            is_owner: true,
            role: "owner".to_string(),
            updated: d.updated,
        });
    }

    // Shared with me
    let shared = PinstarDiagram::find_by_member_with_role(client, user_id).await?;
    for (d, role) in shared {
        if !entries.iter().any(|e| e.id == d.id) {
            entries.push(DiagramEntry {
                id: d.id,
                title: d.title,
                is_owner: false,
                role,
                updated: d.updated,
            });
        }
    }

    // Sort by updated descending
    entries.sort_by(|a, b| b.updated.cmp(&a.updated));
    Ok(entries)
}

/// Accept an invite token and return the diagram id plus the granted role.
pub async fn accept_invite(db: &Db, user_id: Uuid, token: String) -> Result<(Uuid, String)> {
    let token = token.trim().to_string();
    if token.is_empty()
        || token.chars().count() > INVITE_TOKEN_MAX_LEN
        || token.chars().any(|ch| ch.is_control() || ch == '\u{7f}')
    {
        anyhow::bail!("invalid invite token");
    }

    let client = db.get().await?;
    late_core::models::pinstar_invite::PinstarInvite::redeem(&client, user_id, &token).await
}

pub async fn create_invite_for_owner(
    db: &Db,
    owner_id: Uuid,
    diagram_id: Uuid,
    invite_role: String,
) -> Result<String> {
    let client = db.get().await?;
    let Some((_, owner_role)) =
        late_core::models::pinstar_diagram::PinstarDiagram::get_with_member_role(
            &client, diagram_id, owner_id,
        )
        .await?
    else {
        anyhow::bail!("diagram not found");
    };

    if owner_role != "owner" {
        anyhow::bail!("only the owner can create invite links");
    }

    let invite_role = match invite_role.as_str() {
        "editor" | "viewer" => invite_role,
        _ => anyhow::bail!("invalid invite role"),
    };

    for attempt in 0..5 {
        let token = late_core::models::pinstar_invite::PinstarInvite::generate_token();
        if late_core::models::pinstar_invite::PinstarInvite::find_by_token(&client, &token)
            .await?
            .is_some()
        {
            continue;
        }

        let params = late_core::models::pinstar_invite::PinstarInviteParams {
            diagram_id,
            token: token.clone(),
            role: invite_role.clone(),
            uses_left: Some(10),
            expires_at: Some(chrono::Utc::now() + chrono::Duration::hours(24)),
        };
        match late_core::models::pinstar_invite::PinstarInvite::create(&client, params).await {
            Ok(_) => return Ok(token),
            Err(err) if attempt < 4 && err.to_string().contains("duplicate") => continue,
            Err(err) => return Err(err),
        }
    }

    anyhow::bail!("failed to generate a unique invite token")
}

pub async fn delete_diagram_for_owner(db: &Db, owner_id: Uuid, diagram_id: Uuid) -> Result<()> {
    let client = db.get().await?;
    let deleted = PinstarDiagram::delete_by_owner(&client, diagram_id, owner_id).await?;
    if deleted == 0 {
        anyhow::bail!("only the owner can delete this diagram");
    }
    Ok(())
}

pub async fn rename_diagram_for_owner(
    db: &Db,
    owner_id: Uuid,
    diagram_id: Uuid,
    new_title: &str,
) -> Result<()> {
    let client = db.get().await?;
    if PinstarDiagram::update_title_by_owner(&client, diagram_id, owner_id, new_title)
        .await?
        .is_none()
    {
        anyhow::bail!("only the owner can rename this diagram");
    }
    Ok(())
}
