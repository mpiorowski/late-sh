use std::collections::HashSet;
use std::sync::{Arc, OnceLock};

use anyhow::{Context, Result, ensure};
use chrono::NaiveDate;
use late_core::db::Db;
use late_core::models::le_word::{DailyWin, DailyWord, Game, GameParams};
use late_core::models::profile::fetch_username;
use rand_core::{OsRng, RngCore};
use tokio::sync::{broadcast, mpsc};
use uuid::Uuid;

use crate::app::activity::event::{ActivityEvent, ActivityGame};

#[cfg(test)]
use anyhow::anyhow;
#[cfg(test)]
use tokio::sync::oneshot;

const ANSWER_POOL: &str = include_str!("../../../../assets/le_word/answer_pool.txt");
const VALID_EXTRA: &str = include_str!("../../../../assets/le_word/valid_extra.txt");

static ANSWER_WORDS: OnceLock<Vec<&'static str>> = OnceLock::new();
static VALID_GUESSES: OnceLock<HashSet<&'static str>> = OnceLock::new();

#[derive(Clone)]
pub struct LeWordService {
    db: Db,
    activity_feed: broadcast::Sender<ActivityEvent>,
    game_save_tx: Arc<OnceLock<mpsc::UnboundedSender<GameSaveCommand>>>,
}

enum GameSaveCommand {
    Save(GameParams),
    #[cfg(test)]
    Flush(oneshot::Sender<()>),
}

impl LeWordService {
    pub fn new(db: Db, activity_feed: broadcast::Sender<ActivityEvent>) -> Self {
        Self {
            db,
            activity_feed,
            game_save_tx: Arc::new(OnceLock::new()),
        }
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

    pub async fn load_games(&self, user_id: Uuid) -> Result<Vec<Game>> {
        let client = self.db.get().await?;
        Game::list_by_user_id(&client, user_id).await
    }

    pub fn replay_answer(&self, current_answer: &str, daily_answer: Option<&str>) -> &'static str {
        choose_replay_answer(current_answer, daily_answer)
    }

    pub async fn has_won_today(&self, user_id: Uuid) -> Result<bool> {
        let client = self.db.get().await?;
        DailyWin::has_won_today(&client, user_id, self.today()).await
    }

    pub fn save_game_task(&self, params: GameParams) {
        if self
            .game_save_sender()
            .send(GameSaveCommand::Save(params))
            .is_err()
        {
            tracing::error!("failed to enqueue Le Word game state save");
        }
    }

    #[cfg(test)]
    async fn flush_game_saves(&self) -> Result<()> {
        let (done_tx, done_rx) = oneshot::channel();
        self.game_save_sender()
            .send(GameSaveCommand::Flush(done_tx))
            .map_err(|_| anyhow!("Le Word game save queue is closed"))?;
        done_rx
            .await
            .context("Le Word game save worker stopped before flush")?;
        Ok(())
    }

    fn game_save_sender(&self) -> &mpsc::UnboundedSender<GameSaveCommand> {
        self.game_save_tx.get_or_init(|| {
            let (save_tx, save_rx) = mpsc::unbounded_channel();
            tokio::spawn(run_game_save_worker(self.db.clone(), save_rx));
            save_tx
        })
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

async fn run_game_save_worker(db: Db, mut save_rx: mpsc::UnboundedReceiver<GameSaveCommand>) {
    while let Some(command) = save_rx.recv().await {
        match command {
            GameSaveCommand::Save(params) => {
                let result = async {
                    let client = db.get().await?;
                    Game::upsert(&client, params).await?;
                    Result::<()>::Ok(())
                }
                .await;
                if let Err(error) = result {
                    tracing::error!(error = ?error, "failed to save Le Word game state");
                }
            }
            #[cfg(test)]
            GameSaveCommand::Flush(done_tx) => {
                let _ = done_tx.send(());
            }
        }
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

fn choose_replay_answer(current_answer: &str, daily_answer: Option<&str>) -> &'static str {
    choose_replay_answer_from(answer_words(), current_answer, daily_answer)
}

fn choose_replay_answer_from<'a>(
    answers: &'a [&'a str],
    current_answer: &str,
    daily_answer: Option<&str>,
) -> &'a str {
    loop {
        let answer = answers[(OsRng.next_u64() as usize) % answers.len()];
        if answer != current_answer && Some(answer) != daily_answer {
            return answer;
        }
    }
}

#[cfg(test)]
#[path = "svc_test.rs"]
mod svc_test;
