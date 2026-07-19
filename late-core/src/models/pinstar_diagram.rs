use anyhow::Result;
use chrono::{DateTime, Utc};
use deadpool_postgres::GenericClient;
use serde_json::Value;
use tokio_postgres::Client;
use uuid::Uuid;

crate::user_scoped_model! {
    table = "pinstar_diagrams";
    user_field = owner_id;
    params = PinstarDiagramParams;
    struct PinstarDiagram {
        @data
        pub owner_id: Uuid,
        pub title: String,
        pub diagram_data: Value,
        pub format: String,
    }
}

impl PinstarDiagram {
    pub async fn find_by_owner(client: &Client, owner_id: Uuid) -> Result<Vec<Self>> {
        let rows = client
            .query(
                "SELECT * FROM pinstar_diagrams WHERE owner_id = $1 ORDER BY updated DESC",
                &[&owner_id],
            )
            .await?;
        Ok(rows.into_iter().map(Self::from).collect())
    }

    pub async fn find_by_member(client: &Client, user_id: Uuid) -> Result<Vec<Self>> {
        let rows = client
            .query(
                "SELECT d.* FROM pinstar_diagrams d
                 INNER JOIN pinstar_diagram_members m ON m.diagram_id = d.id
                 WHERE m.user_id = $1
                 ORDER BY d.updated DESC",
                &[&user_id],
            )
            .await?;
        Ok(rows.into_iter().map(Self::from).collect())
    }

    pub async fn find_by_member_with_role(
        client: &Client,
        user_id: Uuid,
    ) -> Result<Vec<(Self, String)>> {
        let rows = client
            .query(
                "SELECT d.*, m.role AS member_role FROM pinstar_diagrams d
                 INNER JOIN pinstar_diagram_members m ON m.diagram_id = d.id
                 WHERE m.user_id = $1
                 ORDER BY d.updated DESC",
                &[&user_id],
            )
            .await?;
        Ok(rows
            .into_iter()
            .map(|row| {
                let role: String = row.get("member_role");
                (
                    Self::from(row),
                    valid_member_role(&role).unwrap_or("viewer").to_string(),
                )
            })
            .collect())
    }

    /// Returns the diagram and the requesting user's effective role.
    /// Owners keep owner access, explicit members keep their member role, and
    /// everyone else can open public diagrams as read-only viewers.
    pub async fn get_with_member_role(
        client: &Client,
        diagram_id: Uuid,
        user_id: Uuid,
    ) -> Result<Option<(Self, String)>> {
        let Some(diagram) = Self::get(client, diagram_id).await? else {
            return Ok(None);
        };

        // Owner check first
        if diagram.owner_id == user_id {
            return Ok(Some((diagram, "owner".to_string())));
        }
        // Then member check
        let row = client
            .query_opt(
                "SELECT d.*, m.role FROM pinstar_diagrams d
                 INNER JOIN pinstar_diagram_members m ON m.diagram_id = d.id
                 WHERE d.id = $1 AND m.user_id = $2",
                &[&diagram_id, &user_id],
            )
            .await?;
        Ok(row
            .map(|r| {
                let role: String = r.get("role");
                (
                    Self::from(r),
                    valid_member_role(&role).unwrap_or("viewer").to_string(),
                )
            })
            .or_else(|| Some((diagram, "viewer".to_string()))))
    }

    pub async fn update_data(client: &Client, id: Uuid, diagram_data: Value) -> Result<Self> {
        let row = client
            .query_one(
                "UPDATE pinstar_diagrams SET diagram_data = $1, updated = CURRENT_TIMESTAMP
                 WHERE id = $2 RETURNING *",
                &[&diagram_data, &id],
            )
            .await?;
        Ok(Self::from(row))
    }

    pub async fn update_data_if_updated(
        client: &Client,
        id: Uuid,
        diagram_data: Value,
        expected_updated: DateTime<Utc>,
    ) -> Result<Option<Self>> {
        let row = client
            .query_opt(
                "UPDATE pinstar_diagrams SET diagram_data = $1, updated = CURRENT_TIMESTAMP
                 WHERE id = $2 AND updated = $3 RETURNING *",
                &[&diagram_data, &id, &expected_updated],
            )
            .await?;
        Ok(row.map(Self::from))
    }

    pub async fn update_title(client: &Client, id: Uuid, title: &str) -> Result<Self> {
        let row = client
            .query_one(
                "UPDATE pinstar_diagrams SET title = $1, updated = CURRENT_TIMESTAMP
                 WHERE id = $2 RETURNING *",
                &[&title, &id],
            )
            .await?;
        Ok(Self::from(row))
    }

    pub async fn update_title_by_owner(
        client: &Client,
        id: Uuid,
        owner_id: Uuid,
        title: &str,
    ) -> Result<Option<Self>> {
        let row = client
            .query_opt(
                "UPDATE pinstar_diagrams SET title = $1, updated = CURRENT_TIMESTAMP
                 WHERE id = $2 AND owner_id = $3 RETURNING *",
                &[&title, &id, &owner_id],
            )
            .await?;
        Ok(row.map(Self::from))
    }

    pub async fn delete_by_owner(client: &Client, id: Uuid, owner_id: Uuid) -> Result<u64> {
        let count = client
            .execute(
                "DELETE FROM pinstar_diagrams WHERE id = $1 AND owner_id = $2",
                &[&id, &owner_id],
            )
            .await?;
        Ok(count)
    }

    /// Delete a diagram by id, unconditionally. The caller is responsible for
    /// the permission check; takes a generic client so it can run inside the
    /// transaction that also records the moderation audit entry.
    pub async fn delete_by_id(client: &impl GenericClient, id: Uuid) -> Result<u64> {
        let count = client
            .execute("DELETE FROM pinstar_diagrams WHERE id = $1", &[&id])
            .await?;
        Ok(count)
    }

    /// Every diagram visible to `user_id`, with the user's effective role and
    /// the full member roster, ordered most-recently-updated first. Public
    /// diagrams are visible to everyone as read-only viewers, so this lists all
    /// diagrams, not just owned or joined ones. Drives the diagram browser.
    pub async fn list_for_viewer(
        client: &Client,
        user_id: Uuid,
    ) -> Result<Vec<DiagramListEntry>> {
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
            .map(|row| DiagramListEntry {
                id: row.get("id"),
                title: row.get("title"),
                owner_id: row.get("owner_id"),
                owner_name: row.get("owner_name"),
                effective_role: row.get("effective_role"),
                is_member: row.get("is_member"),
                member_names: row.get("member_names"),
                created: row.get("created"),
                updated: row.get("updated"),
            })
            .collect())
    }
}

/// A row of the diagram browser: a diagram plus the viewer's effective role
/// and the resolved member roster. `owner_id` lets the caller derive
/// ownership against the viewing user.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiagramListEntry {
    pub id: Uuid,
    pub title: String,
    pub owner_id: Uuid,
    pub owner_name: String,
    pub effective_role: String,
    pub is_member: bool,
    pub member_names: String,
    pub created: DateTime<Utc>,
    pub updated: DateTime<Utc>,
}

fn valid_member_role(role: &str) -> Option<&'static str> {
    match role {
        "editor" => Some("editor"),
        "viewer" => Some("viewer"),
        _ => None,
    }
}
