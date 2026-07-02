//! The Late Lounge floor plan: static ASCII furniture plus the metadata the
//! runtime needs (collision, seats, standing room, interactive zones). The
//! art is authored on a fixed grid; rows may be right-trimmed and are padded
//! back to `MAP_W` at read time.

pub const MAP_W: u16 = 94;
pub const MAP_H: u16 = 26;

pub const MAP: [&str; MAP_H as usize] = [
    "╔══════════════════════════════════╡ ☾ THE LATE LOUNGE ☽ ╞═══════════════════════════════════╗",
    "║ ▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔     ┌─ darts ───┐                 ┌───────────┐    ║",
    "║  ╿╽▯ ╿▯╿ ╽╿ ▯╿╽ ╿▯ ╿╽▯ ╿ ▯╿ ╽╿▯ ╿╽ ▯  │     │    ◎      │                 │ JUKEBOX ♪ │    ║",
    "║                         ≡ BAR ≡       │     │  · × ·    │                 │ ▂▄▆█▆▄▂   │    ║",
    "║         ╥           ╥           ╥     │     └───────────┘                 │ [·······] │    ║",
    "║ ▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄                                   └───────────┘    ║",
    "║",
    "║     h     h     h     h     h     h",
    "║                                           ░░░░░░░░░░░░░░░░░░░     h",
    "║  ♣                      h                 ░░░░░░░░░░░░░░░░░░░   ╭────╮",
    "║                       ╭────╮              ░░░░░░░░░░░░░░░░░░░ h │▒▒▒▒│ h                 ♣ ║",
    "║ ▄▄▄▄▄▄▄▄▄▄          h │▒▒▒▒│ h            ░░░░░░░░░░░░░░░░░░░   ╰────╯",
    "║ █ )~(~)~ █            ╰────╯              ░░░░░░░░░░░░░░░░░░░      h",
    "║ █ (~)~(~ █               h                   h",
    "║ ▀▀▀▀▀▀▀▀▀▀                                 ╭────╮",
    "║                 ~o                       h │▒▒▒▒│ h",
    "║    h    h                                  ╰────╯",
    "║                         h                     h                   h",
    "║                       ╭────╮                                    ╭────╮",
    "║                     h │▒▒▒▒│ h                                h │▒▒▒▒│ h",
    "║                       ╰────╯                                    ╰────╯",
    "║                          h                                         h",
    "║                                                                                          ♣ ║",
    "║",
    "║",
    "╚══════════════════════════════════════════╡ door ╞══════════════════════════════════════════╝",
];

/// A seat an active user can occupy. Labels normally float above the head;
/// seats under a table edge flip the label below so names never overdraw
/// their own table.
#[derive(Debug, Clone, Copy)]
pub struct Seat {
    pub x: u16,
    pub y: u16,
    pub label_below: bool,
}

pub const SEATS: &[Seat] = &[
    // bar stools
    Seat {
        x: 6,
        y: 7,
        label_below: false,
    },
    Seat {
        x: 12,
        y: 7,
        label_below: false,
    },
    Seat {
        x: 18,
        y: 7,
        label_below: false,
    },
    Seat {
        x: 24,
        y: 7,
        label_below: false,
    },
    Seat {
        x: 30,
        y: 7,
        label_below: false,
    },
    // fireplace armchairs
    Seat {
        x: 5,
        y: 16,
        label_below: true,
    },
    Seat {
        x: 10,
        y: 16,
        label_below: true,
    },
    // table at (24,10)
    Seat {
        x: 22,
        y: 11,
        label_below: false,
    },
    Seat {
        x: 31,
        y: 11,
        label_below: false,
    },
    Seat {
        x: 26,
        y: 9,
        label_below: false,
    },
    Seat {
        x: 27,
        y: 13,
        label_below: true,
    },
    // table at (45,14)
    Seat {
        x: 43,
        y: 15,
        label_below: false,
    },
    Seat {
        x: 52,
        y: 15,
        label_below: false,
    },
    Seat {
        x: 47,
        y: 13,
        label_below: false,
    },
    Seat {
        x: 48,
        y: 17,
        label_below: true,
    },
    // table at (66,9)
    Seat {
        x: 64,
        y: 10,
        label_below: false,
    },
    Seat {
        x: 73,
        y: 10,
        label_below: false,
    },
    Seat {
        x: 68,
        y: 8,
        label_below: false,
    },
    Seat {
        x: 69,
        y: 12,
        label_below: true,
    },
    // table at (24,18)
    Seat {
        x: 22,
        y: 19,
        label_below: false,
    },
    Seat {
        x: 31,
        y: 19,
        label_below: false,
    },
    Seat {
        x: 26,
        y: 17,
        label_below: false,
    },
    Seat {
        x: 27,
        y: 21,
        label_below: true,
    },
    // table at (66,18)
    Seat {
        x: 64,
        y: 19,
        label_below: false,
    },
    Seat {
        x: 73,
        y: 19,
        label_below: false,
    },
    Seat {
        x: 68,
        y: 17,
        label_below: false,
    },
    Seat {
        x: 69,
        y: 21,
        label_below: true,
    },
];

/// @graybeard's reserved corner stool at the end of the bar. Not part of the
/// general pool; he sits there whenever he is online (always).
pub const GRAYBEARD_SEAT: Seat = Seat {
    x: 36,
    y: 7,
    label_below: false,
};

/// Standing room near the door for the overflow crowd, staggered across two
/// rows so name labels never overdraw a neighbor's avatar.
pub const STANDING_SPOTS: &[(u16, u16)] = &[(38, 22), (44, 23), (50, 22), (56, 23), (62, 22)];

/// Where your `@` appears: just inside the door.
pub const SPAWN: (u16, u16) = (48, 20);

/// Where the bartender stands, behind the counter (sealed off from players).
pub const BARTENDER: (u16, u16) = (18, 3);

/// Where the cat sleeps (two cells: animated tail + body).
pub const CAT: (u16, u16) = (18, 15);

/// The bar counter row players walk up to.
pub const BAR_COUNTER: Zone = Zone {
    x0: 2,
    y0: 5,
    x1: 40,
    y1: 5,
};
pub const JUKEBOX: Zone = Zone {
    x0: 76,
    y0: 1,
    x1: 88,
    y1: 5,
};
pub const DARTBOARD: Zone = Zone {
    x0: 46,
    y0: 1,
    x1: 58,
    y1: 4,
};
pub const FIREPLACE: Zone = Zone {
    x0: 2,
    y0: 11,
    x1: 11,
    y1: 14,
};

/// Fire cells animated every few ticks (inside the fireplace box).
pub const FIRE_CELLS: Zone = Zone {
    x0: 4,
    y0: 12,
    x1: 9,
    y1: 13,
};
/// The jukebox equalizer strip, animated while music is playing.
pub const JUKEBOX_EQ: Zone = Zone {
    x0: 78,
    y0: 3,
    x1: 84,
    y1: 3,
};

#[derive(Debug, Clone, Copy)]
pub struct Zone {
    pub x0: u16,
    pub y0: u16,
    pub x1: u16,
    pub y1: u16,
}

impl Zone {
    pub fn contains(&self, x: u16, y: u16) -> bool {
        x >= self.x0 && x <= self.x1 && y >= self.y0 && y <= self.y1
    }

    /// Chebyshev distance from a point to this rectangle (0 when inside).
    pub fn distance(&self, x: u16, y: u16) -> u16 {
        let dx = self.x0.saturating_sub(x).max(x.saturating_sub(self.x1));
        let dy = self.y0.saturating_sub(y).max(y.saturating_sub(self.y1));
        dx.max(dy)
    }
}

/// Interactive props, in popover priority order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Interactive {
    Bartender,
    Jukebox,
    Dartboard,
    Cat,
    Fireplace,
}

/// The prop the player is close enough to interact with, if any.
pub fn nearest_interactive(x: u16, y: u16) -> Option<Interactive> {
    if BAR_COUNTER.distance(x, y) <= 2 {
        return Some(Interactive::Bartender);
    }
    if JUKEBOX.distance(x, y) <= 2 {
        return Some(Interactive::Jukebox);
    }
    if DARTBOARD.distance(x, y) <= 2 {
        return Some(Interactive::Dartboard);
    }
    let cat_zone = Zone {
        x0: CAT.0,
        y0: CAT.1,
        x1: CAT.0 + 1,
        y1: CAT.1,
    };
    if cat_zone.distance(x, y) <= 1 {
        return Some(Interactive::Cat);
    }
    if FIREPLACE.distance(x, y) <= 2 {
        return Some(Interactive::Fireplace);
    }
    None
}

/// The floor plan as a padded char grid, decoded once per process.
pub fn grid() -> &'static [Vec<char>] {
    static GRID: std::sync::OnceLock<Vec<Vec<char>>> = std::sync::OnceLock::new();
    GRID.get_or_init(|| {
        MAP.iter()
            .map(|row| {
                let mut cells: Vec<char> = row.chars().collect();
                cells.resize(MAP_W as usize, ' ');
                cells
            })
            .collect()
    })
}

/// The map char at `(x, y)`; rows shorter than `MAP_W` read as floor.
pub fn char_at(x: u16, y: u16) -> char {
    if x >= MAP_W || y >= MAP_H {
        return ' ';
    }
    grid()[y as usize][x as usize]
}

/// Players may stand on bare floor and the rug — nothing else.
pub fn walkable(x: u16, y: u16) -> bool {
    if x == 0 || y == 0 || x >= MAP_W - 1 || y >= MAP_H - 1 {
        return false;
    }
    matches!(char_at(x, y), ' ' | '░')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_rows_fit_declared_width() {
        for (y, row) in MAP.iter().enumerate() {
            let width = row.chars().count();
            assert!(
                width <= MAP_W as usize,
                "row {y} is {width} chars, wider than MAP_W"
            );
        }
        assert_eq!(MAP[0].chars().count(), MAP_W as usize);
        assert_eq!(MAP[MAP_H as usize - 1].chars().count(), MAP_W as usize);
    }

    #[test]
    fn seats_sit_on_chair_glyphs() {
        for seat in SEATS.iter().chain(std::iter::once(&GRAYBEARD_SEAT)) {
            assert_eq!(
                char_at(seat.x, seat.y),
                'h',
                "seat at ({}, {}) is not a chair",
                seat.x,
                seat.y
            );
        }
    }

    #[test]
    fn spawn_and_standing_spots_are_walkable() {
        assert!(walkable(SPAWN.0, SPAWN.1));
        for &(x, y) in STANDING_SPOTS {
            assert!(walkable(x, y), "standing spot ({x}, {y}) is blocked");
        }
    }

    #[test]
    fn bar_is_sealed_from_players() {
        // The bartender's alley (rows 2-4 behind the counter) must not be
        // reachable: shelf above, counter below, wall left, seal right.
        for y in 2..=4u16 {
            assert!(!walkable(41, y), "seal column open at y={y}");
        }
        for x in 2..=40u16 {
            assert!(!walkable(x, 1), "shelf row open at x={x}");
            assert!(!walkable(x, 5), "counter row open at x={x}");
        }
    }

    #[test]
    fn interactives_resolve_by_proximity() {
        // Standing in front of the bar.
        assert_eq!(nearest_interactive(18, 6), Some(Interactive::Bartender));
        // Next to the jukebox.
        assert_eq!(nearest_interactive(74, 4), Some(Interactive::Jukebox));
        // Middle of the rug: nothing.
        assert_eq!(nearest_interactive(52, 10), None);
        // By the fire.
        assert_eq!(nearest_interactive(6, 16), Some(Interactive::Fireplace));
        // Petting distance.
        assert_eq!(nearest_interactive(17, 16), Some(Interactive::Cat));
    }

    #[test]
    fn walls_and_furniture_block_movement() {
        assert!(!walkable(0, 10));
        assert!(!walkable(24, 10)); // table corner
        assert!(!walkable(6, 7)); // occupied-able chair
        assert!(walkable(52, 10)); // rug
        assert!(walkable(48, 20)); // spawn floor
    }
}
