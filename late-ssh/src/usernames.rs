use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
    time::Duration,
};

use anyhow::Result;
use late_core::{
    MutexRecover,
    db::Db,
    models::user::{USERNAME_MAX_LEN, User},
    shutdown::CancellationToken,
};
use rand::Rng;
use tokio::task::JoinHandle;
use uuid::Uuid;

const CURATED_MODIFIERS: &str = include_str!("../assets/usernames/curated-modifiers.txt");
const CURATED_NOUNS: &str = include_str!("../assets/usernames/curated-nouns.txt");

pub const USERNAME_DIRECTORY_REFRESH_INTERVAL: Duration = Duration::from_secs(30 * 60);

pub type UsernameDirectory = Arc<Mutex<Arc<HashMap<Uuid, String>>>>;

pub struct UsernameLookup<'a> {
    chat: &'a HashMap<Uuid, String>,
    directory: Option<&'a HashMap<Uuid, String>>,
}

pub trait UsernameResolver {
    fn username(&self, user_id: &Uuid) -> Option<&String>;
}

impl<'a> UsernameLookup<'a> {
    pub fn new(
        chat: &'a HashMap<Uuid, String>,
        directory: Option<&'a HashMap<Uuid, String>>,
    ) -> Self {
        Self { chat, directory }
    }

    pub fn get(&self, user_id: &Uuid) -> Option<&'a String> {
        match self.directory {
            Some(directory) => directory.get(user_id),
            None => self.chat.get(user_id),
        }
    }
}

impl UsernameResolver for HashMap<Uuid, String> {
    fn username(&self, user_id: &Uuid) -> Option<&String> {
        self.get(user_id)
    }
}

impl UsernameResolver for UsernameLookup<'_> {
    fn username(&self, user_id: &Uuid) -> Option<&String> {
        self.get(user_id)
    }
}

pub async fn load(db: &Db) -> Result<UsernameDirectory> {
    let client = db.get().await?;
    let usernames = User::list_all_username_map(&client).await?;
    Ok(Arc::new(Mutex::new(Arc::new(usernames))))
}

pub fn snapshot(directory: &UsernameDirectory) -> Arc<HashMap<Uuid, String>> {
    Arc::clone(&directory.lock_recover())
}

pub fn get(directory: &UsernameDirectory, user_id: Uuid) -> Option<String> {
    directory.lock_recover().get(&user_id).cloned()
}

pub fn upsert(directory: &UsernameDirectory, user_id: Uuid, username: impl Into<String>) {
    let username = username.into();
    let mut guard = directory.lock_recover();
    let usernames = Arc::make_mut(&mut *guard);
    if username.trim().is_empty() {
        usernames.remove(&user_id);
    } else {
        usernames.insert(user_id, username);
    }
}

pub fn remove(directory: &UsernameDirectory, user_id: Uuid) {
    let mut guard = directory.lock_recover();
    Arc::make_mut(&mut *guard).remove(&user_id);
}

/// Pick an unused account name without incorporating the SSH login username.
///
/// The randomized traversal visits every modifier/noun pair once. If every
/// rendered base name is occupied, an incrementing counter is appended to the
/// first randomly selected base until an unused name is found.
pub(crate) async fn next_generated_username(client: &tokio_postgres::Client) -> Result<String> {
    let occupied = User::list_all_usernames(client)
        .await?
        .into_iter()
        .map(|username| username.to_ascii_lowercase())
        .collect();
    let modifiers = curated_words(CURATED_MODIFIERS);
    let nouns = curated_words(CURATED_NOUNS);
    let mut rng = rand::thread_rng();
    Ok(select_generated_username(
        &modifiers, &nouns, &occupied, &mut rng,
    ))
}

fn curated_words(contents: &str) -> Vec<&str> {
    contents.lines().filter(|word| !word.is_empty()).collect()
}

fn select_generated_username<R: Rng + ?Sized>(
    modifiers: &[&str],
    nouns: &[&str],
    occupied: &HashSet<String>,
    rng: &mut R,
) -> String {
    assert!(!modifiers.is_empty(), "modifier wordlist must not be empty");
    assert!(!nouns.is_empty(), "noun wordlist must not be empty");

    let combination_count = modifiers
        .len()
        .checked_mul(nouns.len())
        .expect("username wordlist combination count overflowed");
    let start = rng.gen_range(0..combination_count);
    let step = random_coprime_step(combination_count, rng);
    let fallback_base = combination_at(start, modifiers, nouns);
    let mut index = start;

    for _ in 0..combination_count {
        let candidate = combination_at(index, modifiers, nouns);
        if !occupied.contains(&candidate.to_ascii_lowercase()) {
            return candidate;
        }
        index = (index + step) % combination_count;
    }

    for counter in 1_u64.. {
        let suffix = counter.to_string();
        let max_base_len = USERNAME_MAX_LEN.saturating_sub(suffix.len());
        let candidate = format!(
            "{}{}",
            fallback_base.chars().take(max_base_len).collect::<String>(),
            suffix
        );
        if !occupied.contains(&candidate.to_ascii_lowercase()) {
            return candidate;
        }
    }

    unreachable!("u64 username counter space exhausted")
}

fn random_coprime_step<R: Rng + ?Sized>(combination_count: usize, rng: &mut R) -> usize {
    if combination_count == 1 {
        return 1;
    }

    loop {
        let step = rng.gen_range(1..combination_count);
        if greatest_common_divisor(step, combination_count) == 1 {
            return step;
        }
    }
}

fn greatest_common_divisor(mut left: usize, mut right: usize) -> usize {
    while right != 0 {
        (left, right) = (right, left % right);
    }
    left
}

fn combination_at(index: usize, modifiers: &[&str], nouns: &[&str]) -> String {
    let modifier = modifiers[index / nouns.len()];
    let noun = nouns[index % nouns.len()];
    format!("{modifier}{noun}")
}

#[cfg(test)]
pub(crate) fn is_curated_base_username(username: &str) -> bool {
    let modifiers = curated_words(CURATED_MODIFIERS);
    let nouns = curated_words(CURATED_NOUNS);
    modifiers.iter().any(|modifier| {
        username
            .strip_prefix(modifier)
            .is_some_and(|noun| nouns.contains(&noun))
    })
}

pub fn start_refresh_task(
    db: Db,
    directory: UsernameDirectory,
    shutdown: CancellationToken,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(USERNAME_DIRECTORY_REFRESH_INTERVAL);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        interval.tick().await;

        loop {
            tokio::select! {
                _ = shutdown.cancelled() => break,
                _ = interval.tick() => {
                    match refresh(&db, &directory).await {
                        Ok(count) => tracing::debug!(count, "username directory refreshed"),
                        Err(error) => {
                            tracing::warn!(error = ?error, "failed to refresh username directory");
                        }
                    }
                }
            }
        }
    })
}

async fn refresh(db: &Db, directory: &UsernameDirectory) -> Result<usize> {
    let client = db.get().await?;
    let usernames = User::list_all_username_map(&client).await?;
    let count = usernames.len();
    *directory.lock_recover() = Arc::new(usernames);
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::{SeedableRng, rngs::StdRng};

    #[test]
    fn curated_wordlists_fit_the_username_contract() {
        let modifiers = curated_words(CURATED_MODIFIERS);
        let nouns = curated_words(CURATED_NOUNS);

        assert_eq!(modifiers.len(), 353);
        assert_eq!(nouns.len(), 605);
        assert_eq!(modifiers.len() * nouns.len(), 213_565);
        assert!(modifiers.iter().chain(&nouns).all(|word| {
            word.chars()
                .all(|character| character.is_ascii_alphabetic())
        }));

        let max_generated_len = modifiers
            .iter()
            .map(|modifier| modifier.chars().count())
            .max()
            .unwrap()
            + nouns.iter().map(|noun| noun.chars().count()).max().unwrap();
        assert_eq!(max_generated_len, 25);
        assert!(max_generated_len <= USERNAME_MAX_LEN);

        let distinct_usernames = modifiers
            .iter()
            .flat_map(|modifier| {
                nouns
                    .iter()
                    .map(move |noun| format!("{modifier}{noun}").to_ascii_lowercase())
            })
            .collect::<HashSet<_>>();
        assert_eq!(distinct_usernames.len(), 213_565);
    }

    #[test]
    fn occupied_random_choice_moves_to_another_combination() {
        let modifiers = ["Quiet", "Mossy"];
        let nouns = ["Stoat", "Finch"];
        let mut first_rng = StdRng::seed_from_u64(7);
        let first = select_generated_username(&modifiers, &nouns, &HashSet::new(), &mut first_rng);
        let occupied = HashSet::from([first.to_ascii_lowercase()]);
        let mut retry_rng = StdRng::seed_from_u64(7);
        let retry = select_generated_username(&modifiers, &nouns, &occupied, &mut retry_rng);

        assert_ne!(retry, first);
        assert!(!occupied.contains(&retry.to_ascii_lowercase()));
    }

    #[test]
    fn exhausted_combinations_append_an_incrementing_counter() {
        let modifiers = ["Quiet"];
        let nouns = ["Stoat"];
        let occupied = HashSet::from(["quietstoat".to_string(), "quietstoat1".to_string()]);
        let mut rng = StdRng::seed_from_u64(7);

        assert_eq!(
            select_generated_username(&modifiers, &nouns, &occupied, &mut rng),
            "QuietStoat2"
        );
    }

    #[test]
    fn counter_fallback_stays_within_the_persisted_username_limit() {
        let modifiers = ["ABCDEFGHIJKLMNOP"];
        let nouns = ["QRSTUVWXYZABCDEF"];
        let base = format!("{}{}", modifiers[0], nouns[0]);
        assert_eq!(base.len(), USERNAME_MAX_LEN);
        let occupied = HashSet::from([base.to_ascii_lowercase()]);
        let mut rng = StdRng::seed_from_u64(7);

        let generated = select_generated_username(&modifiers, &nouns, &occupied, &mut rng);
        assert_eq!(generated.len(), USERNAME_MAX_LEN);
        assert!(generated.ends_with('1'));
    }
}
