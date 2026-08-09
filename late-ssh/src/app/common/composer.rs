//! Legacy word-wrap helpers used for composer-height estimation and for
//! rendering read-only wrapped text (e.g. the profile bio). The interactive
//! composer/editor state lives in `ratatui_textarea::TextArea`, but common
//! theme styling for those text areas belongs here so every composer can
//! refresh after the active theme changes.

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui_textarea::{TextArea, WrapMode};

use super::theme;

#[derive(Clone, Debug)]
pub struct ComposerRow {
    pub text: String,
    pub start: usize,
    pub end: usize,
}

pub fn build_composer_rows(text: &str, width: usize) -> Vec<ComposerRow> {
    let mut rows = Vec::new();
    let mut offset = 0;

    for paragraph in text.split('\n') {
        let wrapped = wrap_composer_paragraph(paragraph, width);
        if wrapped.is_empty() {
            rows.push(ComposerRow {
                text: String::new(),
                start: offset,
                end: offset,
            });
        } else {
            for (row_text, start, end) in wrapped {
                rows.push(ComposerRow {
                    text: row_text,
                    start: offset + start,
                    end: offset + end,
                });
            }
        }
        offset += paragraph.chars().count() + 1;
    }

    rows
}

fn wrap_composer_paragraph(paragraph: &str, width: usize) -> Vec<(String, usize, usize)> {
    if paragraph.is_empty() {
        return Vec::new();
    }
    if width == 0 {
        return vec![(String::new(), 0, 0)];
    }

    let chars: Vec<char> = paragraph.chars().collect();
    let mut out = Vec::new();
    let mut start = 0;

    while start < chars.len() {
        let end = (start + width).min(chars.len());
        if end == chars.len() {
            out.push((chars[start..end].iter().collect(), start, end));
            break;
        }

        let break_at = chars[start..end]
            .iter()
            .rposition(|ch| ch.is_whitespace())
            .map(|idx| start + idx);

        match break_at {
            Some(split) if split > start => {
                out.push((chars[start..split].iter().collect(), start, split));
                start = split + 1;
            }
            _ => {
                out.push((chars[start..end].iter().collect(), start, end));
                start = end;
            }
        }
    }

    out
}

pub fn composer_line_count(text: &str, width: usize) -> usize {
    if text.is_empty() {
        1
    } else {
        build_composer_rows(text, width).len().max(1)
    }
}

pub fn new_themed_textarea(
    placeholder: impl Into<String>,
    wrap_mode: WrapMode,
    cursor_visible: bool,
) -> TextArea<'static> {
    let mut ta = TextArea::default();
    apply_themed_textarea_style(&mut ta, cursor_visible);
    ta.set_placeholder_text(placeholder);
    ta.set_wrap_mode(wrap_mode);
    ta
}

pub fn apply_themed_textarea_style(ta: &mut TextArea<'static>, cursor_visible: bool) {
    ta.set_style(Style::default().fg(theme::TEXT()));
    ta.set_placeholder_style(Style::default().fg(theme::TEXT_DIM()));
    ta.set_cursor_line_style(Style::default().fg(theme::TEXT()));
    set_themed_textarea_cursor_visible(ta, cursor_visible);
}

pub fn set_themed_textarea_cursor_visible(ta: &mut TextArea<'static>, visible: bool) {
    let style = if visible {
        visible_textarea_cursor_style()
    } else {
        hidden_textarea_cursor_style()
    };
    ta.set_cursor_style(style);
}

/// An empty input's hint, drawn with the block cursor sitting **on** its first
/// character rather than in a cell of its own before it. A `TextArea` renders
/// its own placeholder after the cursor cell, which reads as a stray block
/// floating to the left of the text; every composer in the app draws the empty
/// state itself for that reason.
pub fn placeholder_with_cursor(text: &str) -> Line<'static> {
    let mut chars = text.chars();
    let Some(first) = chars.next() else {
        return Line::from(Span::styled(" ", visible_textarea_cursor_style()));
    };
    Line::from(vec![
        Span::styled(first.to_string(), visible_textarea_cursor_style()),
        Span::styled(
            chars.collect::<String>(),
            Style::default().fg(theme::TEXT_DIM()),
        ),
    ])
}

fn hidden_textarea_cursor_style() -> Style {
    Style::default().fg(theme::TEXT())
}

/// An inverted block of the text color. Punched through rather than painted
/// as an explicit fg/bg pair because on the terminal palette both `TEXT()`
/// and `BG_CANVAS()` resolve to `Color::Reset`, and that pair paints no
/// visible cursor at all.
fn visible_textarea_cursor_style() -> Style {
    theme::punch_through(theme::TEXT()).add_modifier(Modifier::BOLD)
}
