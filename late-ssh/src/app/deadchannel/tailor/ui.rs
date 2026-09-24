//! The tailor's panel, in the city's palette: the mirror on the left
//! (the draft as a portrait, the mark under it), four rack rows beside
//! it with the cursor on one, each rack a window of five around the
//! piece worn, the row's tint on the piece and named beside the label;
//! the keys; the tailor's last word. Pure: a function of the session's
//! draft and word. The city's `draw_panel` frames it.

use ratatui::{
    style::Modifier,
    text::{Line, Span},
};

use super::state::{Draft, Row};
use crate::app::deadchannel::city::map::Neon;
use crate::app::deadchannel::city::ui::{INK, INK_BRIGHT, INK_DIM, INK_MUTED, glow, ink, lit, tint_rgb};
use crate::app::deadchannel::glyphs::GLYPH_ALPHABET;
use crate::app::deadchannel::runner::state::pieces_for;

/// Pieces shown around the worn one, each side.
const RACK_REACH: usize = 2;

pub(crate) struct MirrorView<'a> {
    pub draft: Option<&'a Draft>,
    pub word: Option<&'a str>,
    /// The draft differs from what the row wears: `[s]` lights up.
    pub changed: bool,
    pub saving: bool,
}

pub(crate) fn mirror_lines(view: &MirrorView<'_>) -> Vec<Line<'static>> {
    let text = ink(INK);
    let dim_text = ink(INK_DIM);
    let muted_text = ink(INK_MUTED);
    let head = ink(INK_BRIGHT).add_modifier(Modifier::BOLD);
    let key = lit(Neon::Amber);
    let mut lines: Vec<Line<'static>> = Vec::new();

    let Some(draft) = view.draft else {
        lines.push(Line::from(Span::styled(
            "nothing looks back. you have no runner yet.",
            muted_text,
        )));
        return lines;
    };

    let rows = [Row::Hood, Row::Eyes, Row::Coat, Row::Mark];
    for row in rows {
        let on = draft.row == row;
        let mut spans: Vec<Span<'static>> = Vec::new();
        // The mirror.
        match draft.worn(row) {
            Some(worn) => spans.push(Span::styled(
                format!(" {} ", worn.piece.row),
                ink(tint_rgb(worn.tint)),
            )),
            None => spans.push(Span::styled(format!("   {}   ", draft.look.mark), key)),
        }
        spans.push(Span::styled("    ", text));
        // The cursor and the label.
        spans.push(Span::styled(
            match on {
                true => "▸ ",
                false => "  ",
            },
            key,
        ));
        let label = match row {
            Row::Hood => "hood",
            Row::Eyes => "eyes",
            Row::Coat => "coat",
            Row::Mark => "mark",
        };
        spans.push(Span::styled(
            format!("{label:<6}"),
            match on {
                true => head,
                false => dim_text,
            },
        ));
        // The tint, then the rack.
        match draft.worn(row) {
            Some(worn) => {
                let tint = format!("{:?}", worn.tint).to_lowercase();
                spans.push(Span::styled(
                    format!("{tint:<10}"),
                    match on {
                        true => ink(tint_rgb(worn.tint)),
                        false => dim_text,
                    },
                ));
                let slot = row.slot().expect("a dressed row has a slot");
                let rack: Vec<&'static str> = pieces_for(slot).map(|piece| piece.row).collect();
                let at = rack
                    .iter()
                    .position(|piece| *piece == worn.piece.row)
                    .expect("the worn piece is on the rack");
                spans.push(Span::styled("◂ ", dim_text));
                for offset in -(RACK_REACH as i32)..=(RACK_REACH as i32) {
                    let index = (at as i32 + offset).rem_euclid(rack.len() as i32) as usize;
                    match offset == 0 {
                        true => spans.push(Span::styled(
                            format!("[{}]", rack[index]),
                            ink(tint_rgb(worn.tint)).add_modifier(Modifier::BOLD),
                        )),
                        false => spans.push(Span::styled(format!(" {} ", rack[index]), dim_text)),
                    }
                }
                spans.push(Span::styled(" ▸", dim_text));
            }
            None => {
                spans.push(Span::styled(" ".repeat(10), text));
                spans.push(Span::styled("◂ ", dim_text));
                for glyph in GLYPH_ALPHABET {
                    match glyph == draft.look.mark {
                        true => spans.push(Span::styled(format!("[{glyph}]"), key)),
                        false => spans.push(Span::styled(format!(" {glyph} "), dim_text)),
                    }
                }
                spans.push(Span::styled(" ▸", dim_text));
            }
        }
        lines.push(Line::from(spans));
    }

    lines.push(Line::default());
    let mut keys = vec![
        Span::styled("[↑↓] ", key),
        Span::styled("row   ", text),
        Span::styled("[←→] ", key),
        Span::styled("pick   ", text),
        Span::styled("[t] ", key),
        Span::styled("tint   ", text),
        Span::styled("[r] ", key),
        Span::styled("shuffle   ", text),
    ];
    match (view.saving, view.changed) {
        (true, _) => keys.push(Span::styled("wearing it...   ", muted_text)),
        (false, true) => {
            keys.push(Span::styled("[s] ", key));
            keys.push(Span::styled("wear it   ", glow(Neon::Cyan)));
        }
        (false, false) => {
            keys.push(Span::styled("[s] ", dim_text));
            keys.push(Span::styled("wear it   ", dim_text));
        }
    }
    keys.push(Span::styled("[Enter] ", key));
    keys.push(Span::styled("back to the street", text));
    lines.push(Line::from(keys));
    lines.push(Line::from(Span::styled(
        "the starter rack is free, forever. bought pieces come with the season, for chips.",
        dim_text,
    )));
    if let Some(word) = view.word {
        lines.push(Line::from(Span::styled(word.to_string(), lit(Neon::Cyan))));
    }
    lines
}

#[cfg(test)]
#[path = "ui_test.rs"]
mod ui_test;
