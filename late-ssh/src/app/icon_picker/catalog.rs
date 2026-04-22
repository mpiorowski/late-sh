use std::cell::RefCell;

use unicode_names2 as unicode_names;

use super::IconPickerTab;
use super::nerd_fonts;

#[derive(Clone, Debug)]
pub struct IconEntry {
    pub icon: String,
    pub name: String,
    pub name_lower: String,
}

pub struct IconSection {
    pub title: &'static str,
    pub entries: Vec<IconEntry>,
}

/// A filtered view over a catalog section. Static tabs borrow from the
/// catalog; unicode search borrows from the cached query result.
pub struct SectionView<'a> {
    pub title: &'static str,
    pub entries: Vec<&'a IconEntry>,
}

struct CachedUnicodeQuery {
    query: String,
    sections: Vec<IconSection>,
}

pub struct IconCatalogData {
    emoji_sections: Vec<IconSection>,
    unicode_browse_sections: Vec<IconSection>,
    nerd_sections: Vec<IconSection>,
    unicode_query: RefCell<Option<CachedUnicodeQuery>>,
}

const COMMON_EMOJI: &[&str] = &[
    "👍", "👎", "🙏", "🙌", "🙋", "🐐", "😂", "🫡", "👀", "💀", "🎉", "🤝", "🧡", "✅", "🔥", "⚡",
    "🚀", "🤔", "🫠", "🌱", "🤖", "🔧", "💎", "⭐", "🎯",
];

const COMMON_NERD_NAMES: &[&str] = &[
    "cod hubot",
    "md folder",
    "md git",
    "oct zap",
    "md chart bar",
    "cod credit card",
    "md timer",
    "md target",
    "md rocket launch",
    "seti code",
];

const COMMON_UNICODE: &[(&str, &str)] = &[
    ("●", "Black Circle"),
    ("◆", "Black Diamond"),
    ("★", "Black Star"),
    ("→", "Rightwards Arrow"),
    ("│", "Box Drawings Light Vertical"),
    ("■", "Black Square"),
    ("▲", "Black Up-Pointing Triangle"),
    ("○", "White Circle"),
    ("✦", "Black Four Pointed Star"),
    ("⟩", "Mathematical Right Angle Bracket"),
    ("·", "Middle Dot"),
    ("»", "Right-Pointing Double Angle Quotation Mark"),
    ("✓", "Check Mark"),
    ("✗", "Ballot X"),
];

const UNICODE_SEARCH_LIMIT: usize = 200;

impl IconCatalogData {
    pub fn load() -> Self {
        let emoji_sections = vec![
            IconSection {
                title: "Common Emoji",
                entries: build_emoji_common(),
            },
            IconSection {
                title: "All Emoji",
                entries: build_emoji_all(),
            },
        ];

        let unicode_browse_sections = vec![
            IconSection {
                title: "Common Unicode",
                entries: build_unicode_common(),
            },
            build_unicode_range("Box Drawing", 0x2500..=0x259F),
            build_unicode_range("Geometric Shapes", 0x25A0..=0x25FF),
            build_unicode_range("Arrows", 0x2190..=0x21FF),
            build_unicode_range("Mathematical Operators", 0x2200..=0x22FF),
            build_unicode_range("Dingbats", 0x2700..=0x27BF),
        ];

        let nerd_all_raw = nerd_fonts::load();
        let (nerd_common, nerd_all) = build_nerd_sections(&nerd_all_raw);
        let nerd_sections = vec![
            IconSection {
                title: "Common Nerd Font",
                entries: nerd_common,
            },
            IconSection {
                title: "All Nerd Font",
                entries: nerd_all,
            },
        ];

        Self {
            emoji_sections,
            unicode_browse_sections,
            nerd_sections,
            unicode_query: RefCell::new(None),
        }
    }

    pub fn with_filtered<R>(
        &self,
        tab: IconPickerTab,
        query: &str,
        f: impl FnOnce(&[SectionView<'_>]) -> R,
    ) -> R {
        match tab {
            IconPickerTab::Emoji => {
                let sections = filter_sections(&self.emoji_sections, query);
                f(&sections)
            }
            IconPickerTab::Unicode => {
                let query = query.trim();
                if query.is_empty() {
                    let sections = filter_sections(&self.unicode_browse_sections, "");
                    return f(&sections);
                }

                let rebuild = self
                    .unicode_query
                    .borrow()
                    .as_ref()
                    .is_none_or(|cached| cached.query != query);
                if rebuild {
                    *self.unicode_query.borrow_mut() = Some(CachedUnicodeQuery {
                        query: query.to_string(),
                        sections: build_unicode_search_sections(query),
                    });
                }

                let cache = self.unicode_query.borrow();
                let cached = cache.as_ref().expect("unicode query cache missing");
                let sections = filter_sections(&cached.sections, "");
                f(&sections)
            }
            IconPickerTab::NerdFont => {
                let sections = filter_sections(&self.nerd_sections, query);
                f(&sections)
            }
        }
    }
}

fn filter_sections<'a>(sections: &'a [IconSection], query: &str) -> Vec<SectionView<'a>> {
    let query_lower = query.to_lowercase();
    sections
        .iter()
        .filter_map(|section| {
            let entries: Vec<&IconEntry> = if query_lower.is_empty() {
                section.entries.iter().collect()
            } else {
                section
                    .entries
                    .iter()
                    .filter(|entry| entry.name_lower.contains(&query_lower))
                    .collect()
            };
            if entries.is_empty() {
                None
            } else {
                Some(SectionView {
                    title: section.title,
                    entries,
                })
            }
        })
        .collect()
}

fn make_entry(icon: String, name: String) -> IconEntry {
    IconEntry {
        icon,
        name_lower: name.to_lowercase(),
        name,
    }
}

fn build_emoji_common() -> Vec<IconEntry> {
    COMMON_EMOJI
        .iter()
        .filter_map(|s| {
            let emoji = emojis::get(s)?;
            Some(make_entry(
                emoji.as_str().to_string(),
                emoji.name().to_string(),
            ))
        })
        .collect()
}

fn build_emoji_all() -> Vec<IconEntry> {
    emojis::iter()
        .map(|emoji| make_entry(emoji.as_str().to_string(), emoji.name().to_string()))
        .collect()
}

fn build_unicode_common() -> Vec<IconEntry> {
    COMMON_UNICODE
        .iter()
        .filter_map(|(icon, _)| icon.chars().next())
        .filter_map(make_named_unicode_entry)
        .collect()
}

fn build_unicode_range(title: &'static str, range: std::ops::RangeInclusive<u32>) -> IconSection {
    let entries = range
        .filter_map(char::from_u32)
        .filter_map(make_named_unicode_entry)
        .collect();
    IconSection { title, entries }
}

fn build_unicode_search_sections(query: &str) -> Vec<IconSection> {
    let mut sections = Vec::new();
    let mut seen = std::collections::HashSet::new();

    if let Some(ch) = resolve_unicode_query(query)
        && let Some(entry) = make_unicode_entry(ch, true)
    {
        seen.insert(ch);
        sections.push(IconSection {
            title: "Exact Match",
            entries: vec![entry],
        });
    }

    if should_scan_unicode_names(query) {
        let matches = scan_unicode_matches(query, &seen, UNICODE_SEARCH_LIMIT);
        if !matches.is_empty() {
            sections.push(IconSection {
                title: "Unicode Matches",
                entries: matches,
            });
        }
    }

    sections
}

fn resolve_unicode_query(query: &str) -> Option<char> {
    let query = query.trim();
    if query.is_empty() {
        return None;
    }

    if query.chars().count() == 1 {
        let ch = query.chars().next()?;
        if !ch.is_control() {
            return Some(ch);
        }
    }

    parse_codepoint_query(query).or_else(|| unicode_names::character(query))
}

fn should_scan_unicode_names(query: &str) -> bool {
    let query = query.trim();
    if query.is_empty() {
        return false;
    }
    if query.chars().count() == 1 {
        return false;
    }
    true
}

fn scan_unicode_matches(
    query: &str,
    seen: &std::collections::HashSet<char>,
    limit: usize,
) -> Vec<IconEntry> {
    let query_lower = query.trim().to_lowercase();
    let mut matches = Vec::new();

    for codepoint in 0..=0x10FFFF {
        let Some(ch) = char::from_u32(codepoint) else {
            continue;
        };
        if seen.contains(&ch) || ch.is_control() {
            continue;
        }
        let Some(name) = unicode_names::name(ch) else {
            continue;
        };
        let display = format!("{name} · U+{codepoint:04X}");
        if display.to_lowercase().contains(&query_lower) {
            matches.push(make_entry(ch.to_string(), display));
            if matches.len() >= limit {
                break;
            }
        }
    }

    matches
}

fn parse_codepoint_query(query: &str) -> Option<char> {
    let trimmed = query.trim();
    let hex = trimmed
        .strip_prefix("U+")
        .or_else(|| trimmed.strip_prefix("u+"))
        .or_else(|| trimmed.strip_prefix("0x"))
        .or_else(|| trimmed.strip_prefix("0X"))
        .unwrap_or(trimmed);

    if hex.is_empty() || !hex.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return None;
    }

    u32::from_str_radix(hex, 16).ok().and_then(char::from_u32)
}

fn make_named_unicode_entry(ch: char) -> Option<IconEntry> {
    make_unicode_entry(ch, false)
}

fn make_unicode_entry(ch: char, allow_unnamed: bool) -> Option<IconEntry> {
    if ch.is_control() {
        return None;
    }

    let label = match unicode_names::name(ch) {
        Some(name) => format!("{name} · U+{:04X}", ch as u32),
        None if allow_unnamed => format!("U+{:04X}", ch as u32),
        None => return None,
    };
    Some(make_entry(ch.to_string(), label))
}

fn build_nerd_sections(all: &[nerd_fonts::NerdFontGlyph]) -> (Vec<IconEntry>, Vec<IconEntry>) {
    let common: Vec<IconEntry> = COMMON_NERD_NAMES
        .iter()
        .filter_map(|prefix| {
            all.iter()
                .find(|glyph| glyph.name == *prefix)
                .map(|glyph| make_entry(glyph.icon.clone(), glyph.name.clone()))
        })
        .collect();

    let all_entries: Vec<IconEntry> = all
        .iter()
        .map(|glyph| make_entry(glyph.icon.clone(), glyph.name.clone()))
        .collect();

    (common, all_entries)
}
