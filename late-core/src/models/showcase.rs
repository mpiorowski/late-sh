use anyhow::Result;
use tokio_postgres::Client;
use uuid::Uuid;

crate::user_scoped_model! {
    table = "showcases";
    user_field = user_id;
    params = ShowcaseParams;
    struct Showcase {
        @data
        pub user_id: Uuid,
        pub title: String,
        pub url: String,
        pub description: String,
        pub tags: Vec<String>,
    }
}

impl Showcase {
    pub async fn list_recent(client: &Client, limit: i64) -> Result<Vec<Self>> {
        let rows = client
            .query(
                "SELECT * FROM showcases ORDER BY created DESC, id DESC LIMIT $1",
                &[&limit],
            )
            .await?;
        Ok(rows.into_iter().map(Self::from).collect())
    }
}
