use chrono::NaiveDate;
use late_core::models::{
    door_opening::DoorCard,
    leaderboard::{DailyPuzzle, DayGame, DayTally, LobbyGame, ScoreGame},
};

use super::week_card;
use crate::pages::shared::card::{CardLine, Ink};

/// The card: the masthead, the week, a section per shelf anyone played
/// on, one line per game with everyone's plays and nobody's name, and
/// the way in.
#[test]
fn the_week_card_lists_each_games_plays_and_nobody() {
    let last = NaiveDate::from_ymd_opt(2026, 10, 24).unwrap();
    let tally = |game, plays| DayTally {
        game,
        plays,
        won: 0,
    };
    let tallies = [
        tally(DayGame::Daily(DailyPuzzle::Sudoku), 61),
        tally(DayGame::Score(ScoreGame::Lateris), 1_234),
        tally(DayGame::Lobby(LobbyGame::EightBall), 1),
        tally(DayGame::Door(DoorCard::Nethack), 52),
    ];
    let line = |ink, text: &str| CardLine::new(ink, text);
    assert_eq!(
        week_card(last, &tallies),
        vec![
            line(Ink::Title, "THE LATE.SH GAMES"),
            line(Ink::Dim, "everything played, 18 October to 24 October 2026"),
            CardLine::blank(),
            line(Ink::Heading, "ARCADE"),
            line(
                Ink::Body,
                "Sudoku                                                  61 plays"
            ),
            line(
                Ink::Body,
                "Lateris                                              1,234 plays"
            ),
            CardLine::blank(),
            line(Ink::Heading, "LOBBY"),
            line(
                Ink::Body,
                "Eight-Ball                                                1 play"
            ),
            CardLine::blank(),
            line(Ink::Heading, "DOORS"),
            line(
                Ink::Body,
                "NetHack                                                 52 plays"
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
