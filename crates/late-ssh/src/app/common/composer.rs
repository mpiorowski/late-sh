//! Legacy word-wrap helpers used for composer-height estimation and for
//! rendering read-only wrapped text (e.g. the profile bio). The interactive
//! composer/editor state lives in `ratatui_textarea::TextArea`, but common
//! theme styling for those text areas belongs here so every composer can
//! refresh after the active theme changes.

use ratatui::style::{Modifier, Style};
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

fn hidden_textarea_cursor_style() -> Style {
    Style::default().fg(theme::TEXT())
}

fn visible_textarea_cursor_style() -> Style {
    Style::default()
        .fg(theme::BG_CANVAS())
        .bg(theme::TEXT())
        .add_modifier(Modifier::BOLD)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn composer_rows_soft_wrap_words() {
        let rows = build_composer_rows("hello wide world", 8);
        let texts: Vec<&str> = rows.iter().map(|row| row.text.as_str()).collect();
        assert_eq!(texts, vec!["hello", "wide", "world"]);
    }

    #[test]
    fn themed_textarea_uses_theme_text_color() {
        let textarea = new_themed_textarea("Type a message...", WrapMode::Word, false);
        assert_eq!(textarea.style().fg, Some(theme::TEXT()));
        assert_eq!(textarea.cursor_line_style().fg, Some(theme::TEXT()));
        assert_eq!(textarea.cursor_style().fg, Some(theme::TEXT()));
        assert_eq!(textarea.cursor_style().bg, None);
    }

    #[test]
    fn themed_textarea_visible_cursor_uses_explicit_theme_colors() {
        let textarea = new_themed_textarea("Type a message...", WrapMode::Word, true);
        assert_eq!(textarea.cursor_style().fg, Some(theme::BG_CANVAS()));
        assert_eq!(textarea.cursor_style().bg, Some(theme::TEXT()));
    }

    #[test]
    fn apply_themed_textarea_style_refreshes_existing_textarea_colors() {
        theme::set_current_by_id("late");
        let mut textarea = new_themed_textarea("Type a message...", WrapMode::Word, false);
        let late_text = textarea.style().fg;

        theme::set_current_by_id("contrast");
        apply_themed_textarea_style(&mut textarea, true);

        assert_ne!(textarea.style().fg, late_text);
        assert_eq!(textarea.style().fg, Some(theme::TEXT()));
        assert_eq!(textarea.cursor_line_style().fg, Some(theme::TEXT()));
        assert_eq!(textarea.cursor_style().fg, Some(theme::BG_CANVAS()));
        assert_eq!(textarea.cursor_style().bg, Some(theme::TEXT()));

        theme::set_current_by_id("late");
    }
}
