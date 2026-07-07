//! Country name → Unicode flag emoji for the World Cup HUD.
//!
//! FotMob's group and bracket payloads label teams with full English country
//! names (e.g. "South Korea", "Ivory Coast"), so the lookup is keyed on those
//! exact strings. Unknown names (including knockout placeholders like
//! "Winner SF 1") return `""` so callers can render without a flag.

/// Returns the flag emoji for a full country name, or `""` if unknown.
pub fn flag_emoji(name: &str) -> &'static str {
    match name.trim() {
        "Algeria" => "🇩🇿",
        "Argentina" => "🇦🇷",
        "Australia" => "🇦🇺",
        "Austria" => "🇦🇹",
        "Belgium" => "🇧🇪",
        "Bosnia and Herzegovina" => "🇧🇦",
        "Brazil" => "🇧🇷",
        "Canada" => "🇨🇦",
        "Cape Verde" => "🇨🇻",
        "Colombia" => "🇨🇴",
        "Croatia" => "🇭🇷",
        "Curacao" | "Curaçao" => "🇨🇼",
        "Czechia" | "Czech Republic" => "🇨🇿",
        "DR Congo" => "🇨🇩",
        "Ecuador" => "🇪🇨",
        "Egypt" => "🇪🇬",
        "England" => "🏴󠁧󠁢󠁥󠁮󠁧󠁿",
        "France" => "🇫🇷",
        "Germany" => "🇩🇪",
        "Ghana" => "🇬🇭",
        "Haiti" => "🇭🇹",
        "Iran" => "🇮🇷",
        "Iraq" => "🇮🇶",
        "Ivory Coast" => "🇨🇮",
        "Japan" => "🇯🇵",
        "Jordan" => "🇯🇴",
        "Mexico" => "🇲🇽",
        "Morocco" => "🇲🇦",
        "Netherlands" => "🇳🇱",
        "New Zealand" => "🇳🇿",
        "Norway" => "🇳🇴",
        "Panama" => "🇵🇦",
        "Paraguay" => "🇵🇾",
        "Portugal" => "🇵🇹",
        "Qatar" => "🇶🇦",
        "Saudi Arabia" => "🇸🇦",
        "Scotland" => "🏴󠁧󠁢󠁳󠁣󠁴󠁿",
        "Senegal" => "🇸🇳",
        "South Africa" => "🇿🇦",
        "South Korea" => "🇰🇷",
        "Spain" => "🇪🇸",
        "Sweden" => "🇸🇪",
        "Switzerland" => "🇨🇭",
        "Tunisia" => "🇹🇳",
        "Turkiye" | "Turkey" | "Türkiye" => "🇹🇷",
        "USA" | "United States" => "🇺🇸",
        "Uruguay" => "🇺🇾",
        "Uzbekistan" => "🇺🇿",
        _ => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_names_map_to_flags() {
        assert_eq!(flag_emoji("Mexico"), "🇲🇽");
        assert_eq!(flag_emoji("South Korea"), "🇰🇷");
        assert_eq!(flag_emoji("Ivory Coast"), "🇨🇮");
    }

    #[test]
    fn handles_whitespace_and_aliases() {
        assert_eq!(flag_emoji("  Germany  "), "🇩🇪");
        assert_eq!(flag_emoji("Czech Republic"), "🇨🇿");
        assert_eq!(flag_emoji("United States"), "🇺🇸");
    }

    #[test]
    fn unknown_and_placeholders_return_empty() {
        assert_eq!(flag_emoji("Winner SF 1"), "");
        assert_eq!(flag_emoji("Atlantis"), "");
    }
}
