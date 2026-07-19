use std::collections::HashSet;
use std::sync::OnceLock;

use anyhow::{Context, Result, ensure};
use chrono::NaiveDate;
use late_core::db::Db;
use late_core::models::le_word::{DailyWin, DailyWord, Game, GameParams};
use late_core::models::profile::fetch_username;
use rand_core::{OsRng, RngCore};
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::app::activity::event::{ActivityEvent, ActivityGame};

const ANSWER_POOL: &str = include_str!("../../../../assets/le_word/answer_pool.txt");
const VALID_EXTRA: &str = include_str!("../../../../assets/le_word/valid_extra.txt");

static ANSWER_WORDS: OnceLock<Vec<&'static str>> = OnceLock::new();
static VALID_GUESSES: OnceLock<HashSet<&'static str>> = OnceLock::new();

#[derive(Clone)]
pub struct LeWordService {
    db: Db,
    activity_feed: broadcast::Sender<ActivityEvent>,
}

impl LeWordService {
    pub fn new(db: Db, activity_feed: broadcast::Sender<ActivityEvent>) -> Self {
        Self { db, activity_feed }
    }

    pub fn today(&self) -> NaiveDate {
        chrono::Utc::now().date_naive()
    }

    pub fn is_valid_guess(&self, guess: &str) -> bool {
        valid_guesses().contains(guess)
    }

    pub async fn ensure_daily_word(&self) -> Result<DailyWord> {
        let mut client = self.db.get().await?;
        let puzzle_date = self.today();

        if let Some(word) = DailyWord::find_by_date(&**client, puzzle_date).await? {
            return Ok(word);
        }

        let tx = client.transaction().await?;
        DailyWord::lock_daily_creation(&*tx).await?;

        if let Some(word) = DailyWord::find_by_date(&*tx, puzzle_date).await? {
            tx.commit().await?;
            return Ok(word);
        }

        let used = DailyWord::used_answer_words(&*tx).await?;
        let used: HashSet<&str> = used.iter().map(String::as_str).collect();
        let answer =
            choose_unused_answer(&used).context("failed to choose Le Word daily answer")?;
        let word = DailyWord::insert_for_date(&*tx, puzzle_date, answer).await?;
        tx.commit().await?;
        Ok(word)
    }

    pub async fn load_game(&self, user_id: Uuid, puzzle_date: NaiveDate) -> Result<Option<Game>> {
        let client = self.db.get().await?;
        Game::find_by_user_id_for_date(&client, user_id, puzzle_date).await
    }

    pub async fn has_won_today(&self, user_id: Uuid) -> Result<bool> {
        let client = self.db.get().await?;
        DailyWin::has_won_today(&client, user_id, self.today()).await
    }

    pub fn save_game_task(&self, params: GameParams) {
        let svc = self.clone();
        tokio::spawn(async move {
            if let Err(error) = svc.save_game(params).await {
                tracing::error!(error = ?error, "failed to save Le Word game state");
            }
        });
    }

    async fn save_game(&self, params: GameParams) -> Result<()> {
        let client = self.db.get().await?;
        Game::upsert(&client, params).await?;
        Ok(())
    }

    pub fn record_win_task(&self, user_id: Uuid, puzzle_date: NaiveDate, guesses_used: usize) {
        let svc = self.clone();
        tokio::spawn(async move {
            if let Err(error) = svc
                .record_win_and_publish(user_id, puzzle_date, guesses_used)
                .await
            {
                tracing::error!(error = ?error, "failed to record Le Word daily win");
            }
        });
    }

    async fn record_win_and_publish(
        &self,
        user_id: Uuid,
        puzzle_date: NaiveDate,
        guesses_used: usize,
    ) -> Result<()> {
        let score = guesses_used as i32;
        let client = self.db.get().await?;
        DailyWin::record_win(&client, user_id, puzzle_date, score).await?;
        let username = fetch_username(&client, user_id).await;
        let _ = self.activity_feed.send(ActivityEvent::game_won_at(
            user_id,
            username,
            ActivityGame::LeWord,
            Some("daily".to_string()),
            Some(score),
            ActivityEvent::occurred_on_utc_date(puzzle_date),
        ));
        Ok(())
    }
}

fn answer_words() -> &'static [&'static str] {
    ANSWER_WORDS
        .get_or_init(|| parse_words(ANSWER_POOL))
        .as_slice()
}

fn valid_guesses() -> &'static HashSet<&'static str> {
    VALID_GUESSES.get_or_init(|| {
        let mut words: HashSet<&'static str> = parse_words(ANSWER_POOL).into_iter().collect();
        words.extend(parse_words(VALID_EXTRA));
        words
    })
}

fn parse_words(source: &'static str) -> Vec<&'static str> {
    source
        .lines()
        .map(str::trim)
        .filter(|word| word.len() == 5 && word.bytes().all(|b| b.is_ascii_lowercase()))
        .collect()
}

fn choose_unused_answer<'a>(used: &HashSet<&str>) -> Result<&'a str>
where
    'static: 'a,
{
    let answers = answer_words();
    ensure!(
        used.len() < answers.len(),
        "Le Word answer pool has no unused words left"
    );

    loop {
        let idx = (OsRng.next_u64() as usize) % answers.len();
        let answer = answers[idx];
        if !used.contains(answer) {
            return Ok(answer);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supplied_word_pools_are_loaded() {
        assert_eq!(answer_words().len(), 2317);
        assert!(valid_guesses().contains("hunch"));
        assert!(valid_guesses().contains("noire"));
    }

    #[test]
    fn daily_selection_avoids_used_answers() {
        let mut used: HashSet<&str> = answer_words().iter().copied().collect();
        used.remove("hunch");
        for _ in 0..32 {
            assert_eq!(choose_unused_answer(&used).expect("answer"), "hunch");
        }
    }
}
