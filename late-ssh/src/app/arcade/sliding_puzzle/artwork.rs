use uuid::Uuid;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub(crate) enum ArtworkSource {
    Embedded(usize),
    Community(Uuid),
}

impl ArtworkSource {
    pub(crate) fn embedded(seed: u64) -> Self {
        Self::Embedded((seed % BUILTINS.len() as u64) as usize)
    }
}

pub(crate) fn embedded_index(key: &str) -> Option<usize> {
    match key {
        "night-terminal" => Some(0),
        "rooftop-garden" => Some(1),
        "night-train" => Some(2),
        _ => None,
    }
}

pub(crate) struct Artwork {
    pub title: &'static str,
    pub credit: &'static str,
    pub bytes: &'static [u8],
}

/// Built-in starter images, also seeded into the database pool by migration
/// 182. Personal boards keep selecting from this immutable roster.
pub(crate) const BUILTINS: &[Artwork] = &[
    Artwork {
        title: "Night Terminal",
        credit: "late.sh · AI illustration",
        bytes: include_bytes!("../../../../assets/sliding-puzzle/night-terminal.png"),
    },
    Artwork {
        title: "Rooftop Garden",
        credit: "late.sh · AI illustration",
        bytes: include_bytes!("../../../../assets/sliding-puzzle/rooftop-garden.png"),
    },
    Artwork {
        title: "Night Train",
        credit: "late.sh · AI illustration",
        bytes: include_bytes!("../../../../assets/sliding-puzzle/night-train.png"),
    },
];

pub(crate) fn for_key(key: u64) -> &'static Artwork {
    &BUILTINS[(key % BUILTINS.len() as u64) as usize]
}
