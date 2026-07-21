use ratatui::{
    style::{Modifier, Style},
    text::Span,
};

use crate::app::common::theme;

/// Returns `true` if `c` is a valid character within a mention username.
pub(crate) fn is_mention_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.'
}

/// Returns `true` if `@` at byte offset `at` in `text` starts a valid mention
/// (i.e. it is at the beginning or preceded by a non-mention character).
pub(crate) fn valid_mention_start(text: &str, at: usize) -> bool {
    if at == 0 {
        return true;
    }

    text[..at]
        .chars()
        .next_back()
        .map(|c| !is_mention_char(c))
        .unwrap_or(true)
}

/// Extract unique usernames from `@mention`s in a message body.
/// Returns deduplicated, lowercased usernames (without the `@` prefix).
pub(crate) fn extract_mentions(body: &str) -> Vec<String> {
    let mut usernames = Vec::new();
    let mut idx = 0;
    let mut in_code = false;

    while idx < body.len() {
        let Some(ch) = body[idx..].chars().next() else {
            break;
        };

        if ch == '`' {
            idx = advance_past_backticks(body, idx);
            in_code = !in_code;
            continue;
        }

        if !in_code && ch == '@' && valid_mention_start(body, idx) {
            let mut end = idx + ch.len_utf8();
            let mut has_mention_chars = false;

            while end < body.len() {
                let Some(next) = body[end..].chars().next() else {
                    break;
                };
                if !is_mention_char(next) {
                    break;
                }
                has_mention_chars = true;
                end += next.len_utf8();
            }

            if has_mention_chars {
                let username = body[idx + 1..end].to_ascii_lowercase();
                if !usernames.contains(&username) {
                    usernames.push(username);
                }
                idx = end;
                continue;
            }
        }

        idx += ch.len_utf8();
    }

    usernames
}

/// Split `text` into spans, highlighting `@mentions` in the theme accent color.
pub(crate) fn mention_spans(text: &str, body_style: Style) -> Vec<Span<'static>> {
    let mention_style = body_style.fg(theme::MENTION()).add_modifier(Modifier::BOLD);
    let mut spans = Vec::new();
    let mut idx = 0;
    let mut segment_start = 0;
    let mut in_code = false;

    while idx < text.len() {
        let Some(ch) = text[idx..].chars().next() else {
            break;
        };

        if ch == '`' {
            idx = advance_past_backticks(text, idx);
            in_code = !in_code;
            continue;
        }

        if !in_code && ch == '@' && valid_mention_start(text, idx) {
            let mut end = idx + ch.len_utf8();
            let mut has_mention_chars = false;

            while end < text.len() {
                let Some(next) = text[end..].chars().next() else {
                    break;
                };
                if !is_mention_char(next) {
                    break;
                }
                has_mention_chars = true;
                end += next.len_utf8();
            }

            if has_mention_chars {
                if segment_start < idx {
                    spans.push(Span::styled(
                        text[segment_start..idx].to_string(),
                        body_style,
                    ));
                }
                spans.push(Span::styled(text[idx..end].to_string(), mention_style));
                idx = end;
                segment_start = end;
                continue;
            }
        }

        idx += ch.len_utf8();
    }

    if segment_start < text.len() {
        spans.push(Span::styled(
            text[segment_start..text.len()].to_string(),
            body_style,
        ));
    }

    spans
}

fn advance_past_backticks(text: &str, mut idx: usize) -> usize {
    while idx < text.len() && text[idx..].starts_with('`') {
        idx += '`'.len_utf8();
    }
    idx
}
