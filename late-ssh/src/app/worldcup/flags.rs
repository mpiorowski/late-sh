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
