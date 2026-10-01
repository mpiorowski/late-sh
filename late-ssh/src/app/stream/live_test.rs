use chrono::{DateTime, TimeZone, Utc};

use super::*;

fn went_live() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 1, 21, 0, 0).unwrap()
}

fn stream(
    n: u128,
    username: &str,
    title: &str,
    went_live_at: Option<DateTime<Utc>>,
) -> LiveStreamView {
    LiveStreamView {
        user_id: Uuid::from_u128(n),
        username: username.to_string(),
        title: title.to_string(),
        room_id: Uuid::from_u128(n + 100),
        voice_channel_id: Uuid::from_u128(n + 200),
        stream_id: format!("stream{n}"),
        live: went_live_at.is_some(),
        went_live_at,
        watching: 3,
        watch_url: String::new(),
    }
}

fn text(spans: &[Span<'_>]) -> String {
    spans.iter().map(|span| span.content.as_ref()).collect()
}

/// A stream joins the queue when its media first flows, stamped with that
/// moment; a pending one is never offered, so the strip never points at a
/// black screen.
#[test]
fn only_a_stream_that_went_live_is_offered_stamped_with_when() {
    let streams = [
        stream(1, "mat", "late night rust", Some(went_live())),
        stream(2, "eggy", "setting up", None),
    ];

    let offered = candidates(&streams);

    assert_eq!(offered.len(), 1);
    assert_eq!(offered[0].source, LiveSource::Stream(Uuid::from_u128(1)));
    assert_eq!(offered[0].updated, went_live());
    assert!(view(&streams, Uuid::from_u128(2)).is_none());
}

#[test]
fn the_body_names_the_stream_the_streamer_and_the_key() {
    let streams = [stream(1, "mat", "late night rust", Some(went_live()))];
    let strip = view(&streams, Uuid::from_u128(1)).expect("a live stream has a view");

    let body = body(60, &strip);
    let words: Vec<String> = body.words.iter().map(|spans| text(spans)).collect();

    assert_eq!(words[1], "late night rust");
    assert_eq!(words[2], "3 watching");
    assert_eq!(words[3], "mat went live");
    assert_eq!(text(&body.hint), "o or click to watch");
    assert!(body.glow);
    assert_eq!(
        text(&compact_spans(60, &strip)),
        "stream mat · late night rust"
    );
}
