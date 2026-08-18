// A durable, tamper-resistant record that an account graduated BashQuest
// (completed every level, via the bashquest door). Written only by late-ssh,
// and only after late-bashquest independently confirms bashquest.sh wrote a
// graduation certificate on its own PVC for that session's account -- the
// player never gets shell access to that filesystem, so this table can only
// ever grow from an actual completion, never a self-reported claim (see
// late-ssh/src/app/door/bashquest/CONTEXT.md). Nothing reads this table
// yet: exposing graduates anywhere is its own change.

use anyhow::Result;
use uuid::Uuid;

crate::model! {
    table = "bashquest_graduates";
    params = BashquestGraduateParams;
    struct BashquestGraduate {
        @data
        pub user_id: Option<Uuid>,
        pub handle: String,
        pub certificate: String,
        pub certificate_digest: String,
    }
}

impl BashquestGraduate {
    /// The account's graduation record, if any.
    pub async fn find_by_user_id(
        client: &impl deadpool_postgres::GenericClient,
        user_id: Uuid,
    ) -> Result<Option<Self>> {
        let row = client
            .query_opt(
                "SELECT * FROM bashquest_graduates WHERE user_id = $1",
                &[&user_id],
            )
            .await?;
        Ok(row.map(Self::from))
    }

    /// Record a graduation. First record wins and is immutable; a re-report
    /// (a graduate logging back in to keep playing) is a harmless no-op,
    /// enforced race-safely by the table's unique constraint on `user_id`.
    /// The caller is trusted: only late-ssh's bashquest proxy calls this,
    /// and only after receiving the host-originated certificate marker (see
    /// `door::bashquest::proxy`), never from anything client-supplied.
    pub async fn record(
        client: &impl deadpool_postgres::GenericClient,
        user_id: Uuid,
        handle: &str,
        certificate: &str,
        certificate_digest: &str,
    ) -> Result<bool> {
        let inserted = client
            .execute(
                "INSERT INTO bashquest_graduates (user_id, handle, certificate, certificate_digest)
                 VALUES ($1, $2, $3, $4)
                 ON CONFLICT (user_id) DO NOTHING",
                &[&user_id, &handle, &certificate, &certificate_digest],
            )
            .await?;
        Ok(inserted == 1)
    }

    /// Every graduate whose account still exists, oldest first.
    ///
    /// No caller yet: nothing exposes graduates anywhere. It lives here so the
    /// table's reads stay in one module, and so the scoping decision is
    /// already made when something does consume it.
    ///
    /// `user_id IS NULL` means the account was deleted (the column is
    /// `ON DELETE SET NULL`, so the row itself survives as a historical fact).
    /// Those are filtered out rather than dropped from the table: anything
    /// built on this list is keyed on the player's handle, and someone who
    /// deletes their late.sh account should stop appearing in it.
    pub async fn list_all(client: &impl deadpool_postgres::GenericClient) -> Result<Vec<Self>> {
        let rows = client
            .query(
                "SELECT * FROM bashquest_graduates
                 WHERE user_id IS NOT NULL
                 ORDER BY created ASC",
                &[],
            )
            .await?;
        Ok(rows.into_iter().map(Self::from).collect())
    }
}
