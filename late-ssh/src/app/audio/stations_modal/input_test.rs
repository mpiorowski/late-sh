use crate::{app::audio::radio_meta::svc::ArtistTitle, test_helpers::make_app};
use late_core::{
    models::user::RadioStation,
    test_utils::{create_test_user, test_db},
};
use std::collections::HashMap;
use tokio::sync::watch;

#[tokio::test]
async fn ctrl_y_copies_the_highlighted_station_track() {
    let test = test_db().await;
    let user = create_test_user(&test.db, "stations-copy").await;
    let mut app = make_app(test.db.clone(), user.id, "stations-copy");
    let (_meta_tx, meta_rx) = watch::channel(HashMap::from([(
        "datawave".to_string(),
        ArtistTitle {
            artist: "Com Truise".to_string(),
            title: "Flightwave".to_string(),
        },
    )]));
    app.radio_meta_rx = Some(meta_rx);
    app.stations_modal_state
        .open(RadioStation::from_key("datawave").unwrap());

    app.handle_input(b"\x19");

    assert_eq!(
        app.pending_clipboard.as_deref(),
        Some("Com Truise - Flightwave")
    );
    assert!(app.stations_modal_state.is_open());
}

#[tokio::test]
async fn ctrl_y_on_a_station_without_metadata_preserves_the_clipboard() {
    let test = test_db().await;
    let user = create_test_user(&test.db, "stations-copy-empty").await;
    let mut app = make_app(test.db.clone(), user.id, "stations-copy-empty");
    app.pending_clipboard = Some("Earlier copy".to_string());
    app.stations_modal_state
        .open(RadioStation::from_key("classical").unwrap());

    app.handle_input(b"\x19");

    assert_eq!(app.pending_clipboard.as_deref(), Some("Earlier copy"));
    assert_eq!(
        app.banner.as_ref().map(|banner| banner.message.as_str()),
        Some("No track info for Classical")
    );
}
