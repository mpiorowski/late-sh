use std::error::Error;

use anyhow::Result;
use bytes::BytesMut;
use tokio_postgres::Client;
use tokio_postgres::types::{FromSql, IsNull, ToSql, Type, to_sql_checked};
use uuid::Uuid;

/// Whether the person wants to hear about work (migration 041). Closed: the
/// list row colours it, the web index sorts by it, and the paper decides
/// whether to show job matches on it, so every reader names all three.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WorkStatus {
    Open,
    Casual,
    NotLooking,
}

impl WorkStatus {
    pub const ALL: [Self; 3] = [Self::Open, Self::Casual, Self::NotLooking];

    /// The persisted `work_profiles.status` value.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Casual => "casual",
            Self::NotLooking => "not-looking",
        }
    }

    /// What the row and the card print.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Casual => "casual",
            Self::NotLooking => "not looking",
        }
    }

    /// A status the database wrote. A value outside the CHECK constraint is
    /// impossible, so this crashes rather than inventing one.
    pub fn from_db(value: &str) -> Self {
        match value {
            "open" => Self::Open,
            "casual" => Self::Casual,
            "not-looking" => Self::NotLooking,
            other => panic!("unknown work profile status in the database: {other}"),
        }
    }

    /// Open first, then casual, then not looking: the web index order.
    pub const fn rank(self) -> u8 {
        match self {
            Self::Open => 0,
            Self::Casual => 1,
            Self::NotLooking => 2,
        }
    }

    pub const fn next(self) -> Self {
        match self {
            Self::Open => Self::Casual,
            Self::Casual => Self::NotLooking,
            Self::NotLooking => Self::Open,
        }
    }

    pub const fn prev(self) -> Self {
        match self {
            Self::Open => Self::NotLooking,
            Self::Casual => Self::Open,
            Self::NotLooking => Self::Casual,
        }
    }
}

/// The shape of work the person is after (migration 192). `Any` is a real
/// variant rather than an absent choice so the column is never null and a
/// job match can say "open to any" instead of nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WorkType {
    FullTime,
    Contract,
    Freelance,
    PartTime,
    Any,
}

impl WorkType {
    pub const ALL: [Self; 5] = [
        Self::FullTime,
        Self::Contract,
        Self::Freelance,
        Self::PartTime,
        Self::Any,
    ];

    /// The persisted `work_profiles.work_type` value.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::FullTime => "full-time",
            Self::Contract => "contract",
            Self::Freelance => "freelance",
            Self::PartTime => "part-time",
            Self::Any => "any",
        }
    }

    /// What the row and the card print.
    pub const fn label(self) -> &'static str {
        match self {
            Self::FullTime => "full-time",
            Self::Contract => "contract",
            Self::Freelance => "freelance",
            Self::PartTime => "part-time",
            Self::Any => "open to any",
        }
    }

    /// A type the database wrote; see [`WorkStatus::from_db`].
    pub fn from_db(value: &str) -> Self {
        match value {
            "full-time" => Self::FullTime,
            "contract" => Self::Contract,
            "freelance" => Self::Freelance,
            "part-time" => Self::PartTime,
            "any" => Self::Any,
            other => panic!("unknown work profile type in the database: {other}"),
        }
    }

    pub const fn next(self) -> Self {
        match self {
            Self::FullTime => Self::Contract,
            Self::Contract => Self::Freelance,
            Self::Freelance => Self::PartTime,
            Self::PartTime => Self::Any,
            Self::Any => Self::FullTime,
        }
    }

    pub const fn prev(self) -> Self {
        match self {
            Self::FullTime => Self::Any,
            Self::Contract => Self::FullTime,
            Self::Freelance => Self::Contract,
            Self::PartTime => Self::Freelance,
            Self::Any => Self::PartTime,
        }
    }
}

/// Both enums travel as their `TEXT` column: the model macro reads every
/// field with `row.get` and writes every param as `&dyn ToSql`, so the
/// parse and print live here at the row boundary and nowhere else.
macro_rules! text_column_enum {
    ($name:ident) => {
        impl<'a> FromSql<'a> for $name {
            fn from_sql(ty: &Type, raw: &'a [u8]) -> Result<Self, Box<dyn Error + Sync + Send>> {
                let value = <&str as FromSql>::from_sql(ty, raw)?;
                Ok(Self::from_db(value))
            }

            fn accepts(ty: &Type) -> bool {
                <&str as FromSql>::accepts(ty)
            }
        }

        impl ToSql for $name {
            fn to_sql(
                &self,
                ty: &Type,
                out: &mut BytesMut,
            ) -> Result<IsNull, Box<dyn Error + Sync + Send>> {
                <&str as ToSql>::to_sql(&self.as_str(), ty, out)
            }

            fn accepts(ty: &Type) -> bool {
                <&str as ToSql>::accepts(ty)
            }

            to_sql_checked!();
        }
    };
}

text_column_enum!(WorkStatus);
text_column_enum!(WorkType);

// `skills` is what the person wrote, for display; `skills_tags` is the
// same list run through the tag vocabulary, for the job matcher.
crate::user_scoped_model! {
    table = "work_profiles";
    user_field = user_id;
    params = WorkProfileParams;
    struct WorkProfile {
        @data
        pub user_id: Uuid,
        pub slug: String,
        pub headline: String,
        pub status: WorkStatus,
        pub work_type: WorkType,
        pub location: String,
        pub contact: String,
        pub links: Vec<String>,
        pub skills: Vec<String>,
        pub skills_tags: Vec<String>,
        pub summary: String,
    }
}

impl WorkProfile {
    /// Every card, freshest first: the Profiles page feed.
    pub async fn list_recent(client: &Client, limit: i64) -> Result<Vec<Self>> {
        let rows = client
            .query(
                "SELECT * FROM work_profiles ORDER BY updated DESC, created DESC, id DESC LIMIT $1",
                &[&limit],
            )
            .await?;
        Ok(rows.into_iter().map(Self::from).collect())
    }

    /// Every card in the web index's order: open, then casual, then not
    /// looking, freshest first inside each. The order is the enum's
    /// [`WorkStatus::rank`], spelled out so the database sorts it.
    pub async fn list_index(client: &Client, limit: i64) -> Result<Vec<Self>> {
        let rows = client
            .query(
                "SELECT * FROM work_profiles
                 ORDER BY CASE status
                     WHEN 'open' THEN 0
                     WHEN 'casual' THEN 1
                     ELSE 2
                 END, updated DESC, created DESC, id DESC
                 LIMIT $1",
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
