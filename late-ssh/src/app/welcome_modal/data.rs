use chrono_tz::TZ_VARIANTS;

#[derive(Clone, Copy, Debug)]
pub struct CountryOption {
    pub code: &'static str,
    pub name: &'static str,
}

pub const COUNTRIES: &[CountryOption] = &[
    CountryOption {
        code: "AF",
        name: "Afghanistan",
    },
    CountryOption {
        code: "AX",
        name: "Åland Islands",
    },
    CountryOption {
        code: "AL",
        name: "Albania",
    },
    CountryOption {
        code: "DZ",
        name: "Algeria",
    },
    CountryOption {
        code: "AS",
        name: "American Samoa",
    },
    CountryOption {
        code: "AD",
        name: "Andorra",
    },
    CountryOption {
        code: "AO",
        name: "Angola",
    },
    CountryOption {
        code: "AI",
        name: "Anguilla",
    },
    CountryOption {
        code: "AQ",
        name: "Antarctica",
    },
    CountryOption {
        code: "AG",
        name: "Antigua and Barbuda",
    },
    CountryOption {
        code: "AR",
        name: "Argentina",
    },
    CountryOption {
        code: "AM",
        name: "Armenia",
    },
    CountryOption {
        code: "AW",
        name: "Aruba",
    },
    CountryOption {
        code: "AU",
        name: "Australia",
    },
    CountryOption {
        code: "AT",
        name: "Austria",
    },
    CountryOption {
        code: "AZ",
        name: "Azerbaijan",
    },
    CountryOption {
        code: "BS",
        name: "Bahamas",
    },
    CountryOption {
        code: "BH",
        name: "Bahrain",
    },
    CountryOption {
        code: "BD",
        name: "Bangladesh",
    },
    CountryOption {
        code: "BB",
        name: "Barbados",
    },
    CountryOption {
        code: "BY",
        name: "Belarus",
    },
    CountryOption {
        code: "BE",
        name: "Belgium",
    },
    CountryOption {
        code: "BZ",
        name: "Belize",
    },
    CountryOption {
        code: "BJ",
        name: "Benin",
    },
    CountryOption {
        code: "BM",
        name: "Bermuda",
    },
    CountryOption {
        code: "BT",
        name: "Bhutan",
    },
    CountryOption {
        code: "BO",
        name: "Bolivia",
    },
    CountryOption {
        code: "BQ",
        name: "Bonaire, Sint Eustatius and Saba",
    },
    CountryOption {
        code: "BA",
        name: "Bosnia and Herzegovina",
    },
    CountryOption {
        code: "BW",
        name: "Botswana",
    },
    CountryOption {
        code: "BV",
        name: "Bouvet Island",
    },
    CountryOption {
        code: "BR",
        name: "Brazil",
    },
    CountryOption {
        code: "IO",
        name: "British Indian Ocean Territory",
    },
    CountryOption {
        code: "BN",
        name: "Brunei Darussalam",
    },
    CountryOption {
        code: "BG",
        name: "Bulgaria",
    },
    CountryOption {
        code: "BF",
        name: "Burkina Faso",
    },
    CountryOption {
        code: "BI",
        name: "Burundi",
    },
    CountryOption {
        code: "CV",
        name: "Cabo Verde",
    },
    CountryOption {
        code: "KH",
        name: "Cambodia",
    },
    CountryOption {
        code: "CM",
        name: "Cameroon",
    },
    CountryOption {
        code: "CA",
        name: "Canada",
    },
    CountryOption {
        code: "KY",
        name: "Cayman Islands",
    },
    CountryOption {
        code: "CF",
        name: "Central African Republic",
    },
    CountryOption {
        code: "TD",
        name: "Chad",
    },
    CountryOption {
        code: "CL",
        name: "Chile",
    },
    CountryOption {
        code: "CN",
        name: "China",
    },
    CountryOption {
        code: "CX",
        name: "Christmas Island",
    },
    CountryOption {
        code: "CC",
        name: "Cocos (Keeling) Islands",
    },
    CountryOption {
        code: "CO",
        name: "Colombia",
    },
    CountryOption {
        code: "KM",
        name: "Comoros",
    },
    CountryOption {
        code: "CG",
        name: "Congo",
    },
    CountryOption {
        code: "CD",
        name: "Congo, Democratic Republic of the",
    },
    CountryOption {
        code: "CK",
        name: "Cook Islands",
    },
    CountryOption {
        code: "CR",
        name: "Costa Rica",
    },
    CountryOption {
        code: "CI",
        name: "Côte d'Ivoire",
    },
    CountryOption {
        code: "HR",
        name: "Croatia",
    },
    CountryOption {
        code: "CU",
        name: "Cuba",
    },
    CountryOption {
        code: "CW",
        name: "Curaçao",
    },
    CountryOption {
        code: "CY",
        name: "Cyprus",
    },
    CountryOption {
        code: "CZ",
        name: "Czechia",
    },
    CountryOption {
        code: "DK",
        name: "Denmark",
    },
    CountryOption {
        code: "DJ",
        name: "Djibouti",
    },
    CountryOption {
        code: "DM",
        name: "Dominica",
    },
    CountryOption {
        code: "DO",
        name: "Dominican Republic",
    },
    CountryOption {
        code: "EC",
        name: "Ecuador",
    },
    CountryOption {
        code: "EG",
        name: "Egypt",
    },
    CountryOption {
        code: "SV",
        name: "El Salvador",
    },
    CountryOption {
        code: "GQ",
        name: "Equatorial Guinea",
    },
    CountryOption {
        code: "ER",
        name: "Eritrea",
    },
    CountryOption {
        code: "EE",
        name: "Estonia",
    },
    CountryOption {
        code: "SZ",
        name: "Eswatini",
    },
    CountryOption {
        code: "ET",
        name: "Ethiopia",
    },
    CountryOption {
        code: "FK",
        name: "Falkland Islands (Malvinas)",
    },
    CountryOption {
        code: "FO",
        name: "Faroe Islands",
    },
    CountryOption {
        code: "FJ",
        name: "Fiji",
    },
    CountryOption {
        code: "FI",
        name: "Finland",
    },
    CountryOption {
        code: "FR",
        name: "France",
    },
    CountryOption {
        code: "GF",
        name: "French Guiana",
    },
    CountryOption {
        code: "PF",
        name: "French Polynesia",
    },
    CountryOption {
        code: "TF",
        name: "French Southern Territories",
    },
    CountryOption {
        code: "GA",
        name: "Gabon",
    },
    CountryOption {
        code: "GM",
        name: "Gambia",
    },
    CountryOption {
        code: "GE",
        name: "Georgia",
    },
    CountryOption {
        code: "DE",
        name: "Germany",
    },
    CountryOption {
        code: "GH",
        name: "Ghana",
    },
    CountryOption {
        code: "GI",
        name: "Gibraltar",
    },
    CountryOption {
        code: "GR",
        name: "Greece",
    },
    CountryOption {
        code: "GL",
        name: "Greenland",
    },
    CountryOption {
        code: "GD",
        name: "Grenada",
    },
    CountryOption {
        code: "GP",
        name: "Guadeloupe",
    },
    CountryOption {
        code: "GU",
        name: "Guam",
    },
    CountryOption {
        code: "GT",
        name: "Guatemala",
    },
    CountryOption {
        code: "GG",
        name: "Guernsey",
    },
    CountryOption {
        code: "GN",
        name: "Guinea",
    },
    CountryOption {
        code: "GW",
        name: "Guinea-Bissau",
    },
    CountryOption {
        code: "GY",
        name: "Guyana",
    },
    CountryOption {
        code: "HT",
        name: "Haiti",
    },
    CountryOption {
        code: "HM",
        name: "Heard Island and McDonald Islands",
    },
    CountryOption {
        code: "VA",
        name: "Holy See",
    },
    CountryOption {
        code: "HN",
        name: "Honduras",
    },
    CountryOption {
        code: "HK",
        name: "Hong Kong",
    },
    CountryOption {
        code: "HU",
        name: "Hungary",
    },
    CountryOption {
        code: "IS",
        name: "Iceland",
    },
    CountryOption {
        code: "IN",
        name: "India",
    },
    CountryOption {
        code: "ID",
        name: "Indonesia",
    },
    CountryOption {
        code: "IR",
        name: "Iran",
    },
    CountryOption {
        code: "IQ",
        name: "Iraq",
    },
    CountryOption {
        code: "IE",
        name: "Ireland",
    },
    CountryOption {
        code: "IM",
        name: "Isle of Man",
    },
    CountryOption {
        code: "IL",
        name: "Israel",
    },
    CountryOption {
        code: "IT",
        name: "Italy",
    },
    CountryOption {
        code: "JM",
        name: "Jamaica",
    },
    CountryOption {
        code: "JP",
        name: "Japan",
    },
    CountryOption {
        code: "JE",
        name: "Jersey",
    },
    CountryOption {
        code: "JO",
        name: "Jordan",
    },
    CountryOption {
        code: "KZ",
        name: "Kazakhstan",
    },
    CountryOption {
        code: "KE",
        name: "Kenya",
    },
    CountryOption {
        code: "KI",
        name: "Kiribati",
    },
    CountryOption {
        code: "KP",
        name: "Korea, Democratic People's Republic of",
    },
    CountryOption {
        code: "KR",
        name: "Korea, Republic of",
    },
    CountryOption {
        code: "KW",
        name: "Kuwait",
    },
    CountryOption {
        code: "KG",
        name: "Kyrgyzstan",
    },
    CountryOption {
        code: "LA",
        name: "Lao People's Democratic Republic",
    },
    CountryOption {
        code: "LV",
        name: "Latvia",
    },
    CountryOption {
        code: "LB",
        name: "Lebanon",
    },
    CountryOption {
        code: "LS",
        name: "Lesotho",
    },
    CountryOption {
        code: "LR",
        name: "Liberia",
    },
    CountryOption {
        code: "LY",
        name: "Libya",
    },
    CountryOption {
        code: "LI",
        name: "Liechtenstein",
    },
    CountryOption {
        code: "LT",
        name: "Lithuania",
    },
    CountryOption {
        code: "LU",
        name: "Luxembourg",
    },
    CountryOption {
        code: "MO",
        name: "Macao",
    },
    CountryOption {
        code: "MG",
        name: "Madagascar",
    },
    CountryOption {
        code: "MW",
        name: "Malawi",
    },
    CountryOption {
        code: "MY",
        name: "Malaysia",
    },
    CountryOption {
        code: "MV",
        name: "Maldives",
    },
    CountryOption {
        code: "ML",
        name: "Mali",
    },
    CountryOption {
        code: "MT",
        name: "Malta",
    },
    CountryOption {
        code: "MH",
        name: "Marshall Islands",
    },
    CountryOption {
        code: "MQ",
        name: "Martinique",
    },
    CountryOption {
        code: "MR",
        name: "Mauritania",
    },
    CountryOption {
        code: "MU",
        name: "Mauritius",
    },
    CountryOption {
        code: "YT",
        name: "Mayotte",
    },
    CountryOption {
        code: "MX",
        name: "Mexico",
    },
    CountryOption {
        code: "FM",
        name: "Micronesia",
    },
    CountryOption {
        code: "MD",
        name: "Moldova",
    },
    CountryOption {
        code: "MC",
        name: "Monaco",
    },
    CountryOption {
        code: "MN",
        name: "Mongolia",
    },
    CountryOption {
        code: "ME",
        name: "Montenegro",
    },
    CountryOption {
        code: "MS",
        name: "Montserrat",
    },
    CountryOption {
        code: "MA",
        name: "Morocco",
    },
    CountryOption {
        code: "MZ",
        name: "Mozambique",
    },
    CountryOption {
        code: "MM",
        name: "Myanmar",
    },
    CountryOption {
        code: "NA",
        name: "Namibia",
    },
    CountryOption {
        code: "NR",
        name: "Nauru",
    },
    CountryOption {
        code: "NP",
        name: "Nepal",
    },
    CountryOption {
        code: "NL",
        name: "Netherlands",
    },
    CountryOption {
        code: "NC",
        name: "New Caledonia",
    },
    CountryOption {
        code: "NZ",
        name: "New Zealand",
    },
    CountryOption {
        code: "NI",
        name: "Nicaragua",
    },
    CountryOption {
        code: "NE",
        name: "Niger",
    },
    CountryOption {
        code: "NG",
        name: "Nigeria",
    },
    CountryOption {
        code: "NU",
        name: "Niue",
    },
    CountryOption {
        code: "NF",
        name: "Norfolk Island",
    },
    CountryOption {
        code: "MK",
        name: "North Macedonia",
    },
    CountryOption {
        code: "MP",
        name: "Northern Mariana Islands",
    },
    CountryOption {
        code: "NO",
        name: "Norway",
    },
    CountryOption {
        code: "OM",
        name: "Oman",
    },
    CountryOption {
        code: "PK",
        name: "Pakistan",
    },
    CountryOption {
        code: "PW",
        name: "Palau",
    },
    CountryOption {
        code: "PS",
        name: "Palestine, State of",
    },
    CountryOption {
        code: "PA",
        name: "Panama",
    },
    CountryOption {
        code: "PG",
        name: "Papua New Guinea",
    },
    CountryOption {
        code: "PY",
        name: "Paraguay",
    },
    CountryOption {
        code: "PE",
        name: "Peru",
    },
    CountryOption {
        code: "PH",
        name: "Philippines",
    },
    CountryOption {
        code: "PN",
        name: "Pitcairn",
    },
    CountryOption {
        code: "PL",
        name: "Poland",
    },
    CountryOption {
        code: "PT",
        name: "Portugal",
    },
    CountryOption {
        code: "PR",
        name: "Puerto Rico",
    },
    CountryOption {
        code: "QA",
        name: "Qatar",
    },
    CountryOption {
        code: "RE",
        name: "Réunion",
    },
    CountryOption {
        code: "RO",
        name: "Romania",
    },
    CountryOption {
        code: "RU",
        name: "Russian Federation",
    },
    CountryOption {
        code: "RW",
        name: "Rwanda",
    },
    CountryOption {
        code: "BL",
        name: "Saint Barthélemy",
    },
    CountryOption {
        code: "SH",
        name: "Saint Helena, Ascension and Tristan da Cunha",
    },
    CountryOption {
        code: "KN",
        name: "Saint Kitts and Nevis",
    },
    CountryOption {
        code: "LC",
        name: "Saint Lucia",
    },
    CountryOption {
        code: "MF",
        name: "Saint Martin (French part)",
    },
    CountryOption {
        code: "PM",
        name: "Saint Pierre and Miquelon",
    },
    CountryOption {
        code: "VC",
        name: "Saint Vincent and the Grenadines",
    },
    CountryOption {
        code: "WS",
        name: "Samoa",
    },
    CountryOption {
        code: "SM",
        name: "San Marino",
    },
    CountryOption {
        code: "ST",
        name: "Sao Tome and Principe",
    },
    CountryOption {
        code: "SA",
        name: "Saudi Arabia",
    },
    CountryOption {
        code: "SN",
        name: "Senegal",
    },
    CountryOption {
        code: "RS",
        name: "Serbia",
    },
    CountryOption {
        code: "SC",
        name: "Seychelles",
    },
    CountryOption {
        code: "SL",
        name: "Sierra Leone",
    },
    CountryOption {
        code: "SG",
        name: "Singapore",
    },
    CountryOption {
        code: "SX",
        name: "Sint Maarten (Dutch part)",
    },
    CountryOption {
        code: "SK",
        name: "Slovakia",
    },
    CountryOption {
        code: "SI",
        name: "Slovenia",
    },
    CountryOption {
        code: "SB",
        name: "Solomon Islands",
    },
    CountryOption {
        code: "SO",
        name: "Somalia",
    },
    CountryOption {
        code: "ZA",
        name: "South Africa",
    },
    CountryOption {
        code: "GS",
        name: "South Georgia and the South Sandwich Islands",
    },
    CountryOption {
        code: "SS",
        name: "South Sudan",
    },
    CountryOption {
        code: "ES",
        name: "Spain",
    },
    CountryOption {
        code: "LK",
        name: "Sri Lanka",
    },
    CountryOption {
        code: "SD",
        name: "Sudan",
    },
    CountryOption {
        code: "SR",
        name: "Suriname",
    },
    CountryOption {
        code: "SJ",
        name: "Svalbard and Jan Mayen",
    },
    CountryOption {
        code: "SE",
        name: "Sweden",
    },
    CountryOption {
        code: "CH",
        name: "Switzerland",
    },
    CountryOption {
        code: "SY",
        name: "Syrian Arab Republic",
    },
    CountryOption {
        code: "TW",
        name: "Taiwan",
    },
    CountryOption {
        code: "TJ",
        name: "Tajikistan",
    },
    CountryOption {
        code: "TZ",
        name: "Tanzania",
    },
    CountryOption {
        code: "TH",
        name: "Thailand",
    },
    CountryOption {
        code: "TL",
        name: "Timor-Leste",
    },
    CountryOption {
        code: "TG",
        name: "Togo",
    },
    CountryOption {
        code: "TK",
        name: "Tokelau",
    },
    CountryOption {
        code: "TO",
        name: "Tonga",
    },
    CountryOption {
        code: "TT",
        name: "Trinidad and Tobago",
    },
    CountryOption {
        code: "TN",
        name: "Tunisia",
    },
    CountryOption {
        code: "TR",
        name: "Turkey",
    },
    CountryOption {
        code: "TM",
        name: "Turkmenistan",
    },
    CountryOption {
        code: "TC",
        name: "Turks and Caicos Islands",
    },
    CountryOption {
        code: "TV",
        name: "Tuvalu",
    },
    CountryOption {
        code: "UG",
        name: "Uganda",
    },
    CountryOption {
        code: "UA",
        name: "Ukraine",
    },
    CountryOption {
        code: "AE",
        name: "United Arab Emirates",
    },
    CountryOption {
        code: "GB",
        name: "United Kingdom",
    },
    CountryOption {
        code: "US",
        name: "United States",
    },
    CountryOption {
        code: "UM",
        name: "United States Minor Outlying Islands",
    },
    CountryOption {
        code: "UY",
        name: "Uruguay",
    },
    CountryOption {
        code: "UZ",
        name: "Uzbekistan",
    },
    CountryOption {
        code: "VU",
        name: "Vanuatu",
    },
    CountryOption {
        code: "VE",
        name: "Venezuela",
    },
    CountryOption {
        code: "VN",
        name: "Vietnam",
    },
    CountryOption {
        code: "VG",
        name: "Virgin Islands (British)",
    },
    CountryOption {
        code: "VI",
        name: "Virgin Islands (U.S.)",
    },
    CountryOption {
        code: "WF",
        name: "Wallis and Futuna",
    },
    CountryOption {
        code: "EH",
        name: "Western Sahara",
    },
    CountryOption {
        code: "YE",
        name: "Yemen",
    },
    CountryOption {
        code: "ZM",
        name: "Zambia",
    },
    CountryOption {
        code: "ZW",
        name: "Zimbabwe",
    },
];

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
    format!("[{normalized}] {name}")
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
