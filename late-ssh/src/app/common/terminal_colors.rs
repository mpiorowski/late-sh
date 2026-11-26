//! The terminal's own default colors. The session asks for them with OSC 10
//! (foreground) and OSC 11 (background) ([`QUERY`], in the alt-screen setup
//! and again whenever they may have changed), and turns on DEC mode 2031, so
//! the terminal says when it switches palettes (`CSI ? 997 ; Ps n`, a
//! light/dark follow). The only reader is the selection fill of a palette
//! whose canvas is the terminal's own (`theme::selection_style`).
//!
//! Answers and notices are the terminal talking, not the user, so
//! `App::handle_input` takes them out of every chunk ([`strip_reports`])
//! before anything else reads it: they never reach a door game as keys,
//! count as a person for the away clock, or dismiss the splash. A report
//! split across two SSH reads slips through as input, the way a split
//! latency probe reply does (`terminal_probe.rs`): its color is lost until
//! the next query, and the away clock, which joins the halves back together,
//! takes it for a person and wakes an away session. The terminal writes each
//! one in a single go, so that takes a read boundary inside it.

use std::borrow::Cow;

/// OSC 10 and OSC 11 queries, ST-terminated.
pub(crate) const QUERY: &[u8] = b"\x1b]10;?\x1b\\\x1b]11;?\x1b\\";

/// The longest color spec looked at, `rgba:ffff/ffff/ffff/ffff` and slack.
/// Bounds the search for a terminator, so input that opens a reply and
/// never closes it costs a few bytes a position, not the rest of the chunk.
const MAX_SPEC: usize = 32;

pub(crate) type Rgb = (u8, u8, u8);

/// The terminal's own default foreground and background, as it answered the
/// OSC 10/11 queries; each `None` until (or unless) it answers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct TerminalColors {
    pub(crate) fg: Option<Rgb>,
    pub(crate) bg: Option<Rgb>,
}

/// What the reports stripped from one chunk said.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Reports {
    pub(crate) fg: Option<Rgb>,
    pub(crate) bg: Option<Rgb>,
    /// A DEC 2031 notice: the colors held are stale, ask again.
    pub(crate) palette_changed: bool,
}

/// `data` without its complete OSC 10/11 replies and DEC 2031 notices, and
/// what they said. An OSC 10/11 reply whose color does not parse is still
/// the terminal's, so it is dropped and reports nothing.
pub(crate) fn strip_reports(data: &[u8]) -> (Cow<'_, [u8]>, Reports) {
    let mut reports = Reports::default();
    if !data.windows(3).any(|w| w == b"\x1b]1" || w == b"\x1b[?") {
        return (Cow::Borrowed(data), reports);
    }
    let mut input = Vec::with_capacity(data.len());
    let mut stripped = false;
    let mut rest = data;
    while let Some(&byte) = rest.first() {
        if let Some((len, background, rgb)) = color_reply(rest) {
            match (background, rgb) {
                (false, Some(rgb)) => reports.fg = Some(rgb),
                (true, Some(rgb)) => reports.bg = Some(rgb),
                (_, None) => {}
            }
            stripped = true;
            rest = &rest[len..];
        } else if let Some(len) = palette_notice(rest) {
            reports.palette_changed = true;
            stripped = true;
            rest = &rest[len..];
        } else {
            input.push(byte);
            rest = &rest[1..];
        }
    }
    match stripped {
        true => (Cow::Owned(input), reports),
        false => (Cow::Borrowed(data), reports),
    }
}

/// Queues [`QUERY`] unless one is already waiting to be written: however
/// many notices arrive between two frames, the terminal is asked once.
pub(crate) fn queue_query(pending: &mut Vec<Vec<u8>>) {
    if !pending.iter().any(|command| command == QUERY) {
        pending.push(QUERY.to_vec());
    }
}

/// A complete `OSC 1x ; <spec> (BEL | ST)` reply at the start of `data`:
/// its length, whether it is the background, and its color if it parses.
fn color_reply(data: &[u8]) -> Option<(usize, bool, Option<Rgb>)> {
    let background = match data.get(..5)? {
        b"\x1b]10;" => false,
        b"\x1b]11;" => true,
        _ => return None,
    };
    let body = &data[5..data.len().min(5 + MAX_SPEC + 2)];
    let bel = body.iter().position(|&b| b == 0x07).map(|at| (at, 1));
    let st = body
        .windows(2)
        .position(|w| w == b"\x1b\\")
        .map(|at| (at, 2));
    let (end, terminator) = match (bel, st) {
        (Some(a), Some(b)) => a.min(b),
        (Some(a), None) | (None, Some(a)) => a,
        (None, None) => return None,
    };
    Some((5 + end + terminator, background, parse_spec(&body[..end])))
}

/// `rgb:R/G/B` with one to four hex digits a channel (XParseColor), or
/// rxvt-unicode's `rgba:R/G/B/A`, whose alpha is ignored.
fn parse_spec(spec: &[u8]) -> Option<Rgb> {
    let spec = std::str::from_utf8(spec).ok()?;
    let (channels, count) = match spec.strip_prefix("rgba:") {
        Some(channels) => (channels, 4),
        None => (spec.strip_prefix("rgb:")?, 3),
    };
    let parts: Vec<&str> = channels.split('/').collect();
    if parts.len() != count {
        return None;
    }
    Some((channel(parts[0])?, channel(parts[1])?, channel(parts[2])?))
}

/// One channel scaled to 8 bits: `ffff`, `ff`, `f` are all 255.
fn channel(hex: &str) -> Option<u8> {
    if !(1..=4).contains(&hex.len()) || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let value = u32::from_str_radix(hex, 16).ok()?;
    let max = (1u32 << (4 * hex.len())) - 1;
    Some(((value * 255 + max / 2) / max) as u8)
}

/// A complete `CSI ? 997 ; Ps n` notice at the start of `data`: its length.
fn palette_notice(data: &[u8]) -> Option<usize> {
    let body = data.strip_prefix(b"\x1b[?997;")?;
    let digits = body
        .iter()
        .take(3)
        .take_while(|b| b.is_ascii_digit())
        .count();
    (digits > 0 && body.get(digits) == Some(&b'n')).then_some(7 + digits + 1)
}
