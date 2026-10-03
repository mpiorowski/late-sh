use super::*;
use late_core::radio::CATALOGUE;

#[test]
fn plaza_status_yields_the_current_song() {
    let body = r#"{"song":{"id":"bd35","artist":"GOLDIE GOLDIE","album":"PHANTASY","title":"Perfect","length":201,"position":43},"listeners":139,"updated_at":1791022399}"#;
    assert_eq!(
        PolledFeed::Plaza.parse(body).unwrap(),
        ArtistTitle {
            artist: "GOLDIE GOLDIE".to_string(),
            title: "Perfect".to_string(),
        }
    );
}

#[test]
fn code_radio_now_playing_yields_the_current_song() {
    let body = r#"{"station":{"id":1,"name":"freeCodeCamp.org Code Radio"},"now_playing":{"sh_id":395798,"duration":208,"song":{"id":"b9e5","text":"City Girl - Palette","artist":"City Girl","title":"Palette","album":"Neon Impasse"},"elapsed":156,"remaining":52}}"#;
    assert_eq!(
        PolledFeed::CodeRadio.parse(body).unwrap(),
        ArtistTitle {
            artist: "City Girl".to_string(),
            title: "Palette".to_string(),
        }
    );
}

#[test]
fn radio_paradise_now_playing_yields_the_current_song() {
    let body = r#"{"time":64,"artist":"Patrick Watson","title":"A Mermaid in Lisbon","album":"Better in the Shade","year":"2022","cover":"https:\/\/img.radioparadise.com\/covers\/l\/1.jpg"}"#;
    assert_eq!(
        PolledFeed::ParadiseMellow.parse(body).unwrap(),
        ArtistTitle {
            artist: "Patrick Watson".to_string(),
            title: "A Mermaid in Lisbon".to_string(),
        }
    );
}

#[test]
fn fip_live_metadata_yields_the_current_song() {
    let body = r#"{"prev":[],"now":{"firstLine":"Invisible Thread","firstLineSongUuid":"5cc1","secondLine":"Shai Maestro Trio","secondLineSongUuid":"5cc1","thirdLine":"Jazz","songUuid":"5cc1","cover":"04fe","startTime":1791026761,"endTime":1791027040},"next":[],"delayToRefresh":120000}"#;
    assert_eq!(
        PolledFeed::FipJazz.parse(body).unwrap(),
        ArtistTitle {
            artist: "Shai Maestro Trio".to_string(),
            title: "Invisible Thread".to_string(),
        }
    );
}

/// Between songs the same fields carry the programme name and its blurb;
/// that must not be shown as an artist credit.
#[test]
fn fip_programme_metadata_is_not_a_song() {
    let body = r#"{"now":{"firstLine":"Le direct","secondLine":"Bebop, afro jazz, salsa, bossa, blues","thirdLine":"Jazz","songUuid":null,"cover":"2de5","startTime":null,"endTime":null}}"#;
    assert!(PolledFeed::FipJazz.parse(body).is_err());
}

#[test]
fn radio_swiss_current_yields_the_current_song() {
    let body = r#"{"channel":{"id":"0191","name":"SSATR SwissJazz","playingnow":{"current":{"time":"2026-10-03T11:38:33Z","duration":193990,"metadata":{"id":"101214009","album":"Sueños Latinos","artist":"Trio Angeluci","duration":"00:03:14.000","colId":null,"title":"Mimoso","composer":null}},"next":[{"time":"2026-10-03T11:41:47Z"}]}}}"#;
    for feed in [PolledFeed::SwissJazz, PolledFeed::SwissClassic] {
        assert_eq!(
            feed.parse(body).unwrap(),
            ArtistTitle {
                artist: "Trio Angeluci".to_string(),
                title: "Mimoso".to_string(),
            }
        );
    }
}

/// Code Radio plays tracks tagged with a title only; that is still a track,
/// not a dead feed.
#[test]
fn a_track_without_an_artist_keeps_its_title() {
    let body = r#"{"now_playing":{"song":{"text":" - night 7","artist":"","title":"night 7","album":"25 Nights for Nujabes"}}}"#;
    assert_eq!(
        PolledFeed::CodeRadio.parse(body).unwrap(),
        ArtistTitle {
            artist: String::new(),
            title: "night 7".to_string(),
        }
    );
}

#[test]
fn a_payload_without_a_title_is_an_error() {
    assert!(
        PolledFeed::CodeRadio
            .parse(r#"{"now_playing":{"song":{"artist":"City Girl"}}}"#)
            .is_err()
    );
    // One feed's shape is not the other's.
    assert!(
        PolledFeed::CodeRadio
            .parse(r#"{"song":{"artist":"A","title":"B"}}"#)
            .is_err()
    );
    assert!(PolledFeed::Plaza.parse("<html>502</html>").is_err());
}

/// The map key a feed writes must be a catalogue row, or the rail and the
/// listen page would never find its track.
#[test]
fn every_feed_writes_under_a_catalogue_key() {
    for feed in PolledFeed::ALL {
        assert!(
            CATALOGUE
                .iter()
                .any(|station| station.key == feed.station_key()),
            "{feed:?}"
        );
    }
}
