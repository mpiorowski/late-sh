use chrono::{DateTime, TimeZone, Utc};

use super::*;
use crate::app::audio::svc::AudioMode;

const WIDTH: u16 = 80;

fn line_text(line: &Line<'_>) -> String {
    line.spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect()
}

fn track(n: u128, title: &str, queued_secs: i64) -> QueueItemView {
    QueueItemView {
        id: Uuid::from_u128(n),
        video_id: format!("video{n:06}"),
        title: Some(title.to_string()),
        channel: Some("Late Night Tapes".to_string()),
        duration_ms: Some(225_000),
        started_at_ms: None,
        is_stream: false,
        submitter: "mat".to_string(),
        submitter_id: Uuid::from_u128(99),
        vote_score: 0,
        unskippable: false,
        queued_at: Utc.with_ymd_and_hms(2026, 9, 29, 21, 0, 0).unwrap()
            + chrono::Duration::seconds(queued_secs),
        thumbnail: None,
    }
}

fn booth() -> QueueSnapshot {
    QueueSnapshot {
        audio_mode: AudioMode::Youtube,
        current: Some(track(1, "Blue in Green", 0)),
        queue: vec![track(2, "Naima", 30), track(3, "So What", 60)],
        history: Vec::new(),
        skip_progress: None,
    }
}

fn words(track: &TrackStripView) -> Vec<String> {
    body(usize::from(WIDTH), track)
        .words
        .iter()
        .map(|spans| spans.iter().map(|span| span.content.as_ref()).collect())
        .collect()
}

#[test]
fn every_track_in_the_booth_is_news_from_when_it_was_queued() {
    let snapshot = booth();
    let stamps: Vec<(LiveSource, DateTime<Utc>)> = candidates(&snapshot)
        .iter()
        .map(|candidate| (candidate.source, candidate.updated))
        .collect();
    let queued = |n: u128, secs: i64| {
        (
            LiveSource::BoothTrack(Uuid::from_u128(n)),
            track(n, "", secs).queued_at,
        )
    };
    assert_eq!(stamps, vec![queued(1, 0), queued(2, 30), queued(3, 60)]);
}

#[test]
fn the_words_say_what_the_track_is_who_brought_it_and_where_it_stands() {
    let snapshot = booth();
    let view_of = |n: u128, source| view(&snapshot, Uuid::from_u128(n), source, None).unwrap();

    let playing = view_of(1, AudioSource::Icecast);
    assert_eq!(
        words(&playing),
        vec![
            "",
            "Blue in Green",
            "Late Night Tapes · 3:45",
            "mat put it on · playing now",
            "",
            "",
            "o or click to tune in",
            "",
        ]
    );
    assert!(glow(&playing), "the track playing lights the label");

    let next = view_of(2, AudioSource::Radio);
    assert_eq!(words(&next)[3], "mat queued it · up next");
    assert!(!glow(&next));

    let waiting = view_of(3, AudioSource::Youtube);
    assert_eq!(words(&waiting)[3], "mat queued it · #2 in line");
    assert_eq!(
        words(&waiting)[6],
        "o or click for the booth",
        "a viewer already on YouTube has nothing to tune in to"
    );

    assert!(
        view(&snapshot, Uuid::from_u128(4), AudioSource::Youtube, None).is_none(),
        "a track that left the booth paints nothing"
    );
}

#[test]
fn the_picture_is_the_thumbnail_once_it_has_loaded() {
    let snapshot = booth();
    let mut track = view(&snapshot, Uuid::from_u128(1), AudioSource::Icecast, None).unwrap();

    let drawn: Vec<String> = body(usize::from(WIDTH), &track)
        .picture
        .iter()
        .map(line_text)
        .collect();
    assert_eq!(
        drawn,
        vec![
            "╭───────────────────╮",
            "│                   │",
            "│                   │",
            "│         ▶         │",
            "│                   │",
            "╰───────────────────╯",
        ]
    );

    // Red on the left half, blue on the right.
    let thumbnail = RgbaImage::from_fn(168, 96, |x, _| {
        if x < 84 {
            image::Rgba([200, 40, 40, 255])
        } else {
            image::Rgba([40, 40, 200, 255])
        }
    });
    track.picture = Some(
        render_picture(&thumbnail, InlineImageRenderSettings::default())
            .expect("a thumbnail renders"),
    );
    let painted = body(usize::from(WIDTH), &track).picture;
    assert_eq!(painted.len(), 6, "16:9 across the picture column");
    // What a cell of flat colour shows: its background, or its foreground
    // when it is drawn reversed.
    let shown = |span: &Span<'_>| {
        let colour = if span.style.add_modifier.contains(Modifier::REVERSED) {
            span.style.fg
        } else {
            span.style.bg
        };
        match colour {
            Some(ratatui::style::Color::Rgb(r, _, b)) => (r, b),
            other => panic!("a painted cell carries a colour, got {other:?}"),
        }
    };
    for line in &painted {
        assert_eq!(line.width(), usize::from(PICTURE_COLS));
        for (col, span) in line.spans.iter().enumerate() {
            let (red, blue) = shown(span);
            match col {
                0..=9 => assert!(red > 150 && blue < 80, "column {col} is red: {span:?}"),
                11..=20 => assert!(blue > 150 && red < 80, "column {col} is blue: {span:?}"),
                // The seam falls inside column 10.
                _ => {}
            }
        }
    }
}

#[test]
fn the_compact_line_names_who_queued_what() {
    let snapshot = booth();
    let track = view(&snapshot, Uuid::from_u128(2), AudioSource::Icecast, None).unwrap();
    let text: String = compact_spans(40, &track)
        .iter()
        .map(|span| span.content.as_ref())
        .collect();
    assert_eq!(text, "booth mat · Naima");
}
