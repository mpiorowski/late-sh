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
        pub adopted_at: Option<DateTime<Utc>>,
        pub name: Option<String>,
    }
}

/// Maximum length of a user-set pet name.
pub const CAT_NAME_MAX_CHARS: usize = 24;

/// Life stage of the cat, derived from how many days it has existed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifeStage {
    Kitten,
    YoungCat,
    Adult,
    WiseOldCat,
}

impl LifeStage {
    /// Display label for use in the modal title and elsewhere.
    pub fn label(self) -> &'static str {
        match self {
            LifeStage::Kitten => "Kitten",
            LifeStage::YoungCat => "Young Cat",
            LifeStage::Adult => "Adult",
            LifeStage::WiseOldCat => "Wise Old Cat",
        }
    }

    /// Stage bucket for a given age in days. Negative inputs are treated as 0.
    pub fn from_age_days(days: i64) -> Self {
        match days.max(0) {
            0..=6 => LifeStage::Kitten,
            7..=29 => LifeStage::YoungCat,
            30..=179 => LifeStage::Adult,
            _ => LifeStage::WiseOldCat,
        }
    }
}

/// Cat age in whole days. Clamped at 0 so freshly-created or future-dated
/// rows count as "today" rather than panicking the renderer with negatives.
pub fn cat_age_days(created: DateTime<Utc>, now: DateTime<Utc>) -> i64 {
    (now - created).num_days().max(0)
}

/// Timestamp used for cat age. Purchased cats age from adoption; pre-adoption
/// fallback states still use row creation so the UI can render sensibly.
pub fn cat_age_anchor(created: DateTime<Utc>, adopted_at: Option<DateTime<Utc>>) -> DateTime<Utc> {
    adopted_at.unwrap_or(created)
}

/// Human-readable age label that pairs naturally with a life-stage label,
/// e.g. "today", "3 days", "2 weeks", "5 months", "1 year".
pub fn cat_age_label(created: DateTime<Utc>, now: DateTime<Utc>) -> String {
    let days = cat_age_days(created, now);
    if days < 1 {
        return "today".to_string();
    }
    if days < 14 {
        return if days == 1 {
            "1 day".to_string()
        } else {
            format!("{days} days")
        };
    }
    if days < 30 {
        let weeks = days / 7;
        return if weeks == 1 {
            "1 week".to_string()
        } else {
            format!("{weeks} weeks")
        };
    }
    if days < 365 {
        let months = days / 30;
        return if months == 1 {
            "1 month".to_string()
        } else {
            format!("{months} months")
        };
    }
    let years = days / 365;
    if years == 1 {
        "1 year".to_string()
    } else {
        format!("{years} years")
    }
}

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

    /// Life stage for this cat at the given `now`, derived from adoption when
    /// available and row creation otherwise.
    pub fn life_stage(&self, now: DateTime<Utc>) -> LifeStage {
        LifeStage::from_age_days(cat_age_days(
            cat_age_anchor(self.created, self.adopted_at),
            now,
        ))
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

    #[test]
    fn life_stage_buckets() {
        assert_eq!(LifeStage::from_age_days(0), LifeStage::Kitten);
        assert_eq!(LifeStage::from_age_days(6), LifeStage::Kitten);
        assert_eq!(LifeStage::from_age_days(7), LifeStage::YoungCat);
        assert_eq!(LifeStage::from_age_days(29), LifeStage::YoungCat);
        assert_eq!(LifeStage::from_age_days(30), LifeStage::Adult);
        assert_eq!(LifeStage::from_age_days(179), LifeStage::Adult);
        assert_eq!(LifeStage::from_age_days(180), LifeStage::WiseOldCat);
        assert_eq!(LifeStage::from_age_days(10_000), LifeStage::WiseOldCat);
    }

    #[test]
    fn life_stage_clamps_negative_days() {
        // Clock skew between server and renderer must not flip the cat into the
        // oldest bucket via signed-int wraparound.
        assert_eq!(LifeStage::from_age_days(-3), LifeStage::Kitten);
    }

    #[test]
    fn cat_age_days_is_zero_for_future_created() {
        use chrono::TimeZone;
        let now = Utc.with_ymd_and_hms(2026, 5, 25, 12, 0, 0).unwrap();
        let future = Utc.with_ymd_and_hms(2026, 5, 26, 12, 0, 0).unwrap();
        assert_eq!(cat_age_days(future, now), 0);
    }

    #[test]
    fn cat_age_anchor_prefers_adoption_timestamp() {
        use chrono::TimeZone;
        let created = Utc.with_ymd_and_hms(2026, 5, 1, 12, 0, 0).unwrap();
        let adopted = Utc.with_ymd_and_hms(2026, 5, 20, 12, 0, 0).unwrap();
        assert_eq!(cat_age_anchor(created, Some(adopted)), adopted);
        assert_eq!(cat_age_anchor(created, None), created);
    }

    #[test]
    fn cat_age_label_formats_typical_durations() {
        use chrono::TimeZone;
        let now = Utc.with_ymd_and_hms(2026, 6, 1, 12, 0, 0).unwrap();
        let cases: &[(i64, &str)] = &[
            (0, "today"),
            (1, "1 day"),
            (3, "3 days"),
            (13, "13 days"),
            (14, "2 weeks"),
            (21, "3 weeks"),
            (30, "1 month"),
            (90, "3 months"),
            (180, "6 months"),
            (365, "1 year"),
            (800, "2 years"),
        ];
        for (days, expected) in cases {
            let created = now - chrono::Duration::days(*days);
            assert_eq!(
                cat_age_label(created, now),
                *expected,
                "wrong label for {days} days ago"
            );
        }
    }
}
