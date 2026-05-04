use anyhow::Result;
use tokio_postgres::Client;
use uuid::Uuid;

crate::user_scoped_model! {
    table = "work_profiles";
    user_field = user_id;
    params = WorkProfileParams;
    struct WorkProfile {
        @data
        pub user_id: Uuid,
        pub slug: String,
        pub headline: String,
        pub status: String,
        pub work_type: String,
        pub location: String,
        pub contact: String,
        pub links: Vec<String>,
        pub skills: Vec<String>,
        pub summary: String,
    }
}

impl WorkProfile {
    pub async fn list_recent(client: &Client, limit: i64) -> Result<Vec<Self>> {
        let rows = client
            .query(
                "SELECT * FROM work_profiles ORDER BY updated DESC, created DESC, id DESC LIMIT $1",
                &[&limit],
            )
            .await?;
        Ok(rows.into_iter().map(Self::from).collect())
    }

    pub async fn find_by_slug(client: &Client, slug: &str) -> Result<Option<Self>> {
        let row = client
            .query_opt("SELECT * FROM work_profiles WHERE slug = $1", &[&slug])
            .await?;
        Ok(row.map(Self::from))
    }
}
