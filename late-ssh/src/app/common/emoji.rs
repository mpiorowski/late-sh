//! Display-time `:shortcode:` expansion.
//!
//! Messages are stored exactly as typed; the substitution happens on the way to
//! the screen, so nothing is rewritten under the author and a client that never
//! learned the shortcodes still shows the original text. Shortcodes come from
//! the `emojis` crate's GitHub set, which is what Discord and Slack users
//! already have in their fingers.

use std::borrow::Cow;

/// The shortcode character set: what GitHub allows between the colons. Anything
/// else ends the candidate, which is what keeps `http://host` and `10:30:00`
/// from being read as shortcodes.
fn is_shortcode_byte(byte: u8) -> bool {
    byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'+' | b'-')
}

/// Replace every known `:shortcode:` in `text` with its emoji, leaving unknown
/// ones exactly as typed: a `:hammer_time:` nobody registered should read as
/// the joke the author made, not vanish.
///
/// Borrows when there is nothing to replace, which is the overwhelmingly common
/// case for a chat line.
pub(crate) fn expand_shortcodes(text: &str) -> Cow<'_, str> {
    if !text.contains(':') {
        return Cow::Borrowed(text);
    }

    let bytes = text.as_bytes();
    let mut out: Option<String> = None;
    let mut cursor = 0;
    let mut search = 0;

    while let Some(offset) = bytes[search..].iter().position(|byte| *byte == b':') {
        let open = search + offset;
        let name_start = open + 1;
        let Some(len) = bytes[name_start..]
            .iter()
            .position(|byte| !is_shortcode_byte(*byte))
        else {
            // No terminator left in the string: nothing further can match.
            break;
        };
        let close = name_start + len;

        // An empty name (`::`) or a run that ended on something other than the
        // closing colon is not a shortcode. Resume from the character that
        // ended it, so `::smile:` still finds the real code inside it.
        if len == 0 || bytes[close] != b':' {
            search = close.max(open + 1);
            continue;
        }

        match emojis::get_by_shortcode(&text[name_start..close]) {
            Some(emoji) => {
                let out = out.get_or_insert_with(|| String::with_capacity(text.len()));
                out.push_str(&text[cursor..open]);
                out.push_str(emoji.as_str());
                cursor = close + 1;
                search = cursor;
            }
            // Unknown code: its closing colon may open the next one, as in
            // `:nope:smile:`, so resume from the colon rather than past it.
            None => search = close,
        }
    }

    match out {
        Some(mut out) => {
            out.push_str(&text[cursor..]);
            Cow::Owned(out)
        }
        None => Cow::Borrowed(text),
    }
}
