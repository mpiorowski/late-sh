use late_core::models::deadchannel_spawn::{Spawn, SpawnHit, SpawnKind};
use uuid::Uuid;

use super::{LoungeLine, line, placeholder};

fn howler(hits: Vec<SpawnHit>) -> Spawn {
    Spawn {
        id: Uuid::now_v7(),
        kind: SpawnKind::Howler,
        hits_needed: 3,
        hits,
        spawned_at_ms: 1_000,
        expires_at_ms: 181_000,
        ended_at_ms: None,
        outcome: None,
    }
}

fn text_of(line: &ratatui::text::Line<'_>) -> String {
    line.spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect()
}

#[test]
fn the_line_is_the_spawn_s_unless_the_session_has_a_word() {
    let viewer = Uuid::now_v7();
    let spawn = howler(Vec::new());
    assert_eq!(
        line(&spawn, None, viewer),
        LoungeLine {
            glyph: '╫',
            text: "a howler is in the channel. 3 hits to put it down. /hit".to_string(),
        }
    );
    assert_eq!(
        line(
            &spawn,
            Some("your tire iron hits the howler. 2 to go."),
            viewer
        ),
        LoungeLine {
            glyph: '╫',
            text: "your tire iron hits the howler. 2 to go.".to_string(),
        }
    );
}

#[test]
fn the_placeholder_lights_the_glyph_and_the_key() {
    let drawn = placeholder(&LoungeLine {
        glyph: '▖',
        text: "a flicker is in the channel. /hit".to_string(),
    });
    assert_eq!(text_of(&drawn), "▖ a flicker is in the channel. /hit");
    assert_eq!(
        drawn.spans.first().map(|span| span.content.as_ref()),
        Some("▖")
    );
    assert_eq!(
        drawn.spans.last().map(|span| span.content.as_ref()),
        Some("/hit")
    );

    let drawn = placeholder(&LoungeLine {
        glyph: '╫',
        text: "you already hit it. somebody else has to.".to_string(),
    });
    assert_eq!(
        text_of(&drawn),
        "╫ you already hit it. somebody else has to."
    );
    assert_eq!(drawn.spans.len(), 3);
}
