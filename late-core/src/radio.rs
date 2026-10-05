//! The radio station catalogue: every station the `radio` audio source can
//! tune to, the provider behind each one, and the per-user pinned slots.
//!
//! The catalogue is code, not a table: a station is a row here, and adding
//! one is a one-row commit plus (for a new provider) a metadata adapter in
//! `late-ssh`. Rows ship `enabled: false` until the stream has been verified
//! and the provider has agreed to attribution; a disabled row is invisible
//! everywhere (settings parsing, selectors, the public listen page).
//!
//! Third-party audio is never proxied through late.sh: a `Direct` stream is
//! the provider's own URL and the client opens it itself. House streams are
//! late.sh Icecast mounts and resolve against the configured base URL.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;

/// Pinned stations per user, reachable as `v1`..`v{RADIO_SLOTS}`.
pub const RADIO_SLOTS: usize = 5;

/// Who runs a station. Carries the credit the rail's station heading shows
/// for the current station and which metadata adapter feeds its
/// now-playing row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Provider {
    /// Nightride FM. Approved as a direct-client source on the condition that
    /// late.sh credits the artists playing when it can.
    Nightride,
    /// late.sh's own Liquidsoap/Icecast mounts (CC and public-domain music).
    House,
    /// Nightwave Plaza (plaza.one), vaporwave.
    Plaza,
    /// freeCodeCamp Code Radio, lofi for coders.
    CodeRadio,
    /// Radio Paradise, listener-supported eclectic radio.
    RadioParadise,
    /// FIP, Radio France's music station and its genre webradios.
    Fip,
    /// Radio Swiss Jazz and Radio Swiss Classic, run by the Swiss public
    /// broadcaster SRG SSR.
    RadioSwiss,
}

impl Provider {
    /// Short lowercase name for station lists and the rail's station heading.
    pub fn label(self) -> &'static str {
        match self {
            Self::Nightride => "nightride",
            Self::House => "late.sh",
            Self::Plaza => "plaza.one",
            Self::CodeRadio => "freecodecamp",
            Self::RadioParadise => "paradise",
            Self::Fip => "fip",
            Self::RadioSwiss => "radio swiss",
        }
    }

    pub fn home_url(self) -> &'static str {
        match self {
            Self::Nightride => "https://nightride.fm",
            Self::House => "https://late.sh/listen",
            Self::Plaza => "https://plaza.one",
            Self::CodeRadio => "https://coderadio.freecodecamp.org",
            Self::RadioParadise => "https://radioparadise.com",
            Self::Fip => "https://www.radiofrance.fr/fip",
            Self::RadioSwiss => "https://www.srgssr.ch",
        }
    }
}

/// The group the Stations modal lists a station under: Nightride's and
/// Radio Paradise's networks first as their own sections, the rest by what
/// they play. Catalogue
/// rows of one section sit together, in the order the variants are declared.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Section {
    Nightride,
    Paradise,
    Chill,
    Jazz,
    Classical,
}

impl Section {
    pub fn label(self) -> &'static str {
        match self {
            Self::Nightride => "nightride",
            Self::Paradise => "paradise",
            Self::Chill => "chill",
            Self::Jazz => "jazz",
            Self::Classical => "classical",
        }
    }
}

/// Where a station's audio comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StationStream {
    /// An absolute URL the client opens directly. Never proxied.
    Direct(&'static str),
    /// A late.sh Icecast mount name, resolved against the house base URL.
    HouseMount(&'static str),
}

#[derive(Debug)]
pub struct Station {
    /// Settings/persistence key and the metadata-map key. Stable forever:
    /// `rektify` keys the station labelled `ambient` because Nightride's
    /// feed keys it by its stream filename, and `chill` keys the house
    /// mount labelled `lofi` because that is the mount's name.
    pub key: &'static str,
    /// Lowercase rail label.
    pub label: &'static str,
    pub provider: Provider,
    pub stream: StationStream,
    /// The group the Stations modal lists this station under.
    pub section: Section,
    /// A disabled row exists in code only; see the module docs.
    pub enabled: bool,
}

const DEFAULT_KEY: &str = "chillsynth";

// Nightride: the `.mp3` URLs, not the `.m4a` ones the site advertises. The
// `.m4a` is a 302 to `.mp3` anyway, and the CLI decoder only aligns MP3
// streams, so going direct removes the dependency on that redirect.
pub const CATALOGUE: &[Station] = &[
    Station {
        key: "chillsynth",
        label: "chillsynth",
        provider: Provider::Nightride,
        stream: StationStream::Direct("https://stream.nightride.fm/chillsynth.mp3"),
        section: Section::Nightride,
        enabled: true,
    },
    Station {
        key: "nightride",
        label: "nightride",
        provider: Provider::Nightride,
        stream: StationStream::Direct("https://stream.nightride.fm/nightride.mp3"),
        section: Section::Nightride,
        enabled: true,
    },
    Station {
        key: "datawave",
        label: "datawave",
        provider: Provider::Nightride,
        stream: StationStream::Direct("https://stream.nightride.fm/datawave.mp3"),
        section: Section::Nightride,
        enabled: true,
    },
    Station {
        key: "spacesynth",
        label: "spacesynth",
        provider: Provider::Nightride,
        stream: StationStream::Direct("https://stream.nightride.fm/spacesynth.mp3"),
        section: Section::Nightride,
        enabled: true,
    },
    Station {
        key: "rektify",
        label: "ambient",
        provider: Provider::Nightride,
        stream: StationStream::Direct("https://stream.nightride.fm/rektify.mp3"),
        section: Section::Nightride,
        enabled: true,
    },
    Station {
        key: "darksynth",
        label: "darksynth",
        provider: Provider::Nightride,
        stream: StationStream::Direct("https://stream.nightride.fm/darksynth.mp3"),
        section: Section::Nightride,
        enabled: true,
    },
    Station {
        key: "horrorsynth",
        label: "horrorsynth",
        provider: Provider::Nightride,
        stream: StationStream::Direct("https://stream.nightride.fm/horrorsynth.mp3"),
        section: Section::Nightride,
        enabled: true,
    },
    Station {
        key: "ebsm",
        label: "ebsm",
        provider: Provider::Nightride,
        stream: StationStream::Direct("https://stream.nightride.fm/ebsm.mp3"),
        section: Section::Nightride,
        enabled: true,
    },
    // Radio Paradise: the 192k MP3 streams, in the order their own channel
    // list uses. The `#.mp3` fragment is there for the CLI, which appends
    // `/stream` to any URL without an audio extension. Serenity is left out:
    // it streams AAC only, which the CLI cannot decode.
    Station {
        key: "paradise",
        label: "main mix",
        provider: Provider::RadioParadise,
        stream: StationStream::Direct("https://stream.radioparadise.com/mp3-192#.mp3"),
        section: Section::Paradise,
        enabled: true,
    },
    Station {
        key: "mellow",
        label: "mellow",
        provider: Provider::RadioParadise,
        stream: StationStream::Direct("https://stream.radioparadise.com/mellow-192#.mp3"),
        section: Section::Paradise,
        enabled: true,
    },
    Station {
        key: "rock",
        label: "rock",
        provider: Provider::RadioParadise,
        stream: StationStream::Direct("https://stream.radioparadise.com/rock-192#.mp3"),
        section: Section::Paradise,
        enabled: true,
    },
    Station {
        key: "global",
        label: "globe",
        provider: Provider::RadioParadise,
        stream: StationStream::Direct("https://stream.radioparadise.com/global-192#.mp3"),
        section: Section::Paradise,
        enabled: true,
    },
    Station {
        key: "beyond",
        label: "beyond",
        provider: Provider::RadioParadise,
        stream: StationStream::Direct("https://stream.radioparadise.com/beyond-192#.mp3"),
        section: Section::Paradise,
        enabled: true,
    },
    Station {
        key: "kfat",
        label: "kfat",
        provider: Provider::RadioParadise,
        stream: StationStream::Direct("https://stream.radioparadise.com/kfat-192#.mp3"),
        section: Section::Paradise,
        enabled: true,
    },
    Station {
        key: "coderadio",
        label: "code radio",
        provider: Provider::CodeRadio,
        stream: StationStream::Direct(
            "https://coderadio-admin-v2.freecodecamp.org/listen/coderadio/radio.mp3",
        ),
        section: Section::Chill,
        enabled: true,
    },
    Station {
        key: "plaza",
        label: "plaza",
        provider: Provider::Plaza,
        // `#.mp3` for the CLI, as on the radio paradise rows.
        stream: StationStream::Direct("https://radio.plaza.one/mp3#.mp3"),
        section: Section::Chill,
        enabled: true,
    },
    Station {
        key: "chill",
        label: "lofi",
        provider: Provider::House,
        stream: StationStream::HouseMount("chill"),
        section: Section::Chill,
        enabled: true,
    },
    // `#.mp3` for the CLI, as on the radio paradise rows. This path
    // redirects to an HTTPS node; the shorter `/m/rsj/mp3_128` one redirects
    // to plain HTTP, which the listen page cannot play.
    Station {
        key: "swissjazz",
        label: "swiss jazz",
        provider: Provider::RadioSwiss,
        stream: StationStream::Direct("https://stream.srg-ssr.ch/srgssr/rsj/mp3/128#.mp3"),
        section: Section::Jazz,
        enabled: true,
    },
    Station {
        key: "fipjazz",
        label: "fip jazz",
        provider: Provider::Fip,
        stream: StationStream::Direct("https://icecast.radiofrance.fr/fipjazz-midfi.mp3"),
        section: Section::Jazz,
        enabled: true,
    },
    Station {
        key: "swissclassic",
        label: "swiss classic",
        provider: Provider::RadioSwiss,
        stream: StationStream::Direct("https://stream.srg-ssr.ch/srgssr/rsc_de/mp3/128#.mp3"),
        section: Section::Classical,
        enabled: true,
    },
    Station {
        key: "classical",
        label: "classical",
        provider: Provider::House,
        stream: StationStream::HouseMount("classical"),
        section: Section::Classical,
        enabled: true,
    },
];

/// A handle to one enabled catalogue row. `Copy` and key-comparable so it
/// travels through settings, pair-client entries and render props the way
/// the old station enum did.
#[derive(Clone, Copy)]
pub struct RadioStation(&'static Station);

impl RadioStation {
    /// The enabled station with this key, or `None`. Strict on purpose: a
    /// disabled or unknown key must drop out, never resolve to the default
    /// (which would hand out the wrong stream).
    pub fn from_key(key: &str) -> Option<Self> {
        CATALOGUE
            .iter()
            .find(|station| station.enabled && station.key == key)
            .map(Self)
    }

    /// Settings parsing: an unknown or disabled key lands on the default
    /// station rather than failing the whole settings read.
    pub fn from_settings_str(value: &str) -> Self {
        Self::from_key(value).unwrap_or_default()
    }

    /// Every enabled station in catalogue order, which is grouped by section.
    pub fn enabled() -> impl Iterator<Item = Self> {
        CATALOGUE.iter().filter(|station| station.enabled).map(Self)
    }

    /// Settings/persistence key, also the metadata-map key.
    pub fn as_str(self) -> &'static str {
        self.0.key
    }

    pub fn label(self) -> &'static str {
        self.0.label
    }

    pub fn provider(self) -> Provider {
        self.0.provider
    }

    pub fn section(self) -> Section {
        self.0.section
    }

    pub fn stream(self) -> StationStream {
        self.0.stream
    }

    /// The URL a client opens for this station. `house_base_url` is the
    /// Icecast base (`https://late.sh/stream` for the public page, the
    /// internal base for paired clients); only house mounts use it.
    pub fn stream_url(self, house_base_url: &str) -> String {
        match self.0.stream {
            StationStream::Direct(url) => url.to_string(),
            StationStream::HouseMount(mount) => {
                format!("{}/{mount}", house_base_url.trim_end_matches('/'))
            }
        }
    }
}

impl Default for RadioStation {
    fn default() -> Self {
        Self::from_key(DEFAULT_KEY).expect("the default station is an enabled catalogue row")
    }
}

impl PartialEq for RadioStation {
    fn eq(&self, other: &Self) -> bool {
        self.0.key == other.0.key
    }
}

impl Eq for RadioStation {}

impl std::hash::Hash for RadioStation {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.key.hash(state);
    }
}

impl std::fmt::Debug for RadioStation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "RadioStation({})", self.0.key)
    }
}

impl Serialize for RadioStation {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.0.key)
    }
}

impl<'de> Deserialize<'de> for RadioStation {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let key = String::deserialize(deserializer)?;
        Ok(Self::from_settings_str(&key))
    }
}

/// A user's pinned stations: slot `i` answers `v{i+1}`. A slot can be empty;
/// a station sits in at most one slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RadioSlots([Option<RadioStation>; RADIO_SLOTS]);

impl RadioSlots {
    const DEFAULT_KEYS: [&'static str; RADIO_SLOTS] =
        ["chillsynth", "nightride", "datawave", "mellow", "plaza"];

    pub fn empty() -> Self {
        Self([None; RADIO_SLOTS])
    }

    /// Zero-based slot lookup; `None` for an empty or out-of-range slot.
    pub fn get(self, index: usize) -> Option<RadioStation> {
        self.0.get(index).copied().flatten()
    }

    pub fn iter(&self) -> impl Iterator<Item = Option<RadioStation>> + '_ {
        self.0.iter().copied()
    }

    pub fn position_of(self, station: RadioStation) -> Option<usize> {
        self.0.iter().position(|slot| *slot == Some(station))
    }

    /// Put `station` in slot `index`, vacating any other slot it held.
    /// Out-of-range indices are ignored.
    pub fn pin(&mut self, index: usize, station: RadioStation) {
        if index >= RADIO_SLOTS {
            return;
        }
        for slot in self.0.iter_mut() {
            if *slot == Some(station) {
                *slot = None;
            }
        }
        self.0[index] = Some(station);
    }

    pub fn unpin(&mut self, index: usize) {
        if let Some(slot) = self.0.get_mut(index) {
            *slot = None;
        }
    }

    /// Parse the persisted shape: a JSON array of station keys or nulls.
    /// Unknown and disabled keys read as empty slots, a repeated key keeps
    /// its first slot, and anything that is not an array is `None` so the
    /// caller can fall back to defaults.
    pub fn from_json(value: &Value) -> Option<Self> {
        let items = value.as_array()?;
        let mut slots = Self::empty();
        for (index, item) in items.iter().take(RADIO_SLOTS).enumerate() {
            let Some(station) = item.as_str().and_then(RadioStation::from_key) else {
                continue;
            };
            if slots.position_of(station).is_none() {
                slots.0[index] = Some(station);
            }
        }
        Some(slots)
    }

    pub fn to_json(self) -> Value {
        Value::Array(
            self.0
                .iter()
                .map(|slot| match slot {
                    Some(station) => Value::String(station.as_str().to_string()),
                    None => Value::Null,
                })
                .collect(),
        )
    }
}

/// The slots a user gets before pinning anything.
impl Default for RadioSlots {
    fn default() -> Self {
        Self(Self::DEFAULT_KEYS.map(RadioStation::from_key))
    }
}

#[cfg(test)]
#[path = "radio_test.rs"]
mod radio_test;
