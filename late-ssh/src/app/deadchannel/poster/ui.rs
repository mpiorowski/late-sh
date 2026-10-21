//! PRINTS's panel body, in the city's palette: the poster at the left, its
//! 32 by 12 cells on their own paper with the cursor and the selection on
//! them; beside it the brush (the seven tints with the one in hand
//! bracketed, or the colour mixed on every colour), the keys, the burn's
//! warning in red while it is armed; the colour picker over the body while
//! it is open. The city's `draw_panel` frames it and puts the printer's
//! last word (`word`) at its foot. Pure.
//!
//! `canvas_lines` is the poster as lines of cells, the one way a poster is
//! drawn in the terminal: here, and on the profile (`profile_modal`).

use dartboard_core::{Canvas, CellValue, Pos, RgbColor};
use dartboard_editor::Selection;
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
};

use super::session::{Bench, Write};
use super::state::{Brush, Draft, POSTER_HEIGHT, POSTER_WIDTH, canvas_rect, tint_paint};
use crate::app::common::color_picker;
use crate::app::deadchannel::city::map::Neon;
use crate::app::deadchannel::city::ui::{INK, INK_BRIGHT, INK_DIM, INK_MUTED, action, ink, lit};
use crate::app::deadchannel::runner::state::TINTS;

/// The poster's own paper: every screen draws it on this, so a picture
/// reads the same on the street, on a profile in any theme, and on the web
/// (`late-web/static/board.js` fills the same colour).
const PAPER: Color = Color::Rgb(17, 24, 39);
/// A cell painted with no colour (none from PRINTS; the web's fallback).
const PAPER_INK: Color = Color::Rgb(209, 213, 219);
/// The selection, `dartboard_tui`'s own colours for it.
const SELECTION_BG: Color = Color::Rgb(64, 40, 24);
const SELECTION_FG: Color = Color::Rgb(208, 166, 89);
/// Columns between the poster and the column beside it.
const GAP: u16 = 3;

pub(crate) struct PosterView<'a> {
    pub bench: &'a Bench,
    /// A poster hangs on the wall: `[^D] burn` lights.
    pub hung: bool,
    pub writing: Option<Write>,
    /// The printer's last word, for the panel's foot.
    pub word: Option<&'a str>,
}

/// The poster's cells as lines, `POSTER_WIDTH` by `POSTER_HEIGHT`
/// whatever the canvas holds (a cell it does not have is paper), with the
/// cursor reversed and the selection lit when given.
pub(crate) fn canvas_lines(
    canvas: &Canvas,
    cursor: Option<Pos>,
    selection: Option<Selection>,
) -> Vec<Line<'static>> {
    (0..POSTER_HEIGHT)
        .map(|y| {
            let mut spans: Vec<Span<'static>> = Vec::with_capacity(POSTER_WIDTH);
            let mut x = 0;
            while x < POSTER_WIDTH {
                let pos = Pos { x, y };
                let (ch, width) = match canvas.cell(pos) {
                    Some(CellValue::Narrow(ch)) => (ch, 1),
                    Some(CellValue::Wide(ch)) if x + 1 < POSTER_WIDTH => (ch, 2),
                    Some(CellValue::Wide(_) | CellValue::WideCont) | None => (' ', 1),
                };
                let fg = match canvas.fg(pos) {
                    Some(fg) => rgb(fg),
                    None => PAPER_INK,
                };
                let mut style = match selection.is_some_and(|selection| selection.contains(pos)) {
                    true => Style::default().fg(SELECTION_FG).bg(SELECTION_BG),
                    false => Style::default().fg(fg).bg(PAPER),
                };
                if cursor == Some(pos) {
                    style = style.add_modifier(Modifier::REVERSED);
                }
                spans.push(Span::styled(ch.to_string(), style));
                x += width;
            }
            Line::from(spans)
        })
        .collect()
}

fn rgb(color: RgbColor) -> Color {
    Color::Rgb(color.r, color.g, color.b)
}

/// The rows the panel's body takes, as the city sizes it: the poster's
/// height while a draft is open (`draw` paints the canvas and the column
/// over them), or the line that says why there is nothing to paint on.
pub(crate) fn panel_rows(view: &PosterView<'_>) -> Vec<Line<'static>> {
    let muted = ink(INK_MUTED);
    match view.bench {
        Bench::Open(_) => vec![Line::default(); POSTER_HEIGHT],
        Bench::Reading { .. } => vec![Line::from(Span::styled(
            "the printer goes to find what hangs on your wall...",
            muted,
        ))],
        Bench::Unread => vec![Line::from(Span::styled(
            "the printer cannot find your wall. step out and come back.",
            muted,
        ))],
        Bench::NoRunner | Bench::Shut => vec![Line::from(Span::styled(
            "nothing to print. you have no runner yet.",
            muted,
        ))],
    }
}

/// The panel's body while a draft is open: the poster at the top left of
/// `body`, the column beside it, the picker over both. Records `body` on
/// the draft, where a click is read against it.
pub(crate) fn draw(frame: &mut Frame, body: Rect, view: &PosterView<'_>) {
    let Bench::Open(draft) = view.bench else {
        return;
    };
    draft.record_drawn_at(body);
    let selection = match draft.selecting() {
        true => draft.editor.selection(),
        false => None,
    };
    frame.render_widget(
        Paragraph::new(canvas_lines(
            &draft.canvas,
            Some(draft.editor.cursor),
            selection,
        )),
        canvas_rect(body),
    );
    let beside = POSTER_WIDTH as u16 + GAP;
    if body.width > beside {
        let column = Rect {
            x: body.x + beside,
            width: body.width - beside,
            ..body
        };
        frame.render_widget(
            Paragraph::new(column_lines(draft, view)).wrap(Wrap { trim: false }),
            column,
        );
    }
    if let Some(picker) = draft.picker() {
        color_picker::draw(frame, body, picker, "brush");
    }
}

/// The column beside the poster: what it is, the brush, the keys.
fn column_lines(draft: &Draft, view: &PosterView<'_>) -> Vec<Line<'static>> {
    let text = ink(INK);
    let dim_text = ink(INK_DIM);
    let head = ink(INK_BRIGHT).add_modifier(Modifier::BOLD);
    let key = lit(Neon::Amber);
    let gap = || Span::styled("  ", text);
    let pair = |keys: &str, verb: &str| {
        [
            Span::styled(format!("[{keys}] "), key),
            Span::styled(verb.to_string(), action()),
        ]
    };
    let mut lines: Vec<Line<'static>> = Vec::new();

    lines.push(Line::from(vec![
        Span::styled("the poster", head),
        Span::styled("  32 x 12", dim_text),
    ]));
    lines.push(Line::from(match (draft.dirty(), view.hung) {
        (true, _) => Span::styled("unsaved: not on your profile yet", lit(Neon::Amber)),
        (false, true) => Span::styled("on your profile", ink(INK_BRIGHT)),
        (false, false) => Span::styled("nothing hangs yet: the test card shows", dim_text),
    }));
    lines.push(Line::default());

    // The brush.
    match &draft.brush {
        Brush::Tints(tint) => {
            let mut spans = Vec::new();
            for candidate in TINTS {
                let fg = ink_of(tint_paint(candidate));
                match candidate == *tint {
                    true => spans.push(Span::styled("[█]", fg.add_modifier(Modifier::BOLD))),
                    false => spans.push(Span::styled(" █ ", fg)),
                }
            }
            spans.push(Span::styled(format!(" {}", tint.name()), text));
            lines.push(Line::from(spans));
            lines.push(Line::from(Span::styled(
                "one Old Signal mark opens every colour",
                dim_text,
            )));
        }
        Brush::Full { color, .. } => {
            lines.push(Line::from(vec![
                Span::styled("███", ink_of(*color)),
                Span::styled(format!(" #{}", color_picker::hex_of(*color)), text),
            ]));
            lines.push(Line::from(Span::styled(
                "every colour: the mark opened them",
                dim_text,
            )));
        }
    }
    lines.push(Line::default());

    // The keys.
    let mut row = pair("←↑↓→", "move").to_vec();
    row.push(gap());
    row.extend(pair("⇧", "select"));
    lines.push(Line::from(row));
    let mut row = pair("type", "paint").to_vec();
    row.push(gap());
    row.extend(pair("⌫", "erase"));
    lines.push(Line::from(row));
    let mut row = pair("^C", "copy").to_vec();
    row.push(gap());
    row.extend(pair("^X", "cut"));
    row.push(gap());
    row.extend(pair("^V", "paste"));
    lines.push(Line::from(row));
    let colour = match draft.brush {
        Brush::Tints(_) => "tint",
        Brush::Full { .. } => "colour",
    };
    let mut row = pair("^K", colour).to_vec();
    row.push(gap());
    row.extend(pair("^]", "glyphs"));
    lines.push(Line::from(row));
    lines.push(Line::from(write_row(draft, view)));
    let mut row = pair("Esc", "back").to_vec();
    row.push(Span::styled(" to the street", text));
    lines.push(Line::from(row));
    lines
}

/// Save and burn: lit when there is something to do, the write named while
/// it is out, and the burn's warning in red, in its own row, while armed.
fn write_row(draft: &Draft, view: &PosterView<'_>) -> Vec<Span<'static>> {
    let key = lit(Neon::Amber);
    let dim_text = ink(INK_DIM);
    let muted = ink(INK_MUTED);
    if draft.burn_armed {
        return vec![Span::styled(
            "[^D] again burns it. any other key keeps it.",
            lit(Neon::Red),
        )];
    }
    let mut spans = Vec::new();
    match (view.writing, draft.dirty()) {
        (Some(Write::Save), _) => spans.push(Span::styled("printing...", muted)),
        (_, true) => {
            spans.push(Span::styled("[^S] ", key));
            spans.push(Span::styled("save it", action()));
        }
        (_, false) => spans.push(Span::styled("[^S] save it", dim_text)),
    }
    spans.push(Span::styled("  ", dim_text));
    match (view.writing, view.hung) {
        (Some(Write::Burn), _) => spans.push(Span::styled("burning...", muted)),
        (_, true) => {
            spans.push(Span::styled("[^D] ", key));
            spans.push(Span::styled("burn", lit(Neon::Red)));
        }
        (_, false) => spans.push(Span::styled("[^D] burn", dim_text)),
    }
    spans
}

/// A colour on the night, for the brush's swatches.
fn ink_of(color: RgbColor) -> Style {
    ink([
        f32::from(color.r) / 255.0,
        f32::from(color.g) / 255.0,
        f32::from(color.b) / 255.0,
    ])
}

#[cfg(test)]
#[path = "ui_test.rs"]
mod ui_test;
