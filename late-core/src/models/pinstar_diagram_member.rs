use anyhow::Result;
use tokio_postgres::Client;
use uuid::Uuid;

crate::model! {
    table = "pinstar_diagram_members";
    params = PinstarDiagramMemberParams;
    struct PinstarDiagramMember {
        @data
        pub diagram_id: Uuid,
        pub user_id: Uuid,
        pub role: String,
    }
}

impl PinstarDiagramMember {
    pub async fn find_by_diagram(client: &Client, diagram_id: Uuid) -> Result<Vec<Self>> {
        let rows = client
            .query(
                "SELECT * FROM pinstar_diagram_members WHERE diagram_id = $1 ORDER BY created",
                &[&diagram_id],
            )
            .await?;
        Ok(rows.into_iter().map(Self::from).collect())
    }

    pub async fn find_by_user(client: &Client, user_id: Uuid) -> Result<Vec<Self>> {
        let rows = client
            .query(
                "SELECT * FROM pinstar_diagram_members WHERE user_id = $1 ORDER BY created",
                &[&user_id],
            )
            .await?;
        Ok(rows.into_iter().map(Self::from).collect())
    }

    pub async fn upsert_member(
        client: &Client,
        diagram_id: Uuid,
        user_id: Uuid,
        role: &str,
    ) -> Result<Self> {
        if !matches!(role, "editor" | "viewer") {
            anyhow::bail!("invalid pinstar member role");
        }
        let row = client
            .query_one(
                "INSERT INTO pinstar_diagram_members (diagram_id, user_id, role)
                 VALUES ($1, $2, $3)
                 ON CONFLICT (diagram_id, user_id) DO UPDATE
                 SET role = EXCLUDED.role, updated = CURRENT_TIMESTAMP
                 RETURNING *",
                &[&diagram_id, &user_id, &role],
            )
            .await?;
        Ok(Self::from(row))
    }

    pub async fn delete_member(client: &Client, diagram_id: Uuid, user_id: Uuid) -> Result<u64> {
        let count = client
            .execute(
                "DELETE FROM pinstar_diagram_members WHERE diagram_id = $1 AND user_id = $2",
                &[&diagram_id, &user_id],
            )
            .await?;
        Ok(count)
    }

    pub async fn delete_by_diagram(client: &Client, diagram_id: Uuid) -> Result<u64> {
        let count = client
            .execute(
                "DELETE FROM pinstar_diagram_members WHERE diagram_id = $1",
                &[&diagram_id],
            )
            .await?;
        Ok(count)
    }
}
