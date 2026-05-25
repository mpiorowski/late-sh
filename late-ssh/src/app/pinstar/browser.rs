use anyhow::Result;
use chrono::{DateTime, Utc};
use late_core::db::Db;
use late_core::models::{
    moderation_audit_log::ModerationAuditLog, pinstar_diagram::PinstarDiagram, user::User,
};
use serde_json::json;
use tokio_postgres::Client;
use uuid::Uuid;

use crate::app::pinstar::data::CanvasData;
use crate::moderation::policy::{Permissions, Tier};

pub const INVITE_TOKEN_MAX_LEN: usize = 128;

#[derive(Debug, Clone)]
pub struct DiagramEntry {
    pub id: Uuid,
    pub title: String,
    pub is_owner: bool,
    pub is_member: bool,
    pub role: String,
    pub owner: String,
    pub members: String,
    pub created: DateTime<Utc>,
    pub updated: DateTime<Utc>,
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
    /// Showing Pinstar keyboard help over the browser
    Help,
    /// Importing a canvas from pasted JSON
    ImportCanvas,
}


#[derive(Debug, Clone)]
pub enum BrowserAction {
    Create { title: String },
    Import { title: String, data: CanvasData },
    Open(Uuid, String), // id, role
    AcceptInvite(String),
    GenerateInvite(Uuid),
    CopySource(Uuid),
    Delete(Uuid),
    Rename(Uuid, String),
}

#[derive(Debug, Clone)]
pub enum BrowserActionResult {
    Open { id: Uuid, role: String },
    InviteCreated { token: String },
    CopiedSource { source: String },
    Deleted { id: Uuid },
    Renamed,
}

pub struct DiagramBrowser {
    pub entries: Vec<DiagramEntry>,
    pub selected: usize,
    pub mode: BrowserMode,
    pub invite_token_input: String,
    pub delete_target_id: Option<Uuid>,
    pub rename_input: String,
    pub new_diagram_name: String,
    pub pending_action: Option<BrowserAction>,
    pub loading: bool,
    pub error: Option<String>,
    pub last_click: Option<(u16, u16, std::time::Instant)>,
    pub generated_invite_token: Option<String>,
    pub import_input: String,
    pub import_name: String,
}

impl Default for DiagramBrowser {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            selected: 0,
            mode: BrowserMode::List,
            invite_token_input: String::new(),
            delete_target_id: None,
            rename_input: String::new(),
            new_diagram_name: String::new(),
            pending_action: None,
            loading: false,
            error: None,
            last_click: None,
            generated_invite_token: None,
            import_input: String::new(),
            import_name: String::from("Imported Diagram"),
        }
    }
}

impl DiagramBrowser {
    pub fn visible_entries(&self) -> Vec<&DiagramEntry> {
        self.entries.iter().collect()
    }

    pub fn visible_len(&self) -> usize {
        self.entries.len()
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
            self.selected = self.selected.saturating_add(1).min(len - 1);
        }
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
    let rows = client
        .query(
            "SELECT d.id,
                    d.title,
                    d.owner_id,
                    d.created,
                    d.updated,
                    COALESCE(NULLIF(owner.username, ''), substring(d.owner_id::text, 1, 8)) AS owner_name,
                    CASE
                        WHEN d.owner_id = $1 THEN 'owner'
                        WHEN self_member.role IN ('editor', 'viewer') THEN self_member.role
                        ELSE 'viewer'
                    END AS effective_role,
                    (d.owner_id = $1 OR self_member.user_id IS NOT NULL) AS is_member,
                    COALESCE(
                        string_agg(
                            COALESCE(NULLIF(member_user.username, ''), substring(member_user.id::text, 1, 8))
                                || ':' || m.role,
                            ', '
                            ORDER BY member_user.username, member_user.id
                        ) FILTER (WHERE m.user_id IS NOT NULL),
                        ''
                    ) AS member_names
               FROM pinstar_diagrams d
               JOIN users owner ON owner.id = d.owner_id
               LEFT JOIN pinstar_diagram_members self_member
                      ON self_member.diagram_id = d.id
                     AND self_member.user_id = $1
               LEFT JOIN pinstar_diagram_members m ON m.diagram_id = d.id
               LEFT JOIN users member_user ON member_user.id = m.user_id
              GROUP BY d.id,
                       d.title,
                       d.owner_id,
                       d.created,
                       d.updated,
                       owner.username,
                       self_member.user_id,
                       self_member.role
              ORDER BY d.updated DESC",
            &[&user_id],
        )
        .await?;

    Ok(rows
        .into_iter()
        .map(|row| DiagramEntry {
            id: row.get("id"),
            title: row.get("title"),
            is_owner: row.get::<_, Uuid>("owner_id") == user_id,
            is_member: row.get("is_member"),
            role: row.get("effective_role"),
            owner: row.get("owner_name"),
            members: row.get("member_names"),
            created: row.get("created"),
            updated: row.get("updated"),
        })
        .collect())
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

pub async fn copy_diagram_source_for_member(
    db: &Db,
    user_id: Uuid,
    diagram_id: Uuid,
) -> Result<String> {
    let client = db.get().await?;
    let Some((diagram, _role)) =
        PinstarDiagram::get_with_member_role(&client, diagram_id, user_id).await?
    else {
        anyhow::bail!("diagram not found");
    };
    Ok(serde_json::to_string_pretty(&diagram.diagram_data)?)
}

pub async fn delete_diagram_for_user(db: &Db, user_id: Uuid, diagram_id: Uuid) -> Result<()> {
    let mut client = db.get().await?;
    let Some(diagram) = PinstarDiagram::get(&client, diagram_id).await? else {
        anyhow::bail!("diagram not found");
    };
    let actor = User::get(&client, user_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("user not found"))?;
    let owner = User::get(&client, diagram.owner_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("diagram owner not found"))?;
    let permissions = Permissions::new(actor.is_admin, actor.is_moderator);
    let is_owner = diagram.owner_id == user_id;
    let owner_tier = Tier::from_user_flags(owner.is_admin, owner.is_moderator);

    if !permissions.can_delete_pinstar_graph(is_owner, owner_tier) {
        anyhow::bail!("not allowed to delete this diagram");
    }

    let tx = client.transaction().await?;
    let deleted = tx
        .execute("DELETE FROM pinstar_diagrams WHERE id = $1", &[&diagram_id])
        .await?;
    if deleted == 0 {
        anyhow::bail!("diagram already deleted");
    }
    ModerationAuditLog::record_if(
        &tx,
        permissions.should_audit(is_owner),
        user_id,
        "pinstar_graph_delete",
        "pinstar_graph",
        Some(diagram_id),
        json!({ "owner_id": diagram.owner_id, "title": diagram.title }),
    )
    .await?;
    tx.commit().await?;
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
