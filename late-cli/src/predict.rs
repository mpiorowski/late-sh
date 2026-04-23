use std::sync::{Arc, Mutex};

use vte::{Params, Parser, Perform};

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize)]
pub(super) struct ChatComposerHint {
    pub(super) active: bool,
    pub(super) x: u16,
    pub(super) y: u16,
    pub(super) width: u16,
    pub(super) height: u16,
    pub(super) text: String,
    pub(super) cursor_line: usize,
    pub(super) cursor_col: usize,
}

#[derive(Clone, Default)]
pub(super) struct LocalEcho {
    inner: Arc<Mutex<LocalEchoState>>,
}

#[derive(Default)]
struct LocalEchoState {
    hint: Option<ChatComposerHint>,
    predicted: Option<ComposerState>,
    vt_input: VtInputParser,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ComposerState {
    text: String,
    cursor_offset: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct OverlaySnapshot {
    pub(super) x: u16,
    pub(super) y: u16,
    pub(super) width: u16,
    pub(super) height: u16,
    pub(super) text: String,
    pub(super) cursor_offset: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum ParsedInput {
    Char(char),
    Byte(u8),
    Arrow(u8),
}

struct VtInputParser {
    parser: Parser,
    collector: VtCollector,
}

#[derive(Default)]
struct VtCollector {
    events: Vec<ParsedInput>,
}

#[derive(Clone, Debug)]
struct ComposerRow {
    text: String,
    start: usize,
    end: usize,
}

impl LocalEcho {
    pub(super) fn apply_chat_hint(&self, hint: ChatComposerHint) {
        let mut state = self.inner.lock().unwrap();
        let previous_authoritative = state
            .hint
            .as_ref()
            .filter(|hint| hint.active)
            .map(ComposerState::from_hint);
        let authoritative = if hint.active {
            Some(ComposerState::from_hint(&hint))
        } else {
            None
        };

        state.hint = Some(hint);
        match (&state.predicted, authoritative) {
            (_, None) => state.predicted = None,
            (Some(predicted), Some(authoritative)) if *predicted == authoritative => {
                state.predicted = None;
            }
            (Some(_), Some(authoritative))
                if previous_authoritative
                    .as_ref()
                    .is_some_and(|previous| previous != &authoritative) =>
            {
                // Server state moved in a way our local prediction did not
                // exactly match (submit/clear, external mutation, cursor move,
                // or partial reconciliation). Drop the prediction and rebase
                // subsequent local input on the new authoritative state.
                state.predicted = None;
            }
            _ => {}
        }
    }

    pub(super) fn apply_local_input(&self, data: &[u8]) -> Option<OverlaySnapshot> {
        let mut state = self.inner.lock().unwrap();
        let Some(hint) = state.hint.clone() else {
            return None;
        };
        if !hint.active {
            state.predicted = None;
            return None;
        }

        let mut composer = state
            .predicted
            .clone()
            .unwrap_or_else(|| ComposerState::from_hint(&hint));
        let mut changed = false;

        for event in state.vt_input.feed(data) {
            changed |= composer.apply_event(event);
        }

        if !changed {
            return None;
        }

        let snapshot = OverlaySnapshot {
            x: hint.x,
            y: hint.y,
            width: hint.width,
            height: hint.height,
            text: composer.text.clone(),
            cursor_offset: composer.cursor_offset,
        };
        state.predicted = Some(composer);
        Some(snapshot)
    }

    pub(super) fn overlay_snapshot(&self) -> Option<OverlaySnapshot> {
        let state = self.inner.lock().unwrap();
        let hint = state.hint.as_ref()?;
        if !hint.active {
            return None;
        }
        let predicted = state.predicted.as_ref()?;
        Some(OverlaySnapshot {
            x: hint.x,
            y: hint.y,
            width: hint.width,
            height: hint.height,
            text: predicted.text.clone(),
            cursor_offset: predicted.cursor_offset,
        })
    }

    pub(super) fn render_overlay(snapshot: &OverlaySnapshot) -> Vec<u8> {
        if snapshot.width == 0 || snapshot.height == 0 {
            return Vec::new();
        }
        let mut out = Vec::new();
        let rows = build_composer_rows(&snapshot.text, snapshot.width as usize);
        let (cursor_row, cursor_col) =
            cursor_screen_position(&rows, snapshot.cursor_offset).unwrap_or((0, 0));

        for row_index in 0..snapshot.height as usize {
            let row_text = rows.get(row_index).map(|row| row.text.as_str()).unwrap_or("");
            out.extend_from_slice(
                format!("\x1b[{};{}H", snapshot.y + row_index as u16 + 1, snapshot.x + 1)
                    .as_bytes(),
            );
            out.extend_from_slice(pad_to_width(row_text, snapshot.width as usize).as_bytes());
        }

        let cursor_row = cursor_row.min(snapshot.height.saturating_sub(1) as usize);
        let cursor_col = cursor_col.min(snapshot.width.saturating_sub(1) as usize);
        out.extend_from_slice(
            format!(
                "\x1b[{};{}H",
                snapshot.y + cursor_row as u16 + 1,
                snapshot.x + cursor_col as u16 + 1
            )
            .as_bytes(),
        );
        out
    }
}

impl ComposerState {
    fn from_hint(hint: &ChatComposerHint) -> Self {
        Self {
            text: hint.text.clone(),
            cursor_offset: line_col_to_offset(&hint.text, hint.cursor_line, hint.cursor_col),
        }
    }

    fn apply_event(&mut self, event: ParsedInput) -> bool {
        match event {
            ParsedInput::Char(ch) => {
                self.insert_char(ch);
                true
            }
            ParsedInput::Byte(0x7F | 0x08) => self.backspace(),
            ParsedInput::Byte(0x15) => self.kill_to_head(),
            ParsedInput::Arrow(b'A') => self.move_vertical(-1),
            ParsedInput::Arrow(b'B') => self.move_vertical(1),
            ParsedInput::Arrow(b'C') => self.move_horizontal(1),
            ParsedInput::Arrow(b'D') => self.move_horizontal(-1),
            _ => false,
        }
    }

    fn insert_char(&mut self, ch: char) {
        let byte_offset = char_to_byte_offset(&self.text, self.cursor_offset);
        self.text.insert(byte_offset, ch);
        self.cursor_offset += 1;
    }

    fn backspace(&mut self) -> bool {
        if self.cursor_offset == 0 {
            return false;
        }
        let remove_at = self.cursor_offset - 1;
        let start = char_to_byte_offset(&self.text, remove_at);
        let end = char_to_byte_offset(&self.text, self.cursor_offset);
        self.text.replace_range(start..end, "");
        self.cursor_offset = remove_at;
        true
    }

    fn kill_to_head(&mut self) -> bool {
        let (line, col) = offset_to_line_col(&self.text, self.cursor_offset);
        if col == 0 {
            return false;
        }
        let line_start = line_col_to_offset(&self.text, line, 0);
        let start = char_to_byte_offset(&self.text, line_start);
        let end = char_to_byte_offset(&self.text, self.cursor_offset);
        self.text.replace_range(start..end, "");
        self.cursor_offset = line_start;
        true
    }

    fn move_horizontal(&mut self, delta: isize) -> bool {
        let len = self.text.chars().count() as isize;
        let current = self.cursor_offset as isize;
        let next = (current + delta).clamp(0, len) as usize;
        if next == self.cursor_offset {
            return false;
        }
        self.cursor_offset = next;
        true
    }

    fn move_vertical(&mut self, delta: isize) -> bool {
        let lines = split_lines(&self.text);
        let (line, col) = offset_to_line_col(&self.text, self.cursor_offset);
        let next_line = (line as isize + delta).clamp(0, lines.len().saturating_sub(1) as isize);
        let next_line = next_line as usize;
        let next_col = col.min(lines[next_line].chars().count());
        let next_offset = line_col_to_offset(&self.text, next_line, next_col);
        if next_offset == self.cursor_offset {
            return false;
        }
        self.cursor_offset = next_offset;
        true
    }
}

impl Default for VtInputParser {
    fn default() -> Self {
        Self {
            parser: Parser::new(),
            collector: VtCollector::default(),
        }
    }
}

impl VtInputParser {
    fn feed(&mut self, data: &[u8]) -> Vec<ParsedInput> {
        self.parser.advance(&mut self.collector, data);
        std::mem::take(&mut self.collector.events)
    }
}

impl VtCollector {
    fn push_byte(&mut self, byte: u8) {
        self.events.push(ParsedInput::Byte(byte));
    }
}

impl Perform for VtCollector {
    fn print(&mut self, c: char) {
        if !c.is_control() {
            self.events.push(ParsedInput::Char(c));
        }
    }

    fn execute(&mut self, byte: u8) {
        self.push_byte(byte);
    }

    fn hook(&mut self, _: &Params, _: &[u8], _: bool, _: char) {}
    fn put(&mut self, _: u8) {}
    fn unhook(&mut self) {}
    fn osc_dispatch(&mut self, _: &[&[u8]], _: bool) {}

    fn csi_dispatch(&mut self, params: &Params, intermediates: &[u8], ignore: bool, action: char) {
        if ignore || !intermediates.is_empty() {
            return;
        }

        let params: Vec<u16> = params
            .iter()
            .map(|param| param.first().copied().unwrap_or(0))
            .collect();
        let p0 = params.first().copied().unwrap_or(0);
        let p1 = params.get(1).copied();
        let modifier = match p1 {
            Some(modifier) => modifier,
            None if p0 > 1 => p0,
            None => 0,
        };

        if modifier != 0 {
            return;
        }

        match action {
            'A' | 'B' | 'C' | 'D' => self.events.push(ParsedInput::Arrow(action as u8)),
            _ => {}
        }
    }

    fn esc_dispatch(&mut self, _: &[u8], _: bool, _: u8) {}
}

fn split_lines(text: &str) -> Vec<&str> {
    text.split('\n').collect()
}

fn line_col_to_offset(text: &str, line: usize, col: usize) -> usize {
    let lines = split_lines(text);
    let mut offset = 0;
    for (index, segment) in lines.iter().enumerate() {
        if index == line {
            return offset + col.min(segment.chars().count());
        }
        offset += segment.chars().count() + 1;
    }
    text.chars().count()
}

fn offset_to_line_col(text: &str, offset: usize) -> (usize, usize) {
    let mut consumed = 0;
    for (line_index, segment) in split_lines(text).iter().enumerate() {
        let len = segment.chars().count();
        if offset <= consumed + len {
            return (line_index, offset - consumed);
        }
        consumed += len + 1;
    }

    let lines = split_lines(text);
    let last_index = lines.len().saturating_sub(1);
    (last_index, lines.get(last_index).map(|line| line.chars().count()).unwrap_or(0))
}

fn char_to_byte_offset(text: &str, char_offset: usize) -> usize {
    if char_offset == 0 {
        return 0;
    }
    text.char_indices()
        .nth(char_offset)
        .map(|(index, _)| index)
        .unwrap_or(text.len())
}

fn build_composer_rows(text: &str, width: usize) -> Vec<ComposerRow> {
    if text.is_empty() {
        return vec![ComposerRow {
            text: String::new(),
            start: 0,
            end: 0,
        }];
    }

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
            .map(|index| start + index);

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

fn cursor_screen_position(rows: &[ComposerRow], cursor_offset: usize) -> Option<(usize, usize)> {
    if rows.is_empty() {
        return Some((0, 0));
    }

    for (index, row) in rows.iter().enumerate() {
        if row.start == row.end && cursor_offset == row.start {
            return Some((index, 0));
        }
        if cursor_offset >= row.start && cursor_offset <= row.end {
            return Some((index, cursor_offset.saturating_sub(row.start)));
        }
    }

    rows.last()
        .map(|row| (rows.len().saturating_sub(1), row.text.chars().count()))
}

fn pad_to_width(text: &str, width: usize) -> String {
    let mut out: String = text.chars().take(width).collect();
    let current = out.chars().count();
    if current < width {
        out.push_str(&" ".repeat(width - current));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hint(active: bool, text: &str, cursor_line: usize, cursor_col: usize) -> ChatComposerHint {
        ChatComposerHint {
            active,
            x: 10,
            y: 20,
            width: 20,
            height: 3,
            text: text.to_string(),
            cursor_line,
            cursor_col,
        }
    }

    #[test]
    fn authoritative_reset_drops_stale_prediction() {
        let echo = LocalEcho::default();
        echo.apply_chat_hint(hint(true, "hello", 0, 5));

        let first = echo
            .apply_local_input(b"x")
            .expect("local prediction after append");
        assert_eq!(first.text, "hellox");

        // Server clears the composer while keeping it active (e.g. stay-open
        // submit or another authoritative mutation). The stale local draft
        // must be dropped before the next local keypress.
        echo.apply_chat_hint(hint(true, "", 0, 0));
        let second = echo
            .apply_local_input(b"y")
            .expect("prediction rebased on cleared authoritative state");
        assert_eq!(second.text, "y");
    }

    #[test]
    fn unchanged_authoritative_hint_keeps_prediction() {
        let echo = LocalEcho::default();
        echo.apply_chat_hint(hint(true, "hello", 0, 5));

        let first = echo
            .apply_local_input(b"x")
            .expect("local prediction after append");
        assert_eq!(first.text, "hellox");

        // Duplicate server hint before the SSH/WS round-trip catches up
        // should not discard the still-pending local echo.
        echo.apply_chat_hint(hint(true, "hello", 0, 5));
        let snapshot = echo.overlay_snapshot().expect("prediction preserved");
        assert_eq!(snapshot.text, "hellox");
    }
}
