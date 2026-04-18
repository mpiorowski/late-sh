use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};

use crate::app::common::theme;

#[derive(Clone, Debug, Default)]
pub struct ComposerState {
    text: String,
    cursor: usize,
    text_width: usize,
    rows: Vec<ComposerRow>,
    layout_dirty: bool,
}

impl ComposerState {
    pub fn new(text_width: usize) -> Self {
        Self {
            text_width: text_width.max(1),
            layout_dirty: true,
            ..Self::default()
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn rows(&self) -> &[ComposerRow] {
        &self.rows
    }

    pub fn set_text_width(&mut self, width: usize) {
        let width = width.max(1);
        if self.text_width != width {
            self.text_width = width;
            self.layout_dirty = true;
        }
    }

    pub fn sync_layout(&mut self) {
        if !self.layout_dirty {
            return;
        }
        self.rows = build_composer_rows(&self.text, self.text_width);
        self.layout_dirty = false;
    }

    pub fn set_text(&mut self, text: impl Into<String>) {
        self.text = text.into();
        self.cursor = self.text.chars().count();
        self.layout_dirty = true;
    }

    pub fn clear(&mut self) {
        self.text.clear();
        self.cursor = 0;
        self.layout_dirty = true;
    }

    pub fn push(&mut self, ch: char) {
        let char_count = self.text.chars().count();
        if self.cursor >= char_count {
            self.text.push(ch);
        } else {
            let byte_pos = self
                .text
                .char_indices()
                .nth(self.cursor)
                .map(|(i, _)| i)
                .unwrap_or(self.text.len());
            self.text.insert(byte_pos, ch);
        }
        self.cursor += 1;
        self.layout_dirty = true;
    }

    pub fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let byte_pos = self
            .text
            .char_indices()
            .nth(self.cursor - 1)
            .map(|(i, _)| i)
            .unwrap_or(0);
        let next_byte = self
            .text
            .char_indices()
            .nth(self.cursor)
            .map(|(i, _)| i)
            .unwrap_or(self.text.len());
        self.text.replace_range(byte_pos..next_byte, "");
        self.cursor -= 1;
        self.layout_dirty = true;
    }

    pub fn delete_right(&mut self) {
        let char_count = self.text.chars().count();
        if self.cursor >= char_count {
            return;
        }

        let byte_pos = self
            .text
            .char_indices()
            .nth(self.cursor)
            .map(|(i, _)| i)
            .unwrap_or(self.text.len());
        let next_byte = self
            .text
            .char_indices()
            .nth(self.cursor + 1)
            .map(|(i, _)| i)
            .unwrap_or(self.text.len());
        self.text.replace_range(byte_pos..next_byte, "");
        self.layout_dirty = true;
    }

    pub fn delete_word_right(&mut self) {
        let chars: Vec<char> = self.text.chars().collect();
        let len = chars.len();
        let start = self.cursor.min(len);
        if start >= len {
            return;
        }

        let mut end = start;
        while end < len && chars[end].is_whitespace() {
            end += 1;
        }
        while end < len && !chars[end].is_whitespace() {
            end += 1;
        }

        let start_byte = self
            .text
            .char_indices()
            .nth(start)
            .map(|(i, _)| i)
            .unwrap_or(self.text.len());
        let end_byte = self
            .text
            .char_indices()
            .nth(end)
            .map(|(i, _)| i)
            .unwrap_or(self.text.len());
        self.text.replace_range(start_byte..end_byte, "");
        self.layout_dirty = true;
    }

    pub fn delete_word_left(&mut self) {
        if self.cursor == 0 {
            return;
        }

        let chars: Vec<char> = self.text.chars().collect();
        let end = self.cursor.min(chars.len());
        let mut start = end;
        let at_word_boundary = end == chars.len() || chars[end].is_whitespace();

        while start > 0 && chars[start - 1].is_whitespace() {
            start -= 1;
        }
        while start > 0 && !chars[start - 1].is_whitespace() {
            start -= 1;
        }
        if at_word_boundary {
            while start > 0 && chars[start - 1].is_whitespace() {
                start -= 1;
            }
        }

        let start_byte = self
            .text
            .char_indices()
            .nth(start)
            .map(|(i, _)| i)
            .unwrap_or(0);
        let end_byte = self
            .text
            .char_indices()
            .nth(end)
            .map(|(i, _)| i)
            .unwrap_or(self.text.len());
        self.text.replace_range(start_byte..end_byte, "");
        self.cursor = start;
        self.layout_dirty = true;
    }

    pub fn cursor_left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    pub fn cursor_right(&mut self) {
        let char_count = self.text.chars().count();
        if self.cursor < char_count {
            self.cursor += 1;
        }
    }

    pub fn cursor_word_left(&mut self) {
        if self.cursor == 0 {
            return;
        }

        let chars: Vec<char> = self.text.chars().collect();
        let mut cursor = self.cursor.min(chars.len());

        while cursor > 0 && chars[cursor - 1].is_whitespace() {
            cursor -= 1;
        }
        while cursor > 0 && !chars[cursor - 1].is_whitespace() {
            cursor -= 1;
        }

        self.cursor = cursor;
    }

    pub fn cursor_word_right(&mut self) {
        let chars: Vec<char> = self.text.chars().collect();
        let len = chars.len();
        let mut cursor = self.cursor.min(len);

        while cursor < len && chars[cursor].is_whitespace() {
            cursor += 1;
        }
        while cursor < len && !chars[cursor].is_whitespace() {
            cursor += 1;
        }

        self.cursor = cursor;
    }

    pub fn cursor_up(&mut self) {
        self.sync_layout();
        if self.rows.is_empty() {
            return;
        }
        let row_idx = self
            .rows
            .iter()
            .position(|r| self.cursor <= r.end)
            .unwrap_or(self.rows.len() - 1);
        if row_idx == 0 {
            return;
        }
        let col = self.cursor.saturating_sub(self.rows[row_idx].start);
        let prev = &self.rows[row_idx - 1];
        let row_len = prev.text.chars().count();
        self.cursor = prev.start + col.min(row_len);
    }

    pub fn cursor_down(&mut self) {
        self.sync_layout();
        if self.rows.is_empty() {
            return;
        }
        let row_idx = self
            .rows
            .iter()
            .position(|r| self.cursor <= r.end)
            .unwrap_or(self.rows.len() - 1);
        if row_idx >= self.rows.len() - 1 {
            return;
        }
        let col = self.cursor.saturating_sub(self.rows[row_idx].start);
        let next = &self.rows[row_idx + 1];
        let row_len = next.text.chars().count();
        self.cursor = next.start + col.min(row_len);
    }
}

#[derive(Clone, Debug)]
pub struct ComposerRow {
    pub text: String,
    pub start: usize,
    pub end: usize,
}

pub fn build_composer_lines(
    text: &str,
    cursor_pos: usize,
    composing: bool,
    cursor_visible: bool,
    width: usize,
) -> Vec<Line<'static>> {
    let rows = build_composer_rows(text, width);
    build_composer_lines_from_rows(text, &rows, cursor_pos, composing, cursor_visible)
}

pub fn build_composer_lines_from_rows(
    text: &str,
    rows: &[ComposerRow],
    cursor_pos: usize,
    composing: bool,
    cursor_visible: bool,
) -> Vec<Line<'static>> {
    if text.is_empty() {
        if composing {
            let dim = Style::default().fg(theme::TEXT_DIM());
            let first_style = if cursor_visible {
                dim.add_modifier(Modifier::REVERSED)
            } else {
                dim
            };
            return vec![Line::from(vec![
                Span::raw(" "),
                Span::styled("T", first_style),
                Span::styled("ype a message...", dim),
            ])];
        }
        return vec![Line::from("")];
    }
    let cursor_row = if cursor_visible && composing {
        Some(
            rows.iter()
                .position(|row| cursor_pos <= row.end)
                .unwrap_or(rows.len().saturating_sub(1)),
        )
    } else {
        None
    };

    let mut result: Vec<Line> = Vec::new();
    for (i, row) in rows.iter().enumerate() {
        if let Some(cursor_row_idx) = cursor_row
            && cursor_row_idx == i
        {
            let cursor_col = cursor_pos
                .saturating_sub(row.start)
                .min(row.text.chars().count());
            let row_chars: Vec<char> = row.text.chars().collect();
            let before: String = row_chars.iter().take(cursor_col).collect();
            let at_cursor = row_chars.get(cursor_col).copied();
            let after: String = row_chars.iter().skip(cursor_col + 1).collect();
            let mut spans = vec![Span::raw(" "), Span::raw(before)];
            if let Some(ch) = at_cursor {
                spans.push(Span::styled(
                    ch.to_string(),
                    Style::default().add_modifier(Modifier::REVERSED),
                ));
                if !after.is_empty() {
                    spans.push(Span::raw(after));
                }
            } else {
                spans.push(Span::styled(
                    " ",
                    Style::default().add_modifier(Modifier::REVERSED),
                ));
            }
            result.push(Line::from(spans));
        } else {
            let is_last = i == rows.len() - 1;
            let mut spans = vec![Span::raw(" "), Span::raw(row.text.clone())];
            if is_last && composing && cursor_row.is_none() {
                spans.push(Span::raw(" "));
            }
            result.push(Line::from(spans));
        }
    }

    result
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

pub fn composer_line_count_for_rows(text: &str, rows: &[ComposerRow]) -> usize {
    if text.is_empty() {
        1
    } else {
        rows.len().max(1)
    }
}

pub fn composer_cursor_scroll_for_rows(
    rows: &[ComposerRow],
    cursor_pos: usize,
    max_visible: usize,
) -> u16 {
    let total = rows.len().max(1);
    if total <= max_visible {
        return 0;
    }
    let cursor_row = rows
        .iter()
        .position(|r| cursor_pos <= r.end)
        .unwrap_or(total.saturating_sub(1));
    let max_scroll = total.saturating_sub(max_visible);
    let scroll = cursor_row.saturating_sub(max_visible - 1).min(max_scroll);
    scroll as u16
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines_to_strings(lines: &[Line<'_>]) -> Vec<String> {
        lines
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect::<String>()
            })
            .collect()
    }

    #[test]
    fn composer_rows_soft_wrap_words() {
        let rows = build_composer_rows("hello wide world", 8);
        let texts: Vec<&str> = rows.iter().map(|row| row.text.as_str()).collect();
        assert_eq!(texts, vec!["hello", "wide", "world"]);
    }

    #[test]
    fn build_lines_shows_placeholder_while_composing() {
        let lines = build_composer_lines("", 0, true, false, 20);
        let strings = lines_to_strings(&lines);
        assert_eq!(strings, vec![" Type a message..."]);
    }

    #[test]
    fn cursor_up_and_down_follow_wrapped_rows() {
        let mut state = ComposerState::new(8);
        state.set_text("hello wide world");
        state.sync_layout();
        state.cursor_up();
        assert_eq!(state.cursor(), 10);
        state.cursor_up();
        assert_eq!(state.cursor(), 4);
        state.cursor_down();
        assert_eq!(state.cursor(), 10);
    }

    #[test]
    fn delete_word_left_removes_previous_word() {
        let mut state = ComposerState::new(20);
        state.set_text("hello wide world");
        state.delete_word_left();
        assert_eq!(state.text(), "hello wide");
    }
}
