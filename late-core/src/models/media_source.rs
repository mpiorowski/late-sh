use anyhow::Result;
use tokio_postgres::Client;
use uuid::Uuid;

crate::model! {
    table = "media_sources";
    params = MediaSourceParams;
    struct MediaSource {
        @data
        pub source_kind: String,
        pub media_kind: String,
        pub external_id: String,
        pub title: Option<String>,
        pub channel: Option<String>,
        pub is_stream: bool,
        pub updated_by: Option<Uuid>,
    }
}

impl MediaSource {
    pub const KIND_YOUTUBE_FALLBACK: &'static str = "youtube_fallback";
    pub const MEDIA_KIND_YOUTUBE: &'static str = "youtube";

    pub async fn youtube_fallback(client: &Client) -> Result<Option<Self>> {
        let row = client
            .query_opt(
                "SELECT * FROM media_sources
                 WHERE source_kind = 'youtube_fallback'
                 LIMIT 1",
                &[],
            )
            .await?;
        Ok(row.map(Self::from))
    }

    pub async fn upsert_youtube_fallback(
        client: &Client,
        external_id: &str,
        title: Option<&str>,
        channel: Option<&str>,
        updated_by: Uuid,
    ) -> Result<Self> {
        let row = client
            .query_one(
                "INSERT INTO media_sources
                    (source_kind, media_kind, external_id, title, channel,
                     is_stream, updated_by)
                 VALUES ('youtube_fallback', 'youtube', $1, $2, $3, true, $4)
                 ON CONFLICT (source_kind)
                 DO UPDATE SET
                     external_id = EXCLUDED.external_id,
                     title = EXCLUDED.title,
                     channel = EXCLUDED.channel,
                     is_stream = true,
                     updated_by = EXCLUDED.updated_by,
                     updated = current_timestamp
                 RETURNING *",
                &[&external_id, &title, &channel, &updated_by],
            )
            .await?;
        Ok(Self::from(row))
    }
}
