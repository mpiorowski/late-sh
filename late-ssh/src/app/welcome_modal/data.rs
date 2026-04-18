use chrono_tz::TZ_VARIANTS;

#[derive(Clone, Copy, Debug)]
pub struct CountryOption {
    pub code: &'static str,
    pub name: &'static str,
}

pub const COUNTRIES: &[CountryOption] = &[
    CountryOption { code: "AR", name: "Argentina" },
    CountryOption { code: "AU", name: "Australia" },
    CountryOption { code: "AT", name: "Austria" },
    CountryOption { code: "BE", name: "Belgium" },
    CountryOption { code: "BR", name: "Brazil" },
    CountryOption { code: "BG", name: "Bulgaria" },
    CountryOption { code: "CA", name: "Canada" },
    CountryOption { code: "CL", name: "Chile" },
    CountryOption { code: "CN", name: "China" },
    CountryOption { code: "CO", name: "Colombia" },
    CountryOption { code: "HR", name: "Croatia" },
    CountryOption { code: "CZ", name: "Czechia" },
    CountryOption { code: "DK", name: "Denmark" },
    CountryOption { code: "EG", name: "Egypt" },
    CountryOption { code: "EE", name: "Estonia" },
    CountryOption { code: "FI", name: "Finland" },
    CountryOption { code: "FR", name: "France" },
    CountryOption { code: "DE", name: "Germany" },
    CountryOption { code: "GR", name: "Greece" },
    CountryOption { code: "HU", name: "Hungary" },
    CountryOption { code: "IS", name: "Iceland" },
    CountryOption { code: "IN", name: "India" },
    CountryOption { code: "ID", name: "Indonesia" },
    CountryOption { code: "IE", name: "Ireland" },
    CountryOption { code: "IL", name: "Israel" },
    CountryOption { code: "IT", name: "Italy" },
    CountryOption { code: "JP", name: "Japan" },
    CountryOption { code: "KE", name: "Kenya" },
    CountryOption { code: "LV", name: "Latvia" },
    CountryOption { code: "LT", name: "Lithuania" },
    CountryOption { code: "LU", name: "Luxembourg" },
    CountryOption { code: "MY", name: "Malaysia" },
    CountryOption { code: "MX", name: "Mexico" },
    CountryOption { code: "MA", name: "Morocco" },
    CountryOption { code: "NL", name: "Netherlands" },
    CountryOption { code: "NZ", name: "New Zealand" },
    CountryOption { code: "NG", name: "Nigeria" },
    CountryOption { code: "NO", name: "Norway" },
    CountryOption { code: "PK", name: "Pakistan" },
    CountryOption { code: "PE", name: "Peru" },
    CountryOption { code: "PH", name: "Philippines" },
    CountryOption { code: "PL", name: "Poland" },
    CountryOption { code: "PT", name: "Portugal" },
    CountryOption { code: "RO", name: "Romania" },
    CountryOption { code: "RS", name: "Serbia" },
    CountryOption { code: "SG", name: "Singapore" },
    CountryOption { code: "SK", name: "Slovakia" },
    CountryOption { code: "SI", name: "Slovenia" },
    CountryOption { code: "ZA", name: "South Africa" },
    CountryOption { code: "KR", name: "South Korea" },
    CountryOption { code: "ES", name: "Spain" },
    CountryOption { code: "SE", name: "Sweden" },
    CountryOption { code: "CH", name: "Switzerland" },
    CountryOption { code: "TW", name: "Taiwan" },
    CountryOption { code: "TH", name: "Thailand" },
    CountryOption { code: "TR", name: "Turkey" },
    CountryOption { code: "UA", name: "Ukraine" },
    CountryOption { code: "AE", name: "United Arab Emirates" },
    CountryOption { code: "GB", name: "United Kingdom" },
    CountryOption { code: "US", name: "United States" },
    CountryOption { code: "UY", name: "Uruguay" },
    CountryOption { code: "VN", name: "Vietnam" },
];

pub fn country_flag(code: &str) -> Option<String> {
    let code = code.trim();
    if code.len() != 2 || !code.is_ascii() {
        return None;
    }

    let mut flag = String::new();
    for ch in code.chars() {
        let upper = ch.to_ascii_uppercase();
        if !upper.is_ascii_uppercase() {
            return None;
        }
        let value = 0x1F1E6 + (upper as u32 - 'A' as u32);
        flag.push(char::from_u32(value)?);
    }
    Some(flag)
}

pub fn country_label(code: Option<&str>) -> String {
    let Some(code) = code else {
        return "Not set".to_string();
    };
    let normalized = code.trim().to_ascii_uppercase();
    let name = COUNTRIES
        .iter()
        .find(|country| country.code == normalized)
        .map(|country| country.name)
        .unwrap_or("Unknown");
    match country_flag(&normalized) {
        Some(flag) => format!("{flag} {name}"),
        None => name.to_string(),
    }
}

pub fn filter_countries(query: &str) -> Vec<&'static CountryOption> {
    let query = query.trim().to_ascii_lowercase();
    COUNTRIES
        .iter()
        .filter(|country| {
            query.is_empty()
                || country.code.to_ascii_lowercase().contains(&query)
                || country.name.to_ascii_lowercase().contains(&query)
        })
        .collect()
}

pub fn filter_timezones(query: &str) -> Vec<&'static str> {
    let query = query.trim().to_ascii_lowercase();
    TZ_VARIANTS
        .iter()
        .map(|tz| tz.name())
        .filter(|name| query.is_empty() || name.to_ascii_lowercase().contains(&query))
        .collect()
}
