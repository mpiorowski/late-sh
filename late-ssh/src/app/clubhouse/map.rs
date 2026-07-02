//! The Late Lounge floor plan: static ASCII furniture plus the metadata the
//! runtime needs (collision, seats, standing room, interactive zones). The
//! art is authored on a fixed grid larger than a typical viewport, so the
//! camera in `ui.rs` pans over it as you walk; rows may be right-trimmed and
//! are padded back to `MAP_W` at read time.
//!
//! Everything is drawn at "zoomed" scale — a stool is `(_)`, people are
//! head-plus-body sprites, the dog is three rows — and each interactive
//! landmark doubles as a signpost for an app page: the arcade cabinet is
//! page 2, the big wooden door is the door games on page 3, the poker table
//! is Tables on page 4, and the easel is the Artboard on page 5. The bar
//! (with the @bartender behind it), the jukebox, the fireplace, and the dog
//! round out the room.

pub const MAP_W: u16 = 200;
pub const MAP_H: u16 = 52;

#[rustfmt::skip]
pub const MAP: [&str; MAP_H as usize] = [
    "╔═══════════════════════════════════════════════════════════════════════════════════════╡ ☾ THE LATE LOUNGE ☽ ╞════════════════════════════════════════════════════════════════════════════════════════╗",
    "║▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔      ╭─────┬─────╮     ╭─────┬─────╮                              ╭───────────────╮            ╭─────────────╮                    ║",
    "║    ¡   !   ¡   °   !   ¡   !   °   ¡   !   ¡   !   ¡   °   !     ▐      │  ·  │   · │     │  ·  │   · │     ╭────────────────╮     ╭╯   ♪ JUKEBOX ♪   ╰╮         ╭╯    DOORS    ╰╮                   ║",
    "║    █   █   █   █   █   █   █   █   █   █   █   █   █   █   █     ▐      │     │ ·   │     │     │ ·   │     │ ☾ late·sh 24/7 │     │     ▂▄▆█▇▆▄▂      │         │ ║ │ ║ ▒ ║ │ ║ │                   ║",
    "║ ──────────────────────────────────────────────────────────────── ▐      │ ☾   │   · │     │   · │  ·  │     ╰────────────────╯     │    ╭─────────╮    │         │ ║ │ ○ ▒ ○ │ ║ │                   ║",
    "║                                                         [$]      ▐      │   · │ ·   │     │   · │ ·   │                            │    │ [·····] │    │         │ ║ │ ║ ▒ ║ │ ║ │                   ║",
    "║                                                                  ▐      ╰─────┴─────╯     ╰─────┴─────╯                            │    ╰─────────╯    │         ╰───────────────╯                   ║",
    "║             ╥╥                  ╥╥                  ╥╥           ▐                                                                 │     ▞▚ ▞▚ ▞▚      │                                             ║",
    "║▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄                                                                 ╰────○─────────○────╯                                             ║",
    "║█████████████████████████≡·THE·LATE·BAR·≡██████████████████████████                                                                                                                                   ║",
    "║                                                                                                                                                                                                      ║",
    "║     (_)       (_)       (_)       (_)       (_)       (_)   (_)                                                                                                                       ╔═══════════╗  ║",
    "║                                                                    ♣♣                                                                                    ♣♣                           ║A R C A D E║  ║",
    "║                                                                   ♣♣♣♣                                                                                  ♣♣♣♣                          ╟───────────╢  ║",
    "║                                                                    ╰╯                                                                                    ╰╯                           ║ ╭───────╮ ║  ║",
    "║ ▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄                                                                                                                                                               ║ │ ▄▀▄ · │ ║  ║",
    "║ █▒▒▒¡▒▒▒▒▒¡▒▒▒▒▒¡▒▒▒▒▒█ ╭──╮                                          ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░                                                 ║ │ ·  ●  │ ║  ║",
    "║ █▒╔═════════════════╗▒█  _ ▐                                          ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░                                                 ║ ╰───────╯ ║  ║",
    "║ █▒║ )~( ^ )~( ~ ( ^ ║▒█ ╰──╯                                          ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░                                                 ║  ┃   ● ●  ║  ║",
    "║ █▒║ (~) ^ (~) ( ^ ) ║▒█                                               ░░░░░░░░░░(_)░░░░░░░░░░░░░░░░░░░░░░░░░(_)░░░░░░░░░░░░░░░░░░░░░░                                                 ║ ▒▒▒▒▒▒▒▒▒ ║  ║",
    "║ █▒╚═════════════════╝▒█                                               ░░░░░░░░╭─────╮░░░░░░░░░░░░░░░░░░░░░╭─────╮░░░░░░░░░░░░░░░░░░░░                                                 ╚═══════════╝  ║",
    "║ ▀▀▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▀▀ ╭──╮                                          ░░░░(_)░│  ¡  │░(_)░░░░░░░░░░░░░(_)░│  ¡  │░(_)░░░░░░░░░░░░░░░░                                                                ║",
    "║    ░░░░░░░░░░░░░░░░░░    _ ▐                                          ░░░░░░░░╰─────╯░░░░░░░░░░░░░░░░░░░░░╰─────╯░░░░░░░░░░░░░░░░░░░░                                                                ║",
    "║    ░░░░░░░░░░░░░░░░░░   ╰──╯                                          ░░░░░░░░░░(_)░░░░░░░░░░░░░░░░░░░░░░░░░(_)░░░░░░░░░░░░░░░░░░░░░░                                                                ║",
    "║    ░░░░░░░░░░░░░░░░░░                                                 ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░                                                                ║",
    "║                                                                       ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░                                                                ║",
    "║       \\,_,/ )                                                         ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░                                                                ║",
    "║       ( o.o )/                                                        ░░░░░░░░░░░░░░░░░░░░░░░░(_)░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░                                                                ║",
    "║        \\_u_/                                                          ░░░░░░░░░░░░░░░░░░░░░░╭─────╮░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░                                                                ║",
    "║                                                                       ░░░░░░░░░░░░░░░░░░(_)░│  ¡  │░(_)░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░                                                                ║",
    "║     ╔═══════════╗                                                     ░░░░░░░░░░░░░░░░░░░░░░╰─────╯░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░                                                                ║",
    "║     ║ ·   ~   ° ║                           (_)                       ░░░░░░░░░░░░░░░░░░░░░░░░(_)░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░                 (_)        (_)                                 ║",
    "║     ║   *   ·   ║                         ╭─────╮                     ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░            ╭──────────────────────╮                            ║",
    "║     ║ °   ·   ~ ║                     (_) │  ¡  │ (_)                 ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░        ╭───╯▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒╰───╮                        ║",
    "║     ╚═══════════╝                         ╰─────╯                     ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░    (_) │▒▒▒▒♠▒▒▒▒▒▒♥▒▒▒▒▒▒♣▒▒▒▒▒▒♦▒▒▒▒│ (_)                    ║",
    "║       ╱       ╲                             (_)                       ░░░░░░░░░░(_)░░░░░░░░░░░░░░░░░░░░░░░░░(_)░░░░░░░░░░░░░░░░░░░░░░        ╰───╮▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒╭───╯                        ║",
    "║      ╱         ╲                                                      ░░░░░░░░╭─────╮░░░░░░░░░░░░░░░░░░░░░╭─────╮░░░░░░░░░░░░░░░░░░░░            ╰──────────────────────╯                            ║",
    "║                                                                       ░░░░(_)░│  ¡  │░(_)░░░░░░░░░░░░░(_)░│  ¡  │░(_)░░░░░░░░░░░░░░░░                 (_)        (_)                                 ║",
    "║                                                                       ░░░░░░░░╰─────╯░░░░░░░░░░░░░░░░░░░░░╰─────╯░░░░░░░░░░░░░░░░░░░░                                                                ║",
    "║                                                                       ░░░░░░░░░░(_)░░░░░░░░░░░░░░░░░░░░░░░░░(_)░░░░░░░░░░░░░░░░░░░░░░                                                                ║",
    "║                                                                       ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░                                                                ║",
    "║                                                                                                                                                                                                      ║",
    "║                              ♣♣                                                                                                                                                                      ║",
    "║                             ♣♣♣♣                                                                                                                                                                     ║",
    "║                              ╰╯                                                                                                                                                  ♣♣                  ║",
    "║                                                                                                                                                                                 ♣♣♣♣                 ║",
    "║  ♣♣                                                                                                                                                                              ╰╯                  ║",
    "║ ♣♣♣♣                                                                                                                                                                                                 ║",
    "║  ╰╯                                                                                   ░░░░░░░░░░░░░░░░░░░░░░░░                                                                                       ║",
    "║                                                                                       ░░░░░░░░░░░░░░░░░░░░░░░░                                                                                       ║",
    "║                                                                                                                                                                                                      ║",
    "╚═══════════════════════════════════════════════════════════════════════════════════════════════╡ door ╞═══════════════════════════════════════════════════════════════════════════════════════════════╝"
];

/// A seat an active user can occupy; `(x, y)` is the anchor cell (the `_` of
/// a stool or armchair), which the renderer swaps for a `☺` when taken.
/// Labels normally float above the head; seats under a table edge flip the
/// label below so names never overdraw their own table.
#[derive(Debug, Clone, Copy)]
pub struct Seat {
    pub x: u16,
    pub y: u16,
    pub label_below: bool,
}

const fn s(x: u16, y: u16, label_below: bool) -> Seat {
    Seat { x, y, label_below }
}

pub const SEATS: &[Seat] = &[
    // bar stools
s(7, 11, false),
    s(17, 11, false),
    s(27, 11, false),
    s(37, 11, false),
    s(47, 11, false),
    s(57, 11, false),
    // fireplace armchairs
    s(27, 17, false),
    s(27, 22, false),
    // round tables (five on the rug, one quiet corner; N/S/W/E each)
    s(83, 19, false),
    s(83, 23, true),
    s(77, 21, false),
    s(89, 21, false),
    s(111, 19, false),
    s(111, 23, true),
    s(105, 21, false),
    s(117, 21, false),
    s(97, 27, false),
    s(97, 31, true),
    s(91, 29, false),
    s(103, 29, false),
    s(83, 35, false),
    s(83, 39, true),
    s(77, 37, false),
    s(89, 37, false),
    s(111, 35, false),
    s(111, 39, true),
    s(105, 37, false),
    s(117, 37, false),
    s(47, 31, false),
    s(47, 35, true),
    s(41, 33, false),
    s(53, 33, false),
    // poker table
    s(153, 31, false),
    s(164, 31, false),
    s(153, 37, true),
    s(164, 37, true),
    s(140, 34, false),
    s(177, 34, false),
];

/// @graybeard's reserved corner stool at the end of the bar. Not part of the
/// general pool; he sits there whenever he is online (always).
pub const GRAYBEARD_SEAT: Seat = s(63, 11, false);

/// Standing room near the door for the overflow crowd, staggered across
/// three rows so name labels never overdraw a neighbor's avatar.
pub const STANDING_SPOTS: &[(u16, u16)] = &[
    (78, 46),
    (84, 48),
    (116, 46),
    (122, 48),
    (72, 47),
    (128, 47),
];

/// Where your avatar appears: on the welcome mat just inside the door.
pub const SPAWN: (u16, u16) = (100, 48);

/// The bartender's head cell, in the alley behind the counter (sealed off
/// from players); the body renders one row below.
pub const BARTENDER: (u16, u16) = (34, 5);

/// Top-left of the dog sprawled in front of the hearth (3 rows, 8 wide).
pub const DOG: (u16, u16) = (8, 26);

/// The dog's bounding box, for proximity and styling.
pub const DOG_ZONE: Zone = Zone {
    x0: 8,
    y0: 26,
    x1: 15,
    y1: 28,
};

/// Where the "+N at the door" overflow label is centered.
pub const DOOR_LABEL: (u16, u16) = (116, 49);

/// The bar counter players walk up to (both counter rows).
pub const BAR_COUNTER: Zone = Zone {
    x0: 1,
    y0: 8,
    x1: 67,
    y1: 9,
};
/// The back-bar bottle shelf, for the multicolor liquor glow.
pub const BACK_BAR: Zone = Zone {
    x0: 1,
    y0: 2,
    x1: 66,
    y1: 3,
};
pub const JUKEBOX: Zone = Zone {
    x0: 133,
    y0: 1,
    x1: 153,
    y1: 8,
};
/// The big wooden door to the door games (page 3).
pub const DOORS: Zone = Zone {
    x0: 163,
    y0: 1,
    x1: 179,
    y1: 6,
};
/// The arcade cabinet (page 2).
pub const ARCADE: Zone = Zone {
    x0: 184,
    y0: 11,
    x1: 196,
    y1: 20,
};
/// The cabinet's screen cells, shimmering with phosphor pixels.
pub const ARCADE_SCREEN: Zone = Zone {
    x0: 188,
    y0: 15,
    x1: 192,
    y1: 16,
};
/// The oval poker table (Tables, page 4).
pub const POKER_TABLE: Zone = Zone {
    x0: 143,
    y0: 32,
    x1: 174,
    y1: 36,
};
/// The easel (the Artboard, page 5).
pub const EASEL: Zone = Zone {
    x0: 6,
    y0: 30,
    x1: 18,
    y1: 36,
};
pub const FIREPLACE: Zone = Zone {
    x0: 2,
    y0: 15,
    x1: 24,
    y1: 21,
};
/// The neon house sign on the north wall, for the glow/flicker styling.
pub const NEON_SIGN: Zone = Zone {
    x0: 110,
    y0: 2,
    x1: 127,
    y1: 4,
};
/// The two moonlit windows; their `·`/`*` panes twinkle.
pub const WINDOWS: [Zone; 2] = [
    Zone {
        x0: 74,
        y0: 1,
        x1: 86,
        y1: 6,
    },
    Zone {
        x0: 92,
        y0: 1,
        x1: 104,
        y1: 6,
    },
];

/// Fire cells animated every few ticks (inside the firebox).
pub const FIRE_CELLS: Zone = Zone {
    x0: 5,
    y0: 18,
    x1: 21,
    y1: 19,
};
/// The jukebox equalizer strip, animated while music is playing.
pub const JUKEBOX_EQ: Zone = Zone {
    x0: 139,
    y0: 3,
    x1: 146,
    y1: 3,
};
/// Every `¡` candle in the room (table centers and the mantle); they flicker.
pub const CANDLES: [(u16, u16); 9] = [
    (83, 21),
    (111, 21),
    (97, 29),
    (83, 37),
    (111, 37),
    (47, 33),
    (6, 16),
    (12, 16),
    (18, 16),
];

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
    Arcade,
    Doors,
    Poker,
    Easel,
    Dog,
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
    if ARCADE.distance(x, y) <= 2 {
        return Some(Interactive::Arcade);
    }
    if DOORS.distance(x, y) <= 2 {
        return Some(Interactive::Doors);
    }
    if POKER_TABLE.distance(x, y) <= 2 {
        return Some(Interactive::Poker);
    }
    if EASEL.distance(x, y) <= 2 {
        return Some(Interactive::Easel);
    }
    if DOG_ZONE.distance(x, y) <= 1 {
        return Some(Interactive::Dog);
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

/// Players may stand on bare floor and rugs/mats — nothing else.
pub fn walkable(x: u16, y: u16) -> bool {
    if x == 0 || y == 0 || x >= MAP_W - 1 || y >= MAP_H - 1 {
        return false;
    }
    matches!(char_at(x, y), ' ' | '░')
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{HashSet, VecDeque};

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
    fn seats_sit_on_seat_anchors() {
        for seat in SEATS.iter().chain(std::iter::once(&GRAYBEARD_SEAT)) {
            assert_eq!(
                char_at(seat.x, seat.y),
                '_',
                "seat at ({}, {}) is not a seat anchor",
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

    /// Every cell a player can reach from spawn, by flood fill.
    fn reachable_from_spawn() -> HashSet<(u16, u16)> {
        let mut seen = HashSet::from([SPAWN]);
        let mut queue = VecDeque::from([SPAWN]);
        while let Some((x, y)) = queue.pop_front() {
            for (nx, ny) in [
                (x + 1, y),
                (x.wrapping_sub(1), y),
                (x, y + 1),
                (x, y.wrapping_sub(1)),
            ] {
                if walkable(nx, ny) && seen.insert((nx, ny)) {
                    queue.push_back((nx, ny));
                }
            }
        }
        seen
    }

    #[test]
    fn bar_alley_is_sealed_from_players() {
        // The bartender's alley (behind the counter) must not be reachable:
        // shelf row above, counter below, wall left, seal column right.
        let reachable = reachable_from_spawn();
        assert!(
            !reachable.contains(&BARTENDER),
            "players can reach the bartender's alley"
        );
        for y in 2..8u16 {
            for x in 1..BAR_COUNTER.x1 {
                assert!(!reachable.contains(&(x, y)), "alley leak at ({x}, {y})");
            }
        }
    }

    #[test]
    fn seats_and_spots_are_reachable() {
        let reachable = reachable_from_spawn();
        for seat in SEATS.iter().chain(std::iter::once(&GRAYBEARD_SEAT)) {
            let (x, y) = (seat.x, seat.y);
            let adjacent = [
                (x + 1, y),
                (x.wrapping_sub(1), y),
                (x, y + 1),
                (x, y.wrapping_sub(1)),
            ];
            assert!(
                adjacent.iter().any(|cell| reachable.contains(cell)),
                "no way to walk up to the seat at ({x}, {y})"
            );
        }
        for &(x, y) in STANDING_SPOTS {
            assert!(reachable.contains(&(x, y)), "spot ({x}, {y}) unreachable");
        }
    }

    #[test]
    fn interactives_resolve_by_proximity() {
        // Standing in front of the bar.
        assert_eq!(nearest_interactive(34, 11), Some(Interactive::Bartender));
        // Next to the jukebox.
        assert_eq!(nearest_interactive(131, 5), Some(Interactive::Jukebox));
        // Under the big door to the door games.
        assert_eq!(nearest_interactive(171, 8), Some(Interactive::Doors));
        // In front of the arcade cabinet.
        assert_eq!(nearest_interactive(182, 15), Some(Interactive::Arcade));
        // Walking up to the poker table.
        assert_eq!(nearest_interactive(145, 30), Some(Interactive::Poker));
        // Admiring the easel.
        assert_eq!(nearest_interactive(20, 33), Some(Interactive::Easel));
        // Petting distance.
        assert_eq!(nearest_interactive(16, 26), Some(Interactive::Dog));
        // Warming up by the hearth, out of the dog's reach.
        assert_eq!(nearest_interactive(26, 19), Some(Interactive::Fireplace));
        // Middle of the rug: nothing.
        assert_eq!(nearest_interactive(122, 26), None);
    }

    #[test]
    fn walls_and_furniture_block_movement() {
        assert!(!walkable(0, 25)); // west wall
        assert!(!walkable(80, 20)); // table corner
        assert!(!walkable(7, 11)); // occupied-able bar stool
        assert!(walkable(122, 26)); // rug
        assert!(walkable(SPAWN.0, SPAWN.1)); // welcome mat
    }
}
