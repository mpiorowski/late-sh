use std::borrow::Cow;

use unicode_width::UnicodeWidthStr;

const MARKER: &str = "||";
const MASK: char = '▒';

/// `(start, end)` byte ranges of each `||spoiler||`, markers included. The
/// text between must be non-empty, stay on one line, and not start or end
/// with whitespace, so `a || b || c` in a code snippet is not a spoiler.
fn spoiler_ranges(body: &str) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut from = 0;
    while let Some(open_rel) = body[from..].find(MARKER) {
        let open = from + open_rel;
        let inner_start = open + MARKER.len();
        let Some(close_rel) = body[inner_start..].find(MARKER) else {
            break;
        };
        let inner = &body[inner_start..inner_start + close_rel];
        let is_spoiler = !inner.is_empty()
            && !inner.contains('\n')
            && !inner.starts_with(char::is_whitespace)
            && !inner.ends_with(char::is_whitespace);
        if is_spoiler {
            let end = inner_start + close_rel + MARKER.len();
            ranges.push((open, end));
            from = end;
        } else {
            from = inner_start;
        }
    }
    ranges
}

pub(crate) fn has_spoiler(body: &str) -> bool {
    !spoiler_ranges(body).is_empty()
}

/// Each spoiler, markers included, becomes a `▒` bar as wide as its text, so
/// the hidden word's length shows but nothing else does.
pub(crate) fn mask_spoilers(body: &str) -> Cow<'_, str> {
    replace_spoilers(body, |inner| MASK.to_string().repeat(inner.width().max(1)))
}

/// A revealed spoiler reads as plain text: the markers drop, the words stay.
pub(crate) fn unwrap_spoilers(body: &str) -> Cow<'_, str> {
    replace_spoilers(body, str::to_string)
}

/// The body as one reader sees it: bars until they revealed the message, the
/// plain words after. Opening a message's link or quoting it in a picker
/// reads this, never the stored body, so neither can reach past a bar.
pub(crate) fn shown(body: &str, revealed: bool) -> Cow<'_, str> {
    match revealed {
        true => unwrap_spoilers(body),
        false => mask_spoilers(body),
    }
}

fn replace_spoilers(body: &str, with: impl Fn(&str) -> String) -> Cow<'_, str> {
    let ranges = spoiler_ranges(body);
    if ranges.is_empty() {
        return Cow::Borrowed(body);
    }
    let mut out = String::with_capacity(body.len());
    let mut last = 0;
    for (start, end) in ranges {
        out.push_str(&body[last..start]);
        out.push_str(&with(&body[start + MARKER.len()..end - MARKER.len()]));
        last = end;
    }
    out.push_str(&body[last..]);
    Cow::Owned(out)
}
