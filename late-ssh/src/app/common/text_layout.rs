//! Terminal text geometry. Ratatui owns the width rules; Unicode graphemes are
//! the indivisible units for clipping, scrolling and wrapping. Byte/character
//! indices and game-grid coordinates are separate from display columns.

use std::{borrow::Cow, ops::Range};

use ratatui::{style::Style, text::Span};
use unicode_segmentation::UnicodeSegmentation;

pub(crate) fn width(text: &str) -> usize {
    Span::raw(text).width()
}

/// For a deliberately individual codepoint (for example a grid's tile).
pub(crate) fn char_width(ch: char) -> usize {
    let mut bytes = [0; 4];
    width(ch.encode_utf8(&mut bytes))
}

pub(crate) fn graphemes(text: &str) -> impl Iterator<Item = &str> {
    text.graphemes(true)
}

/// Longest whole-grapheme prefix fitting in `columns` cells.
pub(crate) fn clip(text: &str, columns: usize) -> String {
    if columns == 0 {
        return String::new();
    }
    let mut used = 0;
    graphemes(text)
        .take_while(|symbol| {
            used += width(symbol);
            used <= columns
        })
        .collect()
}

pub(crate) fn ellipsize(text: &str, columns: usize) -> String {
    truncate_with(text, columns, "…")
}

pub(crate) fn truncate_with(text: &str, columns: usize, marker: &str) -> String {
    if columns == 0 {
        return String::new();
    }
    if width(text) <= columns {
        return text.to_string();
    }
    let marker = clip(marker, columns);
    let mut out = clip(text, columns.saturating_sub(width(&marker)));
    out.push_str(&marker);
    out
}

/// Pad to a minimum width; longer strings are retained.
pub(crate) fn pad_right(text: &str, columns: usize) -> String {
    format!("{text}{}", " ".repeat(columns.saturating_sub(width(text))))
}

pub(crate) fn pad_left(text: &str, columns: usize) -> String {
    format!("{}{text}", " ".repeat(columns.saturating_sub(width(text))))
}

pub(crate) fn pad_center(text: &str, columns: usize) -> String {
    let padding = columns.saturating_sub(width(text));
    format!(
        "{}{text}{}",
        " ".repeat(padding / 2),
        " ".repeat(padding - padding / 2)
    )
}

pub(crate) fn ellipsize_tail(text: &str, columns: usize) -> String {
    if columns == 0 {
        return String::new();
    }
    if width(text) <= columns {
        return text.to_string();
    }
    let mut room = columns - 1;
    let symbols: Vec<_> = graphemes(text).collect();
    let mut start = text.len();
    for symbol in symbols.into_iter().rev() {
        let cells = width(symbol);
        if cells > room {
            break;
        }
        room -= cells;
        start -= symbol.len();
    }
    format!("…{}", &text[start..])
}

/// Clip and pad to an exact field width.
pub(crate) fn fit(text: &str, columns: usize) -> String {
    pad_right(&clip(text, columns), columns)
}

/// A fixed-width column window, replacing cut portions of wide graphemes with
/// spaces. A window starting inside an emoji never emits its trailing selector.
pub(crate) fn window(text: &str, offset: usize, columns: usize) -> String {
    let end = offset.saturating_add(columns);
    let mut column = 0;
    let mut out = String::new();
    for symbol in graphemes(text) {
        let start = column;
        column += width(symbol);
        if start >= end {
            break;
        }
        if start >= offset && column <= end {
            out.push_str(symbol);
        } else if start < end && column > offset {
            out.push_str(&" ".repeat(column.min(end) - start.max(offset)));
        }
    }
    pad_right(&out, columns)
}

/// Byte ranges for soft-wrapped rows. Callers retain their source-index units
/// and whitespace policy; a grapheme too wide even for an empty row is skipped
/// in the display, as it cannot be painted in that row by Ratatui.
pub(crate) fn wrap_ranges(
    text: &str,
    first_width: usize,
    continuation_width: usize,
    trim_start: bool,
) -> Vec<Range<usize>> {
    let symbols: Vec<_> = text.grapheme_indices(true).collect();
    let mut rows = Vec::new();
    let mut start = 0;
    while start < symbols.len() {
        let columns = if rows.is_empty() {
            first_width
        } else {
            continuation_width
        };
        if columns == 0 {
            if rows.is_empty() && continuation_width > 0 {
                rows.push(0..0);
                continue;
            }
            return rows;
        }
        let mut end = start;
        let mut used = 0;
        while end < symbols.len() {
            let next = width(symbols[end].1);
            if used + next > columns {
                break;
            }
            used += next;
            end += 1;
        }
        if end == start {
            rows.push(symbols[start].0..symbols[start].0);
            if rows.len() == 1 && continuation_width > columns {
                continue;
            }
            start += 1;
            continue;
        }
        if end < symbols.len()
            && let Some(space) = (start..end).rfind(|&i| symbols[i].1 == " ")
        {
            // Include the separating space, matching the existing markdown
            // wrappers. Callers that trim rows do so after retaining offsets.
            end = space + 1;
        }
        let byte_end = symbols.get(end).map_or(text.len(), |(offset, _)| *offset);
        rows.push(symbols[start].0..byte_end);
        start = end;
        if trim_start {
            while start < symbols.len() && symbols[start].1 == " " {
                start += 1;
            }
        }
    }
    rows
}

pub(crate) struct StyledGrapheme<'a> {
    pub symbol: Cow<'a, str>,
    pub style: Style,
}

/// Segment after joining spans, so a selector in the next span stays with its
/// base. The base character's style owns the complete grapheme.
pub(crate) fn styled_graphemes<'a>(spans: &'a [Span<'_>]) -> Vec<StyledGrapheme<'a>> {
    let text: String = spans.iter().map(|span| span.content.as_ref()).collect();
    let mut boundaries = Vec::with_capacity(spans.len());
    let mut end = 0;
    for span in spans {
        end += span.content.len();
        boundaries.push((end, span.style));
    }
    let mut index = 0;
    let mut start = 0;
    text.grapheme_indices(true)
        .map(|(offset, symbol)| {
            while index + 1 < boundaries.len() && offset >= boundaries[index].0 {
                start = boundaries[index].0;
                index += 1;
            }
            StyledGrapheme {
                symbol: if offset + symbol.len() <= boundaries[index].0 {
                    Cow::Borrowed(
                        &spans[index].content[offset - start..offset - start + symbol.len()],
                    )
                } else {
                    Cow::Owned(symbol.to_string())
                },
                style: boundaries[index].1,
            }
        })
        .collect()
}

pub(crate) fn push_span(spans: &mut Vec<Span<'static>>, symbol: &str, style: Style) {
    if let Some(last) = spans.last_mut().filter(|span| span.style == style) {
        last.content.to_mut().push_str(symbol);
    } else if !symbol.is_empty() {
        spans.push(Span::styled(symbol.to_string(), style));
    }
}

pub(crate) fn clip_spans(spans: Vec<Span<'static>>, columns: usize) -> Vec<Span<'static>> {
    clipped_spans(spans, columns, true)
}

pub(crate) fn clip_spans_plain(spans: Vec<Span<'static>>, columns: usize) -> Vec<Span<'static>> {
    clipped_spans(spans, columns, false)
}

fn clipped_spans(spans: Vec<Span<'static>>, columns: usize, ellipsis: bool) -> Vec<Span<'static>> {
    if columns == 0 {
        return Vec::new();
    }
    let symbols = styled_graphemes(&spans);
    let total: usize = symbols.iter().map(|g| width(&g.symbol)).sum();
    let truncated = ellipsis && total > columns;
    let limit = columns.saturating_sub(usize::from(truncated));
    let mut out = Vec::new();
    let mut used = 0;
    for g in symbols {
        let next = width(&g.symbol);
        if used + next > limit {
            if truncated {
                push_span(&mut out, "…", g.style);
            }
            break;
        }
        push_span(&mut out, &g.symbol, g.style);
        used += next;
    }
    out
}
