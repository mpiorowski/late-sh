use super::nerd_fonts;

#[derive(Clone)]
pub struct IconEntry {
    pub icon: String,
    pub name: String,
    pub name_lower: String,
}

pub struct IconSection {
    pub title: &'static str,
    pub entries: Vec<IconEntry>,
}

/// A filtered view over a catalog section — borrowed refs, no clones.
pub struct SectionView<'a> {
    pub title: &'static str,
    pub entries: Vec<&'a IconEntry>,
}

pub struct IconCatalogData {
    sections: Vec<IconSection>,
}

const COMMON_EMOJI: &[&str] = &[
    "👍",
    "👎",
    "🙏",
    "🙌",
    "🙋",
    "🐐",
    "😂",
    "🫡",
    "👀",
    "💀",
    "🎉",
    "🤝",
    "❤\u{fe0f}",
    "✅",
    "🔥",
    "⚡",
    "🚀",
    "🤔",
    "🫠",
    "🌱",
    "🤖",
    "🔧",
    "💎",
    "⭐",
    "🎯",
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

impl IconCatalogData {
    pub fn load() -> Self {
        let emoji_common = build_emoji_common();
        let emoji_all = build_emoji_all();
        let nerd_all_raw = nerd_fonts::load();
        let (nerd_common, nerd_all) = build_nerd_sections(&nerd_all_raw);

        let sections = vec![
            IconSection {
                title: "Common Emoji",
                entries: emoji_common,
            },
            IconSection {
                title: "All Emoji",
                entries: emoji_all,
            },
            IconSection {
                title: "Common Nerd Font",
                entries: nerd_common,
            },
            IconSection {
                title: "All Nerd Font",
                entries: nerd_all,
            },
        ];

        Self { sections }
    }

    /// Return borrowed section views for the current query. Empty sections
    /// (e.g. all entries filtered out) are dropped so headers don't appear
    /// alone.
    pub fn filtered(&self, query: &str) -> Vec<SectionView<'_>> {
        let query_lower = query.to_lowercase();
        self.sections
            .iter()
            .filter_map(|section| {
                let entries: Vec<&IconEntry> = if query_lower.is_empty() {
                    section.entries.iter().collect()
                } else {
                    section
                        .entries
                        .iter()
                        .filter(|e| e.name_lower.contains(&query_lower))
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
}

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
