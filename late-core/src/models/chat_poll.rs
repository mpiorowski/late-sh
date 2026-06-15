use anyhow::{Result, bail, ensure};
use chrono::{DateTime, Duration, Utc};
use deadpool_postgres::GenericClient as DeadpoolGenericClient;
use std::collections::HashMap;
use tokio_postgres::{Client, GenericClient as TokioGenericClient};
use uuid::Uuid;

pub const POLL_QUESTION_MAX_CHARS: usize = 200;
pub const POLL_OPTION_MAX_CHARS: usize = 80;
pub const POLL_MAX_OPTIONS: usize = 3;
pub const POLL_MIN_OPTIONS: usize = 2;
pub const POLL_DURATION_OPTIONS_SECS: [i64; 3] = [10 * 60, 20 * 60, 30 * 60];

#[derive(Clone, Debug)]
pub struct ChatPoll {
    pub id: Uuid,
    pub created: DateTime<Utc>,
    pub updated: DateTime<Utc>,
    pub room_id: Uuid,
    pub user_id: Uuid,
    pub question: String,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub active: bool,
}

impl From<tokio_postgres::Row> for ChatPoll {
    fn from(row: tokio_postgres::Row) -> Self {
        Self {
            id: row.get("id"),
            created: row.get("created"),
            updated: row.get("updated"),
            room_id: row.get("room_id"),
            user_id: row.get("user_id"),
            question: row.get("question"),
            starts_at: row.get("starts_at"),
            ends_at: row.get("ends_at"),
            active: row.get("active"),
        }
    }
}

#[derive(Clone, Debug)]
pub struct ChatPollOptionSummary {
    pub id: Uuid,
    pub position: i32,
    pub label: String,
    pub vote_count: i64,
}

#[derive(Clone, Debug)]
pub struct ActiveChatPoll {
    pub poll: ChatPoll,
    pub options: Vec<ChatPollOptionSummary>,
    pub my_vote_option_id: Option<Uuid>,
}

#[derive(Clone, Debug)]
pub struct CreateChatPoll {
    pub user_id: Uuid,
    pub room_id: Uuid,
    pub question: String,
    pub options: Vec<String>,
    pub duration_secs: i64,
}

pub async fn ensure_can_start_poll(client: &Client, user_id: Uuid, room_id: Uuid) -> Result<()> {
    ensure_can_start_poll_with_client(client, user_id, room_id).await
}

async fn ensure_can_start_poll_with_client(
    client: &impl TokioGenericClient,
    user_id: Uuid,
    room_id: Uuid,
) -> Result<()> {
    let is_member = client
        .query_one(
            "SELECT EXISTS (
                 SELECT 1 FROM chat_room_members
                 WHERE user_id = $1 AND room_id = $2
             ) AS member",
            &[&user_id, &room_id],
        )
        .await?
        .get::<_, bool>("member");
    ensure!(is_member, "join the room before starting a poll");

    let active_poll = client
        .query_opt(
            "SELECT ends_at
             FROM chat_polls
             WHERE room_id = $1
               AND active = true
               AND ends_at > current_timestamp
             ORDER BY ends_at DESC
             LIMIT 1",
            &[&room_id],
        )
        .await?;
    if let Some(row) = active_poll {
        let ends_at: DateTime<Utc> = row.get("ends_at");
        bail!(
            "this room already has an active poll; ends in {}",
            format_poll_wait(ends_at - Utc::now())
        );
    }

    Ok(())
}

pub async fn create_poll(client: &mut Client, request: CreateChatPoll) -> Result<ActiveChatPoll> {
    let question = normalize_question(&request.question)?;
    let options = normalize_options(request.options)?;
    let duration_secs = normalize_duration_secs(request.duration_secs)?;
    let tx = client.transaction().await?;

    tx.query_one(
        "SELECT pg_advisory_xact_lock(
           hashtextextended(concat_ws(':', 'chat_poll', $1::uuid::text), 0)
         )",
        &[&request.room_id],
    )
    .await?;

    ensure_can_start_poll_with_client(&tx, request.user_id, request.room_id).await?;

    let ends_at = Utc::now() + Duration::seconds(duration_secs);
    let poll = tx
        .query_one(
            "INSERT INTO chat_polls (room_id, user_id, question, ends_at)
             VALUES ($1, $2, $3, $4)
             RETURNING *",
            &[&request.room_id, &request.user_id, &question, &ends_at],
        )
        .await?;
    let poll = ChatPoll::from(poll);

    for (index, option) in options.iter().enumerate() {
        let position = (index + 1) as i32;
        tx.execute(
            "INSERT INTO chat_poll_options (poll_id, position, label)
             VALUES ($1, $2, $3)",
            &[&poll.id, &position, option],
        )
        .await?;
    }

    tx.commit().await?;
    let mut active =
        list_active_polls_for_rooms(client, request.user_id, &[request.room_id]).await?;
    active
        .remove(&request.room_id)
        .ok_or_else(|| anyhow::anyhow!("created poll was not readable"))
}

pub async fn cast_vote(
    client: &mut Client,
    user_id: Uuid,
    poll_id: Uuid,
    option_position: i32,
) -> Result<ActiveChatPoll> {
    ensure!(
        (1..=POLL_MAX_OPTIONS as i32).contains(&option_position),
        "invalid poll option"
    );
    let tx = client.transaction().await?;

    let row = tx
        .query_opt(
            "SELECT p.room_id, o.id AS option_id
             FROM chat_polls p
             JOIN chat_poll_options o ON o.poll_id = p.id
             JOIN chat_room_members m ON m.room_id = p.room_id AND m.user_id = $2
             WHERE p.id = $1
               AND p.active = true
               AND p.ends_at > current_timestamp
               AND o.position = $3",
            &[&poll_id, &user_id, &option_position],
        )
        .await?;
    let Some(row) = row else {
        bail!("poll option is no longer available");
    };
    let room_id: Uuid = row.get("room_id");
    let option_id: Uuid = row.get("option_id");

    tx.execute(
        "INSERT INTO chat_poll_votes (poll_id, user_id, option_id)
         VALUES ($1, $2, $3)
         ON CONFLICT (poll_id, user_id) DO UPDATE SET
            option_id = EXCLUDED.option_id,
            updated = current_timestamp",
        &[&poll_id, &user_id, &option_id],
    )
    .await?;

    tx.commit().await?;
    let mut active = list_active_polls_for_rooms(client, user_id, &[room_id]).await?;
    active
        .remove(&room_id)
        .ok_or_else(|| anyhow::anyhow!("voted poll was not readable"))
}

pub async fn list_active_polls_for_rooms(
    client: &Client,
    user_id: Uuid,
    room_ids: &[Uuid],
) -> Result<HashMap<Uuid, ActiveChatPoll>> {
    if room_ids.is_empty() {
        return Ok(HashMap::new());
    }

    let poll_rows = client
        .query(
            "SELECT DISTINCT ON (room_id) *
	             FROM chat_polls
	             WHERE room_id = ANY($1)
	               AND active = true
	               AND ends_at > current_timestamp
	             ORDER BY room_id, ends_at DESC, id DESC",
            &[&room_ids],
        )
        .await?;
    let polls = poll_rows
        .into_iter()
        .map(ChatPoll::from)
        .collect::<Vec<_>>();
    if polls.is_empty() {
        return Ok(HashMap::new());
    }

    let poll_ids = polls.iter().map(|poll| poll.id).collect::<Vec<_>>();
    let option_rows = client
        .query(
            "SELECT
                o.poll_id,
                o.id,
                o.position,
                o.label,
                COUNT(v.user_id)::bigint AS vote_count
             FROM chat_poll_options o
             LEFT JOIN chat_poll_votes v ON v.option_id = o.id
             WHERE o.poll_id = ANY($1)
             GROUP BY o.poll_id, o.id, o.position, o.label
             ORDER BY o.poll_id, o.position",
            &[&poll_ids],
        )
        .await?;
    let mut options_by_poll: HashMap<Uuid, Vec<ChatPollOptionSummary>> = HashMap::new();
    for row in option_rows {
        options_by_poll
            .entry(row.get("poll_id"))
            .or_default()
            .push(ChatPollOptionSummary {
                id: row.get("id"),
                position: row.get("position"),
                label: row.get("label"),
                vote_count: row.get("vote_count"),
            });
    }

    let vote_rows = client
        .query(
            "SELECT poll_id, option_id
             FROM chat_poll_votes
             WHERE user_id = $1 AND poll_id = ANY($2)",
            &[&user_id, &poll_ids],
        )
        .await?;
    let mut votes_by_poll = HashMap::new();
    for row in vote_rows {
        votes_by_poll.insert(row.get::<_, Uuid>("poll_id"), row.get("option_id"));
    }

    Ok(polls
        .into_iter()
        .map(|poll| {
            let room_id = poll.room_id;
            let options = options_by_poll.remove(&poll.id).unwrap_or_default();
            let my_vote_option_id = votes_by_poll.remove(&poll.id);
            (
                room_id,
                ActiveChatPoll {
                    poll,
                    options,
                    my_vote_option_id,
                },
            )
        })
        .collect())
}

pub async fn list_expired_active_poll_ids(client: &Client, limit: i64) -> Result<Vec<Uuid>> {
    let rows = client
        .query(
            "SELECT id
             FROM chat_polls
             WHERE active = true
               AND ends_at <= current_timestamp
             ORDER BY ends_at ASC, id ASC
             LIMIT $1",
            &[&limit],
        )
        .await?;

    Ok(rows.into_iter().map(|row| row.get("id")).collect())
}

pub async fn claim_expired_poll(
    client: &impl DeadpoolGenericClient,
    poll_id: Uuid,
) -> Result<Option<ActiveChatPoll>> {
    let row = client
        .query_opt(
            "UPDATE chat_polls
             SET active = false, updated = current_timestamp
             WHERE id = $1
               AND active = true
               AND ends_at <= current_timestamp
             RETURNING *",
            &[&poll_id],
        )
        .await?;

    let Some(row) = row else {
        return Ok(None);
    };
    let poll = ChatPoll::from(row);
    let options = poll_option_summaries(client, &[poll.id])
        .await?
        .remove(&poll.id)
        .unwrap_or_default();

    Ok(Some(ActiveChatPoll {
        poll,
        options,
        my_vote_option_id: None,
    }))
}

fn normalize_question(question: &str) -> Result<String> {
    let question = question.trim();
    ensure!(!question.is_empty(), "poll question is required");
    ensure!(
        question.chars().count() <= POLL_QUESTION_MAX_CHARS,
        "poll question is too long"
    );
    Ok(question.to_string())
}

fn normalize_options(options: Vec<String>) -> Result<Vec<String>> {
    let options = options
        .into_iter()
        .map(|option| option.trim().to_string())
        .filter(|option| !option.is_empty())
        .collect::<Vec<_>>();
    ensure!(
        options.len() >= POLL_MIN_OPTIONS,
        "poll needs at least two options"
    );
    ensure!(
        options.len() <= POLL_MAX_OPTIONS,
        "poll supports at most three options"
    );
    for option in &options {
        ensure!(
            option.chars().count() <= POLL_OPTION_MAX_CHARS,
            "poll option is too long"
        );
    }
    Ok(options)
}

fn normalize_duration_secs(duration_secs: i64) -> Result<i64> {
    ensure!(
        POLL_DURATION_OPTIONS_SECS.contains(&duration_secs),
        "poll duration must be 10, 20, or 30 minutes"
    );
    Ok(duration_secs)
}

async fn poll_option_summaries(
    client: &impl DeadpoolGenericClient,
    poll_ids: &[Uuid],
) -> Result<HashMap<Uuid, Vec<ChatPollOptionSummary>>> {
    if poll_ids.is_empty() {
        return Ok(HashMap::new());
    }

    let option_rows = client
        .query(
            "SELECT
                o.poll_id,
                o.id,
                o.position,
                o.label,
                COUNT(v.user_id)::bigint AS vote_count
             FROM chat_poll_options o
             LEFT JOIN chat_poll_votes v ON v.option_id = o.id
             WHERE o.poll_id = ANY($1)
             GROUP BY o.poll_id, o.id, o.position, o.label
             ORDER BY o.poll_id, o.position",
            &[&poll_ids],
        )
        .await?;
    let mut options_by_poll: HashMap<Uuid, Vec<ChatPollOptionSummary>> = HashMap::new();
    for row in option_rows {
        options_by_poll
            .entry(row.get("poll_id"))
            .or_default()
            .push(ChatPollOptionSummary {
                id: row.get("id"),
                position: row.get("position"),
                label: row.get("label"),
                vote_count: row.get("vote_count"),
            });
    }

    Ok(options_by_poll)
}

fn format_poll_wait(duration: Duration) -> String {
    let seconds = duration.num_seconds().max(1);
    if seconds < 60 {
        return format!("{seconds}s");
    }
    let minutes = (seconds + 59) / 60;
    if minutes < 60 {
        return format!("{minutes}m");
    }
    let hours = minutes / 60;
    let remaining_minutes = minutes % 60;
    if remaining_minutes == 0 {
        format!("{hours}h")
    } else {
        format!("{hours}h {remaining_minutes}m")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_duration_accepts_configured_options() {
        for duration_secs in POLL_DURATION_OPTIONS_SECS {
            assert_eq!(
                normalize_duration_secs(duration_secs).unwrap(),
                duration_secs
            );
        }
    }

    #[test]
    fn normalize_duration_rejects_unconfigured_values() {
        assert!(normalize_duration_secs(5 * 60).is_err());
        assert!(normalize_duration_secs(40 * 60).is_err());
    }
}
