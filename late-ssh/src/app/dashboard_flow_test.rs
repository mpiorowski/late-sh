//! App-level dashboard input integration tests against a real ephemeral DB.

use crate::paired_clients::PairControlMessage;
use crate::test_helpers::{
    make_app, make_app_with_paired_client, new_test_db, render_plain, wait_for_app,
    wait_for_render_contains,
};
use late_core::models::{
    chat_message::{ChatMessage, ChatMessageParams},
    chat_room::ChatRoom,
    chat_room_member::ChatRoomMember,
};
use late_core::test_utils::create_test_user;

async fn make_app_harness() -> (late_core::test_utils::TestDb, crate::app::state::App) {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "todo-it").await;
    let app = make_app(test_db.db.clone(), user.id, "todo-flow-it");
    (test_db, app)
}

#[tokio::test]
async fn guide_routing_preserves_dashboard_content_input_and_lateania_context() {
    let (_test_db, mut app) = make_app_harness().await;

    wait_for_render_contains(&mut app, " Home ").await;

    app.handle_input(b"\x12");
    let frame = render_plain(&mut app);
    assert!(!frame.contains("Install `late` / Listen Anywhere"));
    assert!(!frame.contains("Browser pairing"));

    // The old terminal FAQ byte no longer opens a standalone modal; those
    // topics now live in the guide.
    app.handle_input(b"\x0c");
    assert!(
        !render_plain(&mut app).contains("Why copy sometimes silently fails"),
        "the legacy help byte must remain inert"
    );

    app.handle_input(b"b");
    assert!(
        !render_plain(&mut app).contains("Install `late` / Listen Anywhere"),
        "lowercase b should not open the guide"
    );

    app.handle_input(b"?");
    wait_for_render_contains(&mut app, "Install `late` / Listen Anywhere").await;
    wait_for_render_contains(&mut app, "https://cli.late.sh/install.sh | bash").await;
    wait_for_render_contains(&mut app, "https://cli.late.sh/install.ps1 | iex").await;
    wait_for_render_contains(&mut app, "What `late` unlocks").await;
    wait_for_render_contains(&mut app, "CLI YouTube").await;
    wait_for_render_contains(&mut app, "?/Esc/q close").await;

    app.handle_input(b"\x1b[<35;20;5M");
    wait_for_render_contains(&mut app, "Install `late` / Listen Anywhere").await;

    app.handle_input(b"?");
    assert!(
        !render_plain(&mut app).contains("Install `late` / Listen Anywhere"),
        "? should close the guide"
    );

    app.handle_input(b"?");
    wait_for_render_contains(&mut app, "Install `late` / Listen Anywhere").await;
    for _ in 0..30 {
        app.handle_input(b"j");
    }
    wait_for_render_contains(&mut app, "Listen without the CLI").await;
    wait_for_render_contains(&mut app, "█▀▀▀▀▀█").await;

    app.handle_input(b"q");
    assert!(
        !render_plain(&mut app).contains("Listen without the CLI"),
        "q should close the guide"
    );

    // Lateania has no top-level key: open Games and launch its default card,
    // then verify that ? selects the screen-specific guide section.
    app.handle_input(b"3");
    wait_for_render_contains(&mut app, " Games ").await;
    app.handle_input(b"\r");
    wait_for_render_contains(&mut app, " Lateania ").await;

    app.handle_input(b"?");
    wait_for_render_contains(&mut app, "Lateania is the persistent BBS-style world").await;
    assert!(
        !render_plain(&mut app).contains("Install `late` / Listen Anywhere"),
        "Lateania should select its own guide section"
    );
}

#[tokio::test]
async fn r_refresh_on_dashboard_keeps_dashboard_visible() {
    let (_test_db, mut app) = make_app_harness().await;

    wait_for_render_contains(&mut app, " Home ").await;
    app.handle_input(b"r");
    wait_for_render_contains(&mut app, " Home ").await;
}

#[tokio::test]
async fn m_on_dashboard_sends_toggle_to_paired_client() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "paired-browser-it").await;
    let (mut app, mut rx) =
        make_app_with_paired_client(test_db.db.clone(), user.id, "paired-browser-flow-it");

    app.handle_input(b"m");

    assert_eq!(rx.try_recv().unwrap(), PairControlMessage::ToggleMute);
    wait_for_render_contains(&mut app, "Sent mute toggle to paired client").await;
}

#[tokio::test]
async fn plus_and_minus_send_volume_controls_to_paired_client() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "paired-volume-it").await;
    let (mut app, mut rx) =
        make_app_with_paired_client(test_db.db.clone(), user.id, "paired-volume-flow-it");

    app.handle_input(b"+");
    assert_eq!(rx.try_recv().unwrap(), PairControlMessage::VolumeUp);
    wait_for_render_contains(&mut app, "Sent volume up to paired client").await;

    app.handle_input(b"-");
    assert_eq!(rx.try_recv().unwrap(), PairControlMessage::VolumeDown);
    wait_for_render_contains(&mut app, "Sent volume down to paired client").await;
}

#[tokio::test]
async fn c_on_dashboard_copies_selected_message() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "dashboard-copy-priority-it").await;
    let client = test_db.db.get().await.expect("db client");
    let lounge = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge room");
    ChatRoomMember::join(&client, lounge.id, user.id)
        .await
        .expect("join lounge room");
    ChatMessage::create(
        &client,
        ChatMessageParams {
            room_id: lounge.id,
            user_id: user.id,
            body: "copy me from dashboard".to_string(),
        },
    )
    .await
    .expect("create dashboard message");

    let mut app = make_app(
        test_db.db.clone(),
        user.id,
        "dashboard-copy-priority-flow-it",
    );
    wait_for_render_contains(&mut app, "copy me from dashboard").await;

    app.handle_input(b"j");
    app.handle_input(b"c");
    wait_for_render_contains(&mut app, "Message copied to clipboard!").await;
}

/// `o` on the #lounge card opens the match the live strip is showing, and
/// does nothing while no strip is up.
#[tokio::test]
async fn o_opens_the_live_strip_match_from_the_lounge_card() {
    use crate::app::activity::{event::ActivityEvent, publisher::ActivityPublisher};
    use crate::app::common::primitives::Screen;
    use crate::app::games::chips::svc::ChipService;
    use crate::app::lobby::daily::{games::DailyGame, svc::DailyService};

    let test_db = new_test_db().await;
    let me = create_test_user(&test_db.db, "strip-key-me").await;
    let them = create_test_user(&test_db.db, "strip-key-them").await;
    let client = test_db.db.get().await.expect("db client");
    let lounge = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge room");
    ChatRoomMember::join(&client, lounge.id, me.id)
        .await
        .expect("join lounge room");
    let mut app = make_app(test_db.db.clone(), me.id, "strip-key-flow-it");
    wait_for_render_contains(&mut app, "lounge").await;

    app.handle_input(b"o");
    assert_eq!(app.screen, Screen::Dashboard, "no strip, nothing to open");

    // The challenge is posted by another service over the same database;
    // claiming it through the app's own puts it in the app's snapshot.
    let (activity_tx, _activity_rx) = tokio::sync::broadcast::channel::<ActivityEvent>(8);
    let poster = DailyService::new(
        test_db.db.clone(),
        ChipService::new(test_db.db.clone()),
        ActivityPublisher::new(test_db.db.clone(), activity_tx),
    );
    let posted = poster
        .post_challenge(them.id, DailyGame::Chess)
        .await
        .expect("post");
    app.daily.claim_challenge(posted.id);
    wait_for_render_contains(&mut app, "\u{2500}\u{2500} live").await;

    app.handle_input(b"o");
    assert_eq!(app.screen, Screen::DailyMatch, "o opens the featured match");
    wait_for_render_contains(&mut app, "Daily Match").await;
}

/// A board opened from the live strip closes back to the #lounge card: the
/// viewer never opened the Lobby modal, so it stays shut and the challenge
/// they have not looked at keeps its glow.
#[tokio::test]
async fn closing_a_board_opened_from_the_live_strip_returns_to_the_lounge_card() {
    use crate::app::activity::{event::ActivityEvent, publisher::ActivityPublisher};
    use crate::app::common::primitives::Screen;
    use crate::app::games::chips::svc::ChipService;
    use crate::app::lobby::daily::{games::DailyGame, svc::DailyService};

    let test_db = new_test_db().await;
    let me = create_test_user(&test_db.db, "strip-close-me").await;
    let them = create_test_user(&test_db.db, "strip-close-them").await;
    let client = test_db.db.get().await.expect("db client");
    let lounge = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge room");
    ChatRoomMember::join(&client, lounge.id, me.id)
        .await
        .expect("join lounge room");
    let mut app = make_app(test_db.db.clone(), me.id, "strip-close-flow-it");
    wait_for_render_contains(&mut app, "lounge").await;

    let (activity_tx, _activity_rx) = tokio::sync::broadcast::channel::<ActivityEvent>(8);
    let poster = DailyService::new(
        test_db.db.clone(),
        ChipService::new(test_db.db.clone()),
        ActivityPublisher::new(test_db.db.clone(), activity_tx),
    );
    let posted = poster
        .post_challenge(them.id, DailyGame::Chess)
        .await
        .expect("post");
    // A second challenge stays open: news the viewer has not looked at.
    poster
        .post_challenge(them.id, DailyGame::Reversi)
        .await
        .expect("post the open one");
    app.daily.claim_challenge(posted.id);
    wait_for_render_contains(&mut app, "\u{2500}\u{2500} live").await;
    assert!(app.lobby.glow(), "the open challenge glows");

    app.handle_input(b"o");
    assert_eq!(app.screen, Screen::DailyMatch);
    wait_for_render_contains(&mut app, "Daily Match").await;

    app.handle_input(b"q");
    assert_eq!(app.screen, Screen::Dashboard);
    assert!(!app.show_lobby_modal, "the modal was never open");
    assert!(app.lobby.glow(), "nothing looked at the lobby");
}

const STRIP_CLICK_COLS: u16 = 160;
const STRIP_CLICK_ROWS: u16 = 40;

/// A viewer on Home with a chess match of their own on the live strip, and
/// the service and opponent to post more matches with. Returns the match on
/// the strip.
async fn app_with_a_match_on_the_strip(
    name: &str,
) -> (
    late_core::test_utils::TestDb,
    crate::app::state::App,
    crate::app::lobby::daily::svc::DailyService,
    uuid::Uuid,
    uuid::Uuid,
) {
    use crate::app::activity::{event::ActivityEvent, publisher::ActivityPublisher};
    use crate::app::games::chips::svc::ChipService;
    use crate::app::lobby::daily::{games::DailyGame, svc::DailyService};

    let test_db = new_test_db().await;
    let me = create_test_user(&test_db.db, &format!("{name}-me")).await;
    let them = create_test_user(&test_db.db, &format!("{name}-them")).await;
    let client = test_db.db.get().await.expect("db client");
    let lounge = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge room");
    ChatRoomMember::join(&client, lounge.id, me.id)
        .await
        .expect("join lounge room");
    let mut app = make_app(test_db.db.clone(), me.id, &format!("{name}-flow-it"));
    app.resize(STRIP_CLICK_COLS, STRIP_CLICK_ROWS)
        .expect("resize test terminal");
    wait_for_render_contains(&mut app, "lounge").await;

    let (activity_tx, _activity_rx) = tokio::sync::broadcast::channel::<ActivityEvent>(8);
    let poster = DailyService::new(
        test_db.db.clone(),
        ChipService::new(test_db.db.clone()),
        ActivityPublisher::new(test_db.db.clone(), activity_tx),
    );
    let posted = poster
        .post_challenge(them.id, DailyGame::Chess)
        .await
        .expect("post");
    app.daily.claim_challenge(posted.id);
    wait_for_render_contains(&mut app, "\u{2500}\u{2500} live").await;
    (test_db, app, poster, them.id, posted.id)
}

/// Click the status line's Live segment on the bottom border.
fn click_the_live_segment(app: &mut crate::app::state::App) {
    app.tick();
    app.reset_render();
    let mut terminal = vt100::Parser::new(STRIP_CLICK_ROWS, STRIP_CLICK_COLS, 0);
    terminal.process(&app.render().expect("render"));
    let screen = terminal.screen().contents();
    let bottom_row = screen.lines().last().expect("bottom border row");
    let byte = bottom_row.find("live ").expect("the live segment");
    let live_col = unicode_width::UnicodeWidthStr::width(&bottom_row[..byte]);
    // SGR mouse coords are 1-indexed.
    app.handle_input(format!("\x1b[<0;{};{STRIP_CLICK_ROWS}M", live_col + 1).as_bytes());
}

/// The status line's Live segment is on every page, the board of the match
/// it names included. Clicked there it has nothing to open: the board stays
/// as the player left it, cursor and all, and still closes to the page it
/// was opened from.
#[tokio::test]
async fn clicking_the_live_segment_on_its_own_board_leaves_the_board_alone() {
    use crate::app::common::primitives::Screen;

    let (_test_db, mut app, _poster, _them, _on_strip) =
        app_with_a_match_on_the_strip("strip-own").await;
    app.handle_input(b"o");
    assert_eq!(app.screen, Screen::DailyMatch);
    wait_for_render_contains(&mut app, "Daily Match").await;

    let cursor = |app: &crate::app::state::App| app.daily.board.as_ref().expect("a board").cursor;
    let start = cursor(&app);
    app.handle_input(b"d");
    let moved = cursor(&app);
    assert_ne!(moved, start, "d moves the cursor");

    click_the_live_segment(&mut app);
    assert_eq!(app.screen, Screen::DailyMatch);
    assert_eq!(cursor(&app), moved, "the board was not opened again");

    app.handle_input(b"q");
    assert_eq!(app.screen, Screen::Dashboard);
}

/// Clicked on the board of another match, the Live segment swaps in the
/// match the strip shows and keeps the first board's way out: closing lands
/// on the page that board was opened from, never on a board page with no
/// board.
#[tokio::test]
async fn clicking_the_live_segment_on_another_board_keeps_that_boards_way_out() {
    use crate::app::common::primitives::Screen;
    use crate::app::lobby::daily::{games::DailyGame, state::BoardEntry};

    let (_test_db, mut app, poster, them, on_strip) =
        app_with_a_match_on_the_strip("strip-other").await;
    let other = poster
        .post_challenge(them, DailyGame::Reversi)
        .await
        .expect("post the other match");
    app.daily.claim_challenge(other.id);
    wait_for_app(&mut app, "the other match to start", |app| {
        app.daily.live_item(other.id).is_some()
    })
    .await;

    // The other match's board, opened from Home the way the Lobby opens it.
    // The strip still holds the first match for its minute.
    let item = app.daily.live_item(other.id).expect("the other match");
    app.daily
        .open_board(&item, Screen::Dashboard, BoardEntry::Lobby);
    app.set_screen(Screen::DailyMatch);
    wait_for_render_contains(&mut app, "Daily Match").await;

    click_the_live_segment(&mut app);
    assert_eq!(app.screen, Screen::DailyMatch);
    assert_eq!(
        app.daily.board.as_ref().map(|board| board.match_id),
        Some(on_strip),
        "the click opens the match on the strip"
    );

    app.handle_input(b"q");
    assert_eq!(
        app.screen,
        Screen::Dashboard,
        "closing hands back the page the first board was opened from"
    );
}

/// Ctrl+F on a board goes to Zen and closes the board, so the chord back
/// hands over the page the board was opened from, never an empty board. A
/// board opened from Zen's Live tile hands back Zen's own page.
#[tokio::test]
async fn ctrl_f_twice_on_a_board_lands_where_the_board_was_opened_from() {
    use crate::app::activity::{event::ActivityEvent, publisher::ActivityPublisher};
    use crate::app::common::primitives::Screen;
    use crate::app::games::chips::svc::ChipService;
    use crate::app::lobby::daily::{games::DailyGame, svc::DailyService};
    use crate::app::zen::state::{KindPick, TileKind};

    let test_db = new_test_db().await;
    let me = create_test_user(&test_db.db, "strip-zen-me").await;
    let them = create_test_user(&test_db.db, "strip-zen-them").await;
    let client = test_db.db.get().await.expect("db client");
    let lounge = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge room");
    ChatRoomMember::join(&client, lounge.id, me.id)
        .await
        .expect("join lounge room");
    let mut app = make_app(test_db.db.clone(), me.id, "strip-zen-flow-it");
    app.resize(160, 40).expect("resize test terminal");
    wait_for_render_contains(&mut app, "lounge").await;

    let (activity_tx, _activity_rx) = tokio::sync::broadcast::channel::<ActivityEvent>(8);
    let poster = DailyService::new(
        test_db.db.clone(),
        ChipService::new(test_db.db.clone()),
        ActivityPublisher::new(test_db.db.clone(), activity_tx),
    );
    let posted = poster
        .post_challenge(them.id, DailyGame::Chess)
        .await
        .expect("post");
    app.daily.claim_challenge(posted.id);
    wait_for_render_contains(&mut app, "\u{2500}\u{2500} live").await;

    // Opened from Home: Ctrl+F twice comes back to Home.
    app.handle_input(b"o");
    assert_eq!(app.screen, Screen::DailyMatch);
    app.handle_input(b"\x06");
    assert_eq!(app.screen, Screen::Zen, "Ctrl+F on the board opens Zen");
    app.handle_input(b"\x06");
    assert_eq!(
        app.screen,
        Screen::Dashboard,
        "the board was opened from Home"
    );

    // Opened from Zen's Live tile: Ctrl+F twice comes back to Home, the
    // page Zen was opened over.
    app.handle_input(b"\x06");
    assert_eq!(app.screen, Screen::Zen);
    app.zen.focus = app
        .zen
        .first_tile_of(TileKind::Lobby)
        .expect("the default has a lobby");
    app.zen.open_kind_picker();
    while app.zen.kind_picker_selection() != Some(TileKind::Live) {
        app.zen.move_kind_picker(1);
    }
    assert_eq!(app.zen.pick_kind(), KindPick::Changed);
    wait_for_render_contains(&mut app, "o open").await;
    app.handle_input(b"\r");
    assert_eq!(app.screen, Screen::DailyMatch, "Enter opens the match");
    app.handle_input(b"\x06");
    assert_eq!(app.screen, Screen::Zen, "Ctrl+F on the board opens Zen");
    app.handle_input(b"\x06");
    assert_eq!(app.screen, Screen::Dashboard, "Zen was opened over Home");
}

/// A track somebody queued in the booth goes up on the live strip, and `o`
/// tunes a viewer on another source in to YouTube; once there, `o` opens
/// the booth.
#[tokio::test]
async fn o_on_a_booth_track_tunes_in_then_opens_the_booth() {
    use crate::app::audio::youtube::YoutubeVideo;
    use late_core::models::user::AudioSource;

    let test_db = new_test_db().await;
    let me = create_test_user(&test_db.db, "strip-booth-me").await;
    let them = create_test_user(&test_db.db, "strip-booth-them").await;
    let client = test_db.db.get().await.expect("db client");
    let lounge = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge room");
    ChatRoomMember::join(&client, lounge.id, me.id)
        .await
        .expect("join lounge room");
    let mut app = make_app(test_db.db.clone(), me.id, "strip-booth-flow-it");
    app.resize(160, 40)
        .expect("resize to a card the full strip fits");
    wait_for_render_contains(&mut app, "lounge").await;
    app.set_paired_playback_source(AudioSource::Radio);

    app.audio
        .service()
        .submit_validated_video(
            them.id,
            YoutubeVideo {
                video_id: "ggggggggggg".to_string(),
                title: Some("Blue in Green".to_string()),
                channel: Some("Late Night Tapes".to_string()),
                duration_ms: Some(225_000),
                is_stream: false,
            },
        )
        .await
        .expect("queue a track");
    wait_for_render_contains(&mut app, "Late Night Tapes \u{b7} 3:45").await;
    wait_for_render_contains(&mut app, "o or click to tune in").await;

    app.handle_input(b"o");
    assert_eq!(app.paired_source, AudioSource::Youtube, "o tunes in");
    assert!(!app.booth_modal_state.is_open());
    wait_for_render_contains(&mut app, "o or click for the booth").await;

    app.handle_input(b"o");
    assert!(app.booth_modal_state.is_open(), "then o opens the booth");
}

/// A track that left the booth between the strip's last tick and the key
/// has nothing to tune in to: `o` leaves the viewer's audio source alone.
#[tokio::test]
async fn o_on_a_booth_track_that_left_the_booth_changes_nothing() {
    use crate::app::audio::youtube::YoutubeVideo;
    use late_core::models::user::AudioSource;

    let test_db = new_test_db().await;
    let me = create_test_user(&test_db.db, "strip-gone-me").await;
    let them = create_test_user(&test_db.db, "strip-gone-them").await;
    let client = test_db.db.get().await.expect("db client");
    let lounge = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge room");
    ChatRoomMember::join(&client, lounge.id, me.id)
        .await
        .expect("join lounge room");
    let mut app = make_app(test_db.db.clone(), me.id, "strip-gone-flow-it");
    app.resize(160, 40)
        .expect("resize to a card the full strip fits");
    wait_for_render_contains(&mut app, "lounge").await;
    app.set_paired_playback_source(AudioSource::Radio);

    app.audio
        .service()
        .submit_validated_video(
            them.id,
            YoutubeVideo {
                video_id: "hhhhhhhhhhh".to_string(),
                title: Some("Naima".to_string()),
                channel: Some("Late Night Tapes".to_string()),
                duration_ms: Some(225_000),
                is_stream: false,
            },
        )
        .await
        .expect("queue a track");
    wait_for_render_contains(&mut app, "o or click to tune in").await;

    // The track is skipped, and the key lands before the next tick.
    app.audio
        .service()
        .force_skip()
        .await
        .expect("skip the track");
    let snapshot = app.audio.queue_snapshot();
    assert!(
        snapshot.current.is_none() && snapshot.queue.is_empty(),
        "the track left the booth"
    );

    app.handle_input(b"o");
    assert_eq!(
        app.paired_source,
        AudioSource::Radio,
        "nothing to tune in to"
    );
    assert!(!app.booth_modal_state.is_open());
}

/// A link somebody shared to News goes up on the live strip, and `o` opens
/// the article modal.
#[tokio::test]
async fn o_on_a_shared_article_opens_the_article_modal() {
    use late_core::models::article::{Article, ArticleParams};

    let test_db = new_test_db().await;
    let me = create_test_user(&test_db.db, "strip-news-me").await;
    let them = create_test_user(&test_db.db, "strip-news-them").await;
    let client = test_db.db.get().await.expect("db client");
    let lounge = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge room");
    ChatRoomMember::join(&client, lounge.id, me.id)
        .await
        .expect("join lounge room");
    Article::create_by_user_id(
        &client,
        them.id,
        ArticleParams {
            user_id: them.id,
            url: "https://example.com/terminal-renaissance".to_string(),
            title: "The terminal renaissance".to_string(),
            summary: "• terminals are back".to_string(),
            ascii_art: "############\n#  late.sh #\n############".to_string(),
        },
    )
    .await
    .expect("share an article");
    let mut app = make_app(test_db.db.clone(), me.id, "strip-news-flow-it");
    app.resize(160, 40)
        .expect("resize to a card the full strip fits");
    wait_for_render_contains(&mut app, "strip-news-them shared it").await;
    wait_for_render_contains(&mut app, "o read \u{b7} r reply").await;

    app.handle_input(b"o");
    assert_eq!(
        app.chat.news_modal_url(),
        Some("https://example.com/terminal-renaissance"),
        "o opens the article"
    );
}

/// Shares no longer post into #lounge, so the strip is where a link is
/// answered: `r` opens the lounge composer replying to the article, and the
/// sent message quotes its title.
#[tokio::test]
async fn r_on_a_shared_article_replies_with_its_title_quoted() {
    use late_core::models::article::{Article, ArticleParams};

    let test_db = new_test_db().await;
    let me = create_test_user(&test_db.db, "strip-reply-me").await;
    let them = create_test_user(&test_db.db, "strip-reply-them").await;
    let client = test_db.db.get().await.expect("db client");
    let lounge = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge room");
    ChatRoomMember::join(&client, lounge.id, me.id)
        .await
        .expect("join lounge room");
    Article::create_by_user_id(
        &client,
        them.id,
        ArticleParams {
            user_id: them.id,
            url: "https://example.com/terminal-renaissance".to_string(),
            title: "The terminal renaissance".to_string(),
            summary: "• terminals are back".to_string(),
            ascii_art: "############\n#  late.sh #\n############".to_string(),
        },
    )
    .await
    .expect("share an article");
    let mut app = make_app(test_db.db.clone(), me.id, "strip-reply-flow-it");
    app.resize(160, 40)
        .expect("resize to a card the full strip fits");
    wait_for_render_contains(&mut app, "o read \u{b7} r reply").await;

    app.handle_input(b"r");
    assert!(app.chat.is_composing(), "r opens the lounge composer");
    app.handle_input(b"worth a read\r");
    wait_for_render_contains(&mut app, "worth a read").await;

    let sent = ChatMessage::list_recent(&client, lounge.id, 1)
        .await
        .expect("list lounge");
    assert_eq!(
        sent.iter()
            .map(|message| (message.body.as_str(), message.reply_to_message_id))
            .collect::<Vec<_>>(),
        vec![(
            "> @strip-reply-them: 📰 The terminal renaissance\nworth a read",
            None
        )],
        "the reply quotes the article, with no message to point at"
    );
}

/// The same reply from the News room: `r` on the selected story takes you
/// to #lounge with the composer replying to it, quoting its title.
#[tokio::test]
async fn r_on_a_news_story_replies_in_lounge_with_its_title_quoted() {
    use crate::app::chat::state::RoomSlot;
    use late_core::models::article::{Article, ArticleParams};

    let test_db = new_test_db().await;
    let me = create_test_user(&test_db.db, "news-reply-me").await;
    let them = create_test_user(&test_db.db, "news-reply-them").await;
    let client = test_db.db.get().await.expect("db client");
    let lounge = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge room");
    ChatRoomMember::join(&client, lounge.id, me.id)
        .await
        .expect("join lounge room");
    Article::create_by_user_id(
        &client,
        them.id,
        ArticleParams {
            user_id: them.id,
            url: "https://example.com/terminal-renaissance".to_string(),
            title: "The terminal renaissance".to_string(),
            summary: "• terminals are back".to_string(),
            ascii_art: "############\n#  late.sh #\n############".to_string(),
        },
    )
    .await
    .expect("share an article");
    let mut app = make_app(test_db.db.clone(), me.id, "news-reply-flow-it");
    app.resize(160, 40).expect("resize");
    wait_for_render_contains(&mut app, "o read \u{b7} r reply").await;
    app.chat.select_room_slot(RoomSlot::News);
    app.sync_visible_chat_room();
    wait_for_render_contains(&mut app, "r reply in #lounge").await;

    app.handle_input(b"r");
    assert_eq!(
        (
            app.chat.news_selected,
            app.chat.selected_room_id,
            app.chat.is_composing()
        ),
        (false, Some(lounge.id), true),
        "r leaves News for #lounge with the composer open"
    );
    app.handle_input(b"worth a read\r");
    wait_for_render_contains(&mut app, "worth a read").await;

    let sent = ChatMessage::list_recent(&client, lounge.id, 1)
        .await
        .expect("list lounge");
    assert_eq!(
        sent.iter()
            .map(|message| (message.body.as_str(), message.reply_to_message_id))
            .collect::<Vec<_>>(),
        vec![(
            "> @news-reply-them: 📰 The terminal renaissance\nworth a read",
            None
        )],
        "the reply quotes the article, with no message to point at"
    );
}

/// Somebody starts a DCSS game: it goes up on the live strip, `o` opens the
/// watch on it (the game across the Games page, its chat beside it), and the
/// watch is a stop on the backtick cycle, so `` ` `` toggles between it and
/// Home. Reaching the Games page by its number shows the hub's cards, and
/// the watch is still on the cycle. No render runs between the keys: the
/// test door host is unreachable, so a tick would see the stream end and
/// drop the watch.
#[tokio::test]
async fn o_on_a_live_door_game_opens_the_watch_and_backtick_toggles_it_with_home() {
    use crate::app::common::primitives::Screen;
    use crate::app::door::spectate::{
        proxy::LiveGame,
        state::{SpectateGame, WatchMode},
    };

    let test_db = new_test_db().await;
    let me = create_test_user(&test_db.db, "strip-door-me").await;
    let client = test_db.db.get().await.expect("db client");
    let lounge = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge room");
    ChatRoomMember::join(&client, lounge.id, me.id)
        .await
        .expect("join lounge room");
    let mut app = make_app(test_db.db.clone(), me.id, "strip-door-flow-it");
    app.resize(160, 40)
        .expect("resize to a card the full strip fits");
    let started_unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("wall clock after the unix epoch")
        .as_secs()
        - 30;
    app.live_games.publish_roster_for_tests(
        SpectateGame::Dcss,
        vec![LiveGame {
            playname: "crawler".to_string(),
            started_unix,
            watchers: 0,
            status: "XL3 Lair:2".to_string(),
        }],
    );
    wait_for_render_contains(&mut app, "crawler is playing").await;
    wait_for_render_contains(&mut app, "o or click to watch").await;

    app.handle_input(b"o");
    let watch = |app: &crate::app::state::App| {
        app.spectate_state
            .as_ref()
            .map(|state| (state.game(), state.playname().to_string(), state.mode()))
    };
    assert_eq!(app.screen, Screen::Games, "o opens the Games page");
    assert_eq!(
        watch(&app),
        Some((SpectateGame::Dcss, "crawler".to_string(), WatchMode::Open)),
        "on the open watch of that game"
    );

    app.handle_input(b"`");
    assert_eq!(app.screen, Screen::Dashboard, "backtick hops home");

    app.handle_input(b"`");
    assert_eq!(app.screen, Screen::Games, "and backtick hops back in");
    assert_eq!(watch(&app).map(|(_, _, mode)| mode), Some(WatchMode::Open));

    app.handle_input(b"1");
    app.handle_input(b"3");
    assert_eq!(app.screen, Screen::Games, "3 is the Games page");
    assert_eq!(
        watch(&app),
        None,
        "its cards, with no watch drawn over them"
    );

    app.handle_input(b"1");
    app.handle_input(b"`");
    assert_eq!(app.screen, Screen::Games, "the watch is still on the cycle");
    assert_eq!(
        watch(&app),
        Some((SpectateGame::Dcss, "crawler".to_string(), WatchMode::Open)),
        "the same open watch"
    );
}

/// Two watches kept at once: open one from the hub's rail, go Home, come back
/// with `3` (the cards, no watch), open another, and the backtick cycles
/// Home, the first, the second, Home. No render runs between the keys: the
/// test door hosts are unreachable, so a tick would see the streams end.
#[tokio::test]
async fn every_watch_opened_stays_a_stop_on_the_backtick_cycle() {
    use crate::app::common::primitives::Screen;
    use crate::app::door::spectate::{
        proxy::LiveGame,
        state::{SpectateGame, WatchMode},
    };

    let test_db = new_test_db().await;
    let me = create_test_user(&test_db.db, "two-watches-me").await;
    let mut app = make_app(test_db.db.clone(), me.id, "two-watches-flow-it");
    app.resize(160, 40).expect("resize");
    let live = |playname: &str| LiveGame {
        playname: playname.to_string(),
        started_unix: 1_790_000_000,
        watchers: 0,
        status: String::new(),
    };
    app.live_games
        .publish_roster_for_tests(SpectateGame::Dcss, vec![live("crawler")]);
    app.live_games
        .publish_roster_for_tests(SpectateGame::Nethack, vec![live("digger")]);
    let watch = |app: &crate::app::state::App| {
        app.spectate_state
            .as_ref()
            .map(|state| (state.playname().to_string(), state.mode()))
    };
    let open = |playname: &str| Some((playname.to_string(), WatchMode::Open));

    // Up from the top card wraps to the rail's last live row (digger), then
    // the one above it (crawler).
    app.handle_input(b"3");
    app.handle_input(b"k");
    app.handle_input(b"k");
    app.handle_input(b"\r");
    assert_eq!(watch(&app), open("crawler"), "the first watch is open");

    app.handle_input(b"1");
    app.handle_input(b"3");
    assert_eq!(app.screen, Screen::Games);
    assert_eq!(watch(&app), None, "3 shows the cards");
    app.handle_input(b"k");
    app.handle_input(b"\r");
    assert_eq!(watch(&app), open("digger"), "the second watch is open");

    app.handle_input(b"`");
    assert_eq!(app.screen, Screen::Dashboard, "the last watch hops home");
    app.handle_input(b"`");
    assert_eq!(
        (app.screen, watch(&app)),
        (Screen::Games, open("crawler")),
        "then the first watch"
    );
    app.handle_input(b"`");
    assert_eq!(
        (app.screen, watch(&app)),
        (Screen::Games, open("digger")),
        "then the second"
    );
    app.handle_input(b"`");
    assert_eq!(app.screen, Screen::Dashboard, "and home again");
}

/// A #lounge draft belongs to #lounge. Clicking a live door game on the strip
/// while one is half typed opens the watch without it: the watch chat's
/// composer is the watch room's alone. Carried along, the draft would draw
/// under the watch's messages while Enter still sent it to #lounge.
#[tokio::test]
async fn opening_a_watch_from_the_strip_drops_a_lounge_draft() {
    use crate::app::common::primitives::Screen;
    use crate::app::door::spectate::{proxy::LiveGame, state::SpectateGame};

    let test_db = new_test_db().await;
    let me = create_test_user(&test_db.db, "strip-draft-me").await;
    let client = test_db.db.get().await.expect("db client");
    let lounge = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge room");
    ChatRoomMember::join(&client, lounge.id, me.id)
        .await
        .expect("join lounge room");
    let mut app = make_app(test_db.db.clone(), me.id, "strip-draft-flow-it");
    app.resize(160, 40)
        .expect("resize to a card the full strip fits");
    let started_unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("wall clock after the unix epoch")
        .as_secs()
        - 30;
    app.live_games.publish_roster_for_tests(
        SpectateGame::Dcss,
        vec![LiveGame {
            playname: "crawler".to_string(),
            started_unix,
            watchers: 0,
            status: String::new(),
        }],
    );
    wait_for_render_contains(&mut app, "o or click to watch").await;

    app.handle_input(b"i");
    app.handle_input(b"nice sling");
    assert!(app.chat.is_composing(), "i opens the lounge composer");
    assert_eq!(app.chat.composer_room_id(), Some(lounge.id));

    // The frame records where the strip is; the click lands on it. No
    // render runs after the click: the test door host is unreachable, so a
    // tick would see the stream end and drop the watch.
    render_plain(&mut app);
    let (strip, _) = app.live.hit.get().expect("the strip is on the card");
    app.handle_input(format!("\x1b[<0;{};{}M", strip.x + 1, strip.y + 1).as_bytes());
    assert_eq!(app.screen, Screen::Games, "the click opens the watch");
    assert!(
        app.spectate_state
            .as_ref()
            .is_some_and(|state| state.is_open()),
        "on the open watch"
    );
    assert!(
        !app.chat.is_composing(),
        "the lounge draft does not come along into the watch"
    );
    assert_eq!(app.chat.composer_room_id(), None);
}
