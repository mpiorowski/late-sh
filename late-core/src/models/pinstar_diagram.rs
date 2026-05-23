use anyhow::Result;
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
                (Self::from(row), role)
            })
            .collect())
    }

    /// Returns the diagram and the requesting user's role if they have access.
    pub async fn get_with_member_role(
        client: &Client,
        diagram_id: Uuid,
        user_id: Uuid,
    ) -> Result<Option<(Self, String)>> {
        // Owner check first
        if let Some(diagram) = Self::get(client, diagram_id).await? {
            if diagram.owner_id == user_id {
                return Ok(Some((diagram, "owner".to_string())));
            }
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
        Ok(row.map(|r| {
            let role: String = r.get("role");
            (Self::from(r), role)
        }))
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
}
