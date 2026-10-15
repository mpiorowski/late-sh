//! The spawn on the composer: one glyph and one line in the placeholder
//! of #lounge's composer, for a runner's session only, and the glyph
//! alone in the composer's title while they type. Pure: the chat's
//! composer block draws what `line` hands it.

use late_core::models::deadchannel_spawn::Spawn;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use uuid::Uuid;

use super::state::{SpawnKindRules, composer_text};
use crate::app::common::theme;

/// What the composer draws for the lounge: the glyph and the words.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoungeLine {
    pub glyph: char,
    pub text: String,
}

/// The line for `viewer`'s composer: the session's own `word` while it
/// stands, else what the spawn says.
pub(crate) fn line(spawn: &Spawn, word: Option<&str>, viewer: Uuid) -> LoungeLine {
    LoungeLine {
        glyph: spawn.kind.glyph(),
        text: match word {
            Some(word) => word.to_string(),
            None => composer_text(spawn, viewer),
        },
    }
}

/// The placeholder row: the glyph lit, the words dim, `/hit` lit where it
/// ends the line.
pub(crate) fn placeholder(line: &LoungeLine) -> Line<'static> {
    let lit = Style::default()
        .fg(theme::AMBER())
        .add_modifier(Modifier::BOLD);
    let dim = Style::default().fg(theme::TEXT_DIM());
    let mut spans = vec![
        Span::styled(line.glyph.to_string(), lit),
        Span::styled(" ", dim),
    ];
    match line.text.strip_suffix(" /hit") {
        Some(words) => {
            spans.push(Span::styled(words.to_string(), dim));
            spans.push(Span::styled(" ", dim));
            spans.push(Span::styled("/hit", lit));
        }
        None => spans.push(Span::styled(line.text.clone(), dim)),
    }
    Line::from(spans)
}

#[cfg(test)]
#[path = "ui_test.rs"]
mod ui_test;
