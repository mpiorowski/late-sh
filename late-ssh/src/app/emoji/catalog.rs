use super::nerd_fonts;

/// A single icon entry: the icon string, display name, and a precomputed
/// lowercase name for fast case-insensitive search.
#[derive(Clone)]
pub struct IconEntry {
    pub icon: String,
    pub name: String,
    pub name_lower: String,
}

/// A view over a section's entries: either a borrowed slice (unfiltered) or a
/// Vec of references (filtered by a search query). Either way no entry data is
/// cloned; the Unicode "All" set (~100k rows) is touched without allocation.
pub enum SectionEntries<'a> {
    Full(&'a [IconEntry]),
    Filtered(Vec<&'a IconEntry>),
}

impl<'a> SectionEntries<'a> {
    pub fn len(&self) -> usize {
        match self {
            Self::Full(s) => s.len(),
            Self::Filtered(v) => v.len(),
        }
    }

    pub fn get(&self, i: usize) -> Option<&'a IconEntry> {
        match self {
            Self::Full(s) => s.get(i),
            Self::Filtered(v) => v.get(i).copied(),
        }
    }
}

pub struct SectionView<'a> {
    pub title: &'static str,
    pub entries: SectionEntries<'a>,
}

/// Pre-built icon catalog data, held on App for the lifetime of the process.
pub struct IconCatalogData {
    emoji_common: Vec<IconEntry>,
    emoji_all: Vec<IconEntry>,
    nerd_common: Vec<IconEntry>,
    nerd_all: Vec<IconEntry>,
    unicode_common: Vec<IconEntry>,
    unicode_all: Vec<IconEntry>,
}

/// Common emoji for the "Common" section — curated for chat use.
const COMMON_EMOJI: &[&str] = &[
    "🌱",
    "🔧",
    "⚡",
    "⭐",
    "✨",
    "🔥",
    "💎",
    "🤖",
    "🎯",
    "🚀",
    "📁",
    "🌿",
    "📊",
    "💰",
    "⏱\u{fe0f}",
    "🎨",
    "💡",
    "🔒",
];

/// Common nerd font glyph name prefixes to match.
const COMMON_NERD_NAMES: &[&str] = &[
    "cod hubot",        // robot
    "md folder",        // folder
    "md git",           // git
    "oct zap",          // zap/lightning
    "md chart bar",     // chart
    "cod credit card",  // cost
    "md timer",         // timer
    "md target",        // target
    "md rocket launch", // rocket
    "seti code",        // code
];

/// Common Unicode symbols.
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

impl IconCatalogData {
    /// Build the full catalog. Called once on first picker open.
    pub fn load() -> Self {
        let emoji_common = build_emoji_common();
        let emoji_all = build_emoji_all();
        let nerd_all_raw = nerd_fonts::load();
        let (nerd_common, nerd_all) = build_nerd_sections(&nerd_all_raw);
        let unicode_common = build_unicode_common();
        let unicode_all = build_unicode_all();

        Self {
            emoji_common,
            emoji_all,
            nerd_common,
            nerd_all,
            unicode_common,
            unicode_all,
        }
    }

    /// Return a borrowed, non-allocating view over the sections for a given
    /// tab and search query. Unfiltered (empty query) returns slice views;
    /// filtered returns a Vec of references — no entry data is cloned either
    /// way.
    pub fn sections(&self, tab: IconPickerTab, query: &str) -> Vec<SectionView<'_>> {
        let query_lower = query.to_lowercase();
        match tab {
            IconPickerTab::Emoji => filter_two_sections(
                "Common Emoji",
                &self.emoji_common,
                "All Emoji",
                &self.emoji_all,
                &query_lower,
            ),
            IconPickerTab::NerdFont => filter_two_sections(
                "Common",
                &self.nerd_common,
                "All Nerd Font",
                &self.nerd_all,
                &query_lower,
            ),
            IconPickerTab::Unicode => filter_two_sections(
                "Common",
                &self.unicode_common,
                "All Unicode",
                &self.unicode_all,
                &query_lower,
            ),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconPickerTab {
    Emoji,
    NerdFont,
    Unicode,
}

// --- builders ---

fn make_entry(icon: String, name: String) -> IconEntry {
    let name_lower = name.to_lowercase();
    IconEntry {
        icon,
        name,
        name_lower,
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

fn build_nerd_sections(all: &[nerd_fonts::NerdFontGlyph]) -> (Vec<IconEntry>, Vec<IconEntry>) {
    let common: Vec<IconEntry> = COMMON_NERD_NAMES
        .iter()
        .filter_map(|prefix| {
            all.iter()
                .find(|g| g.name == *prefix)
                .map(|g| make_entry(g.icon.clone(), g.name.clone()))
        })
        .collect();

    let all_entries: Vec<IconEntry> = all
        .iter()
        .map(|g| make_entry(g.icon.clone(), g.name.clone()))
        .collect();

    (common, all_entries)
}

fn build_unicode_common() -> Vec<IconEntry> {
    COMMON_UNICODE
        .iter()
        .map(|(icon, name)| make_entry(icon.to_string(), name.to_string()))
        .collect()
}

fn build_unicode_all() -> Vec<IconEntry> {
    let mut entries = Vec::new();
    for code in 0u32..=0x10FFFF {
        // Skip surrogates
        if (0xD800..=0xDFFF).contains(&code) {
            continue;
        }
        if let Some(ch) = char::from_u32(code)
            && let Some(name) = unicode_names2::name(ch)
        {
            let name_str = name.to_string();
            // Skip control characters and uninteresting blocks
            if name_str.starts_with('<') {
                continue;
            }
            entries.push(make_entry(ch.to_string(), name_str));
        }
    }
    entries
}

/// Build two borrowed views for a tab: unfiltered = slice views, filtered =
/// Vec-of-refs views. Empty sections are dropped.
fn filter_two_sections<'a>(
    common_title: &'static str,
    common: &'a [IconEntry],
    all_title: &'static str,
    all: &'a [IconEntry],
    query: &str,
) -> Vec<SectionView<'a>> {
    let mut sections = Vec::new();
    if query.is_empty() {
        if !common.is_empty() {
            sections.push(SectionView {
                title: common_title,
                entries: SectionEntries::Full(common),
            });
        }
        if !all.is_empty() {
            sections.push(SectionView {
                title: all_title,
                entries: SectionEntries::Full(all),
            });
        }
    } else {
        let filtered_common: Vec<&IconEntry> = common
            .iter()
            .filter(|e| e.name_lower.contains(query))
            .collect();
        let filtered_all: Vec<&IconEntry> = all
            .iter()
            .filter(|e| e.name_lower.contains(query))
            .collect();

        if !filtered_common.is_empty() {
            sections.push(SectionView {
                title: common_title,
                entries: SectionEntries::Filtered(filtered_common),
            });
        }
        if !filtered_all.is_empty() {
            sections.push(SectionView {
                title: all_title,
                entries: SectionEntries::Filtered(filtered_all),
            });
        }
    }
    sections
}
