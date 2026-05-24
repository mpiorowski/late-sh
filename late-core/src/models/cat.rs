use anyhow::Result;
use chrono::{DateTime, Utc};
use tokio_postgres::Client;
use uuid::Uuid;

crate::user_scoped_model! {
    table = "cat_companions";
    user_field = user_id;
    params = CatCompanionParams;
    struct CatCompanion {
        @data
        pub user_id: Uuid,
        pub last_fed: Option<DateTime<Utc>>,
        pub last_watered: Option<DateTime<Utc>>,
        pub last_played: Option<DateTime<Utc>>,
        pub last_groomed: Option<DateTime<Utc>>,
        pub last_treated: Option<DateTime<Utc>>,
        pub name: Option<String>,
    }
}

/// Maximum length of a user-set pet name.
pub const CAT_NAME_MAX_CHARS: usize = 24;

/// Normalise a candidate pet name. Trims surrounding whitespace, collapses
/// inner whitespace runs to a single space, caps to `CAT_NAME_MAX_CHARS`
/// characters. Returns `None` when the result would be empty.
pub fn normalize_cat_name(input: &str) -> Option<String> {
    let collapsed: String = input.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.is_empty() {
        return None;
    }
    Some(collapsed.chars().take(CAT_NAME_MAX_CHARS).collect())
}

impl CatCompanion {
    pub async fn ensure(client: &Client, user_id: Uuid) -> Result<Self> {
        let row = client
            .query_one(
                "INSERT INTO cat_companions (user_id) VALUES ($1)
                 ON CONFLICT (user_id) DO UPDATE SET updated = cat_companions.updated
                 RETURNING *",
                &[&user_id],
            )
            .await?;
        Ok(Self::from(row))
    }

    pub async fn touch_fed(client: &Client, user_id: Uuid) -> Result<()> {
        client
            .execute(
                "UPDATE cat_companions SET last_fed = current_timestamp, updated = current_timestamp WHERE user_id = $1",
                &[&user_id],
            )
            .await?;
        Ok(())
    }

    pub async fn touch_watered(client: &Client, user_id: Uuid) -> Result<()> {
        client
            .execute(
                "UPDATE cat_companions SET last_watered = current_timestamp, updated = current_timestamp WHERE user_id = $1",
                &[&user_id],
            )
            .await?;
        Ok(())
    }

    pub async fn touch_played(client: &Client, user_id: Uuid) -> Result<()> {
        client
            .execute(
                "UPDATE cat_companions SET last_played = current_timestamp, updated = current_timestamp WHERE user_id = $1",
                &[&user_id],
            )
            .await?;
        Ok(())
    }

    pub async fn touch_groomed(client: &Client, user_id: Uuid) -> Result<()> {
        client
            .execute(
                "UPDATE cat_companions SET last_groomed = current_timestamp, updated = current_timestamp WHERE user_id = $1",
                &[&user_id],
            )
            .await?;
        Ok(())
    }

    pub async fn touch_treated(client: &Client, user_id: Uuid) -> Result<()> {
        client
            .execute(
                "UPDATE cat_companions SET last_treated = current_timestamp, updated = current_timestamp WHERE user_id = $1",
                &[&user_id],
            )
            .await?;
        Ok(())
    }

    pub async fn set_name(client: &Client, user_id: Uuid, name: Option<&str>) -> Result<()> {
        client
            .execute(
                "UPDATE cat_companions SET name = $1, updated = current_timestamp WHERE user_id = $2",
                &[&name, &user_id],
            )
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_trims_and_collapses_whitespace() {
        assert_eq!(
            normalize_cat_name("  Whiskers  ").as_deref(),
            Some("Whiskers")
        );
        assert_eq!(
            normalize_cat_name("Mr   Mittens").as_deref(),
            Some("Mr Mittens")
        );
    }

    #[test]
    fn normalize_caps_length_to_max() {
        let very_long = "a".repeat(200);
        let out = normalize_cat_name(&very_long).expect("non-empty");
        assert_eq!(out.chars().count(), CAT_NAME_MAX_CHARS);
    }

    #[test]
    fn normalize_returns_none_for_empty_or_whitespace_only() {
        assert!(normalize_cat_name("").is_none());
        assert!(normalize_cat_name("   ").is_none());
    }
}
