//! Drunk text: the tavern's buzz bleeding into what a patron types.
//!
//! [`slur`] is pure and deterministic given its seed. The chat send path owns
//! the drunk level and the seed, so the transform itself stays trivially
//! testable and never touches I/O.
//!
//! Readability rests on one rule: a word's first and last characters are never
//! touched. Readers recognise a word by its shape, not letter by letter, so
//! scrambling only the interior stays legible even at the top level, which is
//! what keeps a wasted patron funny instead of unintelligible. The trailing
//! `ing` contraction is the one deliberate exception, and it reads as speech
//! rather than as damage.
//!
//! One thing a patron says is never touched at any level: the phrases in
//! [`round_phrase_spans`] that buy the house a round. Everything else here is
//! cosmetic, but that sentence is a spending authorization the bartender
//! matches literally, and the patrons most likely to buy a round are exactly
//! the ones drunk enough to have it scrambled out from under them. The list
//! lives in `late_core::models::drink_round` so the matcher and the guard can
//! never read different phrases.

use late_core::models::drink_round::round_phrase_spans;

/// Shortest word that can take a typo. Below this there is no interior left
/// once the first and last characters are off limits.
const MIN_WORD_LEN: usize = 4;

/// How far a scrambled word's interior is disturbed. Letters only ever move;
/// nothing is added or dropped, so the word keeps its length and its ends.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Depth {
    /// One adjacent interior swap: a single fumbled keystroke.
    One,
    /// One or two swaps. The word wobbles but still reads at a glance.
    Two,
    /// Every interior letter reshuffled. This is the Cambridge typoglycemia
    /// demo, and the reason it stays legible is precisely that the ends hold.
    Shuffle,
}

/// What one drunk level does to a message. Two dials: how many words get
/// scrambled, and how far each one goes. Both climb together, so levels 1-2
/// read clean, 3 takes a beat, and 4 takes real effort.
struct Intensity {
    /// Chance in 100 that each eligible word gets scrambled.
    word_percent: u32,
    /// How far a scrambled word is disturbed.
    depth: Depth,
    /// Chance in 100 that a word also slurs its speech: an interior `s`
    /// thickens to `sh`, or a trailing `ing` contracts to `in'`. This is the
    /// only effect that changes a word's letters rather than their order, so
    /// it stays rare and only turns up once a patron is properly drunk.
    slur_percent: u32,
    /// Chance in 100 that the whole message picks up a single `*hic*`.
    hiccup_percent: u32,
}

/// Levels mirror `late_core::models::drinks::drunk_level`: 0 sober through 4
/// wasted, with anything above 4 treated as wasted like `drunk_level_word`.
fn intensity_for(level: u8) -> Option<Intensity> {
    match level {
        0 => None,
        1 => Some(Intensity {
            word_percent: 6,
            depth: Depth::One,
            slur_percent: 0,
            hiccup_percent: 0,
        }),
        2 => Some(Intensity {
            word_percent: 32,
            depth: Depth::One,
            slur_percent: 0,
            hiccup_percent: 0,
        }),
        3 => Some(Intensity {
            word_percent: 60,
            depth: Depth::Two,
            slur_percent: 15,
            hiccup_percent: 0,
        }),
        _ => Some(Intensity {
            word_percent: 85,
            depth: Depth::Shuffle,
            slur_percent: 30,
            hiccup_percent: 25,
        }),
    }
}

/// `body` as the patron actually managed to type it at `level`. Level 0 is
/// returned unchanged, byte for byte.
pub(crate) fn slur(body: &str, level: u8, seed: u64) -> String {
    let Some(intensity) = intensity_for(level) else {
        return body.to_string();
    };

    let mut rng = SlurRng::new(seed);
    let mut out = String::with_capacity(body.len());
    // Everything before this offset is the untouchable quote line, so the
    // hiccup has to land after it too.
    let mut quoted_len = 0;
    for (index, line) in body.split('\n').enumerate() {
        if index > 0 {
            out.push('\n');
        }
        // A leading `> ` line is the composer's quote of someone else's
        // message; typos there would put words in their mouth.
        if index == 0 && line.trim_start().starts_with("> ") {
            out.push_str(line);
            quoted_len = out.len();
            continue;
        }
        out.push_str(&slur_line(line, &intensity, &mut rng));
    }

    if rng.percent(intensity.hiccup_percent) {
        out = with_hiccup(&out, quoted_len, &mut rng);
    }
    out
}

/// Code spans pass through untouched: this is a developer chat, and a typo
/// inside a snippet is a lie about what the code says. Splitting on the
/// backtick alternates outside/inside starting outside, so odd segments are
/// inside a span. An unbalanced backtick therefore protects the rest of the
/// line, which is the safe way to be wrong.
fn slur_line(line: &str, intensity: &Intensity, rng: &mut SlurRng) -> String {
    let mut out = String::with_capacity(line.len());
    for (index, segment) in line.split('`').enumerate() {
        if index > 0 {
            out.push('`');
        }
        match index % 2 {
            1 => out.push_str(segment),
            _ => out.push_str(&slur_segment(segment, intensity, rng)),
        }
    }
    out
}

fn slur_segment(segment: &str, intensity: &Intensity, rng: &mut SlurRng) -> String {
    let protected = round_phrase_spans(segment);
    let mut out = String::with_capacity(segment.len());
    let mut offset = 0;
    // Inclusive split keeps each token's trailing whitespace attached, so
    // spacing survives exactly as typed.
    for token in segment.split_inclusive(char::is_whitespace) {
        let end = offset + token.len();
        let inside_phrase = protected
            .iter()
            .any(|(start, stop)| offset < *stop && *start < end);
        match inside_phrase {
            true => out.push_str(token),
            false => out.push_str(&slur_token(token, intensity, rng)),
        }
        offset = end;
    }
    out
}

fn slur_token(token: &str, intensity: &Intensity, rng: &mut SlurRng) -> String {
    let Some((start, end)) = word_span(token) else {
        return token.to_string();
    };
    if !rng.percent(intensity.word_percent) {
        return token.to_string();
    }

    let mut out = String::with_capacity(token.len() + 3);
    out.push_str(&token[..start]);
    out.push_str(&scramble(&token[start..end], intensity, rng));
    out.push_str(&token[end..]);
    out
}

/// The byte range of the ASCII-letter core of `token`, or `None` when the
/// token is off limits. Protected: handles (`@mat`), room slugs (`#lounge`),
/// slash commands, URLs, the `---NEWS---` family of card markers, and anything
/// carrying non-ASCII, so CJK and emoji pass through whole rather than being
/// sliced at a byte boundary.
fn word_span(token: &str) -> Option<(usize, usize)> {
    let trimmed = token.trim_start();
    if !token.is_ascii()
        || trimmed.starts_with('@')
        || trimmed.starts_with('#')
        || trimmed.starts_with('/')
        || trimmed.starts_with("---")
        || trimmed.starts_with("www.")
        || trimmed.contains("://")
    {
        return None;
    }

    let start = token.find(|c: char| c.is_ascii_alphabetic())?;
    let end = token[start..]
        .find(|c: char| !c.is_ascii_alphabetic())
        .map(|offset| start + offset)
        .unwrap_or(token.len());
    (end - start >= MIN_WORD_LEN).then_some((start, end))
}

/// Scramble a word of at least [`MIN_WORD_LEN`] ASCII letters. The fingers
/// fumble first, then the mouth catches up: swapping runs on the word the
/// patron meant, and the optional slur reads whatever came out.
fn scramble(word: &str, intensity: &Intensity, rng: &mut SlurRng) -> String {
    let mut chars: Vec<char> = word.chars().collect();
    match intensity.depth {
        Depth::One => swap_once(&mut chars, rng),
        Depth::Two => {
            for _ in 0..1 + rng.below(2) {
                swap_once(&mut chars, rng);
            }
        }
        Depth::Shuffle => shuffle_interior(&mut chars, rng),
    }

    if rng.percent(intensity.slur_percent)
        && let Some(slurred) = slurred(&chars, rng)
    {
        return slurred;
    }
    chars.into_iter().collect()
}

/// Swap one adjacent interior pair. The pair is `at` and `at + 1`, so `at`
/// runs 1 through len-3 inclusive and neither end can be caught up in it.
/// Always available at [`MIN_WORD_LEN`], where the middle pair is the only
/// choice.
fn swap_once(chars: &mut [char], rng: &mut SlurRng) {
    let at = 1 + rng.below(chars.len() - 3);
    chars.swap(at, at + 1);
}

/// Fisher-Yates over the interior only, so every letter between the first and
/// the last lands somewhere new while both ends stay put.
fn shuffle_interior(chars: &mut [char], rng: &mut SlurRng) {
    let last_interior = chars.len() - 2;
    for at in (2..=last_interior).rev() {
        let swap_with = 1 + rng.below(at);
        chars.swap(at, swap_with);
    }
}

/// Thicken an interior `s` into `sh`, or contract a trailing `ing` to `in'`.
/// `None` when the word offers neither, which leaves it purely scrambled.
fn slurred(chars: &[char], rng: &mut SlurRng) -> Option<String> {
    let interior_s: Vec<usize> = (1..chars.len() - 1)
        .filter(|index| chars[*index].eq_ignore_ascii_case(&'s'))
        .collect();
    if !interior_s.is_empty() {
        let at = interior_s[rng.below(interior_s.len())];
        let mut out = String::with_capacity(chars.len() + 1);
        for (index, c) in chars.iter().enumerate() {
            out.push(*c);
            if index == at {
                out.push('h');
            }
        }
        return Some(out);
    }

    let word: String = chars.iter().collect();
    if chars.len() >= 5 && word.to_ascii_lowercase().ends_with("ing") {
        return Some(format!("{}in'", &word[..word.len() - 3]));
    }
    None
}

/// Drop a single `*hic*` between two words. It only ever widens a gap, so no
/// token is split, but the gap itself still has to be fair game: gaps inside a
/// code span, inside a round phrase (a hiccup mid-order would break the match
/// the same way a scramble would), or in the quoted line before `from` are off
/// limits. With nothing eligible the hiccup goes on the end.
fn with_hiccup(text: &str, from: usize, rng: &mut SlurRng) -> String {
    let protected = round_phrase_spans(text);
    let mut gaps = Vec::new();
    let mut in_code = false;
    for (index, c) in text.char_indices() {
        match c {
            '`' => in_code = !in_code,
            ' ' if !in_code
                && index >= from
                && !protected
                    .iter()
                    .any(|(start, stop)| (*start..*stop).contains(&index)) =>
            {
                gaps.push(index)
            }
            _ => {}
        }
    }

    let Some(at) = gaps.get(rng.below(gaps.len())).copied() else {
        return format!("{text} *hic*");
    };
    format!("{} *hic*{}", &text[..at], &text[at..])
}

/// xorshift64: enough randomness for typos, no new dependency, and seeded by
/// the caller so every test is deterministic.
struct SlurRng {
    state: u64,
}

impl SlurRng {
    fn new(seed: u64) -> Self {
        let state = if seed == 0 {
            0xA409_3822_299F_31D0
        } else {
            seed
        };
        Self { state }
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x
    }

    /// Uniform in `0..upper`, and `0` for an empty range.
    fn below(&mut self, upper: usize) -> usize {
        if upper == 0 {
            return 0;
        }
        (self.next_u64() % upper as u64) as usize
    }

    /// True `percent` times in 100.
    fn percent(&mut self, percent: u32) -> bool {
        if percent == 0 {
            return false;
        }
        self.below(100) < percent as usize
    }
}

#[cfg(test)]
#[path = "slur_test.rs"]
mod slur_test;
