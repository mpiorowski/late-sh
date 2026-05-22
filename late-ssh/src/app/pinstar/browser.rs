use anyhow::Result;
use late_core::db::Db;
use late_core::models::pinstar_diagram::PinstarDiagram;
use tokio_postgres::Client;
use uuid::Uuid;

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
    pub fn selected_entry(&self) -> Option<&DiagramEntry> {
        self.entries.get(self.selected)
    }

    pub fn move_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    pub fn move_down(&mut self) {
        if !self.entries.is_empty() && self.selected < self.entries.len() - 1 {
            self.selected += 1;
        }
    }

    pub fn switch_tab(&mut self) {
        self.tab = self.tab.next();
        self.selected = 0;
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
    let shared = PinstarDiagram::find_by_member(client, user_id).await?;
    for d in shared {
        if !entries.iter().any(|e| e.id == d.id) {
            entries.push(DiagramEntry {
                id: d.id,
                title: d.title,
                is_owner: false,
                role: "editor".to_string(), // TODO: look up actual role
                updated: d.updated,
            });
        }
    }

    // Sort by updated descending
    entries.sort_by(|a, b| b.updated.cmp(&a.updated));
    Ok(entries)
}

/// Accept an invite token and return the diagram_id.
pub async fn accept_invite(db: &Db, user_id: Uuid, token: String) -> Result<Uuid> {
    let client = db.get().await?;
    let invite = late_core::models::pinstar_invite::PinstarInvite::find_by_token(&client, &token)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Invite not found"))?;

    if !invite.is_valid() {
        anyhow::bail!("Invite has expired or has no uses left");
    }

    // Add user as member
    late_core::models::pinstar_diagram_member::PinstarDiagramMember::upsert_member(
        &client,
        invite.diagram_id,
        user_id,
        &invite.role,
    )
    .await?;

    // Decrement uses
    late_core::models::pinstar_invite::PinstarInvite::decrement_uses(&client, invite.id).await?;

    Ok(invite.diagram_id)
}
