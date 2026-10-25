use chrono::NaiveDate;
use late_core::models::leaderboard::{
    DailyPuzzle, DoorGame, LobbyGame, ScoreGame, WeekBest, WeekGame,
};

use super::week_card;
use crate::pages::shared::card::{CardLine, Ink};

/// The card: the masthead, the week, a section per shelf anyone played
/// on, one line per game in three columns with the player credited
/// `~name`, and the way in.
#[test]
fn the_week_card_lists_each_game_its_best_and_who() {
    let last = NaiveDate::from_ymd_opt(2026, 10, 24).unwrap();
    let best = |game, username: &str, value| WeekBest {
        game,
        username: username.to_string(),
        value,
    };
    let bests = [
        best(WeekGame::Daily(DailyPuzzle::Sudoku), "alice", 6),
        best(WeekGame::Score(ScoreGame::Lateris), "bob", 12_340),
        best(WeekGame::Lobby(LobbyGame::EightBall), "carol", 2),
        best(WeekGame::Door(DoorGame::Nethack), "dave", 52_340),
    ];
    let line = |ink, text: &str| CardLine::new(ink, text);
    assert_eq!(
        week_card(last, &bests),
        vec![
            line(Ink::Title, "THE LATE.SH GAMES"),
            line(Ink::Dim, "the week's best, 18 October to 24 October 2026"),
            CardLine::blank(),
            line(Ink::Heading, "ARCADE"),
            line(
                Ink::Body,
                "Sudoku            ~alice                                  6 wins"
            ),
            line(
                Ink::Body,
                "Lateris           ~bob                                    12,340"
            ),
            CardLine::blank(),
            line(Ink::Heading, "LOBBY"),
            line(
                Ink::Body,
                "Eight-Ball        ~carol                                  2 wins"
            ),
            CardLine::blank(),
            line(Ink::Heading, "DOORS"),
            line(
                Ink::Body,
                "NetHack           ~dave                                   52,340"
            ),
            CardLine::blank(),
            line(Ink::Dim, "ssh late.sh"),
        ]
    );
}

/// The route serves a played week as a PNG; a week nobody played and a
/// path that is not a date are 404.
#[tokio::test]
async fn the_week_card_is_a_png_for_a_week_someone_played() {
    use axum::{
        body::{Body, to_bytes},
        http::{Request, StatusCode},
    };
    use chrono::Utc;
    use late_core::{
        db::DbConfig,
        models::tetris,
        test_utils::{create_test_user, test_db},
    };
    use tower::ServiceExt;

    let test = test_db().await;
    let client = test.db.get().await.unwrap();
    let player = create_test_user(&test.db, "week-player").await;
    tetris::HighScore::record_score_event(&client, player.id, 4_200)
        .await
        .unwrap();

    let router = crate::app(crate::AppState {
        config: crate::config::Config {
            env: crate::config::Env::Dev,
            port: 0,
            ssh_internal_url: "http://127.0.0.1:9".into(),
            audio_base_url: "http://127.0.0.1:9".into(),
            public_base_url: "http://late.test".into(),
            db: DbConfig::default(),
        },
        db: test.db.clone(),
        http_client: reqwest::Client::new(),
    });
    let fetch = |uri: String| {
        let router = router.clone();
        async move {
            let response = router
                .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
                .await
                .unwrap();
            let status = response.status();
            let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
            (status, body.to_vec())
        }
    };
    let today = Utc::now().date_naive();
    let (status, png) = fetch(format!("/games/week/{today}/image.png")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(png.starts_with(b"\x89PNG"));
    let quiet = today - chrono::Duration::days(30);
    assert_eq!(
        fetch(format!("/games/week/{quiet}/image.png")).await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        fetch("/games/week/sunday/image.png".to_string()).await.0,
        StatusCode::NOT_FOUND
    );
}
