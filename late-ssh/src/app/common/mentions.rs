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

/// Whether `body` mentions `username_lower`. Uses the same mention parser as
/// the notification path, so `@Alice` matches the user `alice`, `@alicebob`
/// does not, and a mention inside a code span does not count.
pub(crate) fn mentions_user(body: &str, username_lower: Option<&str>) -> bool {
    let Some(username_lower) = username_lower else {
        return false;
    };
    extract_mentions(body)
        .iter()
        .any(|mentioned| mentioned == username_lower)
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

        if !in_code
            && ch == '@'
            && valid_mention_start(body, idx)
            && let Some(end) = mention_end(body, idx)
        {
            let username = body[idx + 1..end].to_ascii_lowercase();
            if !usernames.contains(&username) {
                usernames.push(username);
            }
            idx = end;
            continue;
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

        if !in_code
            && ch == '@'
            && valid_mention_start(text, idx)
            && let Some(end) = mention_end(text, idx)
        {
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

/// Where the handle of the mention whose `@` sits at byte offset `at`
/// ends, or `None` when no handle follows. Trailing dots are the
/// sentence's, not the handle's: a username never ends in one
/// (`sanitize_username_input`), so "thanks @alice." mentions alice.
fn mention_end(text: &str, at: usize) -> Option<usize> {
    let start = at + '@'.len_utf8();
    let len = text[start..]
        .find(|c: char| !is_mention_char(c))
        .unwrap_or(text.len() - start);
    let handle = text[start..start + len].trim_end_matches('.');
    match handle.is_empty() {
        true => None,
        false => Some(start + handle.len()),
    }
}

fn advance_past_backticks(text: &str, mut idx: usize) -> usize {
    while idx < text.len() && text[idx..].starts_with('`') {
        idx += '`'.len_utf8();
    }
    idx
}
