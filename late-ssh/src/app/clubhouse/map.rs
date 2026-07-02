//! The Late Lounge floor plan: static ASCII furniture plus the metadata the
//! runtime needs (collision, seats, standing room, interactive zones). The
//! art is authored on a fixed grid larger than a typical viewport, so the
//! camera in `ui.rs` pans over it as you walk; rows may be right-trimmed and
//! are padded back to `MAP_W` at read time.
//!
//! Everything is drawn at "zoomed" scale, Dwarf Fortress vibes, single-width
//! glyphs only: stools are `(_)` on a `╨` leg, tables are 10x4 ovals with a
//! candle, people render as 3-row stick figures, the dog is three rows. Each
//! interactive landmark carries its page number in the art and doubles as a
//! signpost: the arcade cabinet is page 2, the big wooden door is the door
//! games on page 3, the poker table is Tables on page 4, and the easel is
//! the Artboard on page 5. The bar (with @bartender behind it), the jukebox,
//! the fireplace, and the dog round out the room.

pub const MAP_W: u16 = 184;
pub const MAP_H: u16 = 50;

#[rustfmt::skip]
pub const MAP: [&str; MAP_H as usize] = [
    "╔═══════════════════════════════════════════════════════════════════════════════╡ ☾ THE LATE LOUNGE ☽ ╞════════════════════════════════════════════════════════════════════════════════╗",
    "║▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔         ╭──────┬──────╮     ╭───────────╮                         ╭───────────╮     ╔═════════╗   ╭──────┬──────╮            ║",
    "║   ¡   !   ¡   °   !   ¡   !   °   ¡   !   ¡   °   !   ▐         │  ·   │    · │   ╭╯ ♪ JUKEBOX ♪ ╰╮  ╭────────────────╮ ╭╯   DOORS·3   ╰╮   ║ARCADE·2 ║   │  ·   │    · │      ♣♣♣   ║",
    "║   █   █   █   █   █   █   █   █   █   █   █   █   █   ▐         │    ☾ │  ·   │   │   ▂▄▆█▇▆▄▂    │  │ ☾ late·sh 24/7 │ │   ║ │ ▒ │ ║   │   ║╭───────╮║   │ ·    │    · │     ♣♣♣♣♣  ║",
    "║ ───────────────────────────────────────────────────── ▐         ├──────┼──────┤   │   [·······]   │  ╰────────────────╯ │   ║ │ ○ │ ║   │   ║│ ▄▀▄ · │║   ├──────┼──────┤      ♣♣♣   ║",
    "║      Y     Y     Y     Y     Y     Y     Y     Y      ▐         │ ·    │   ·  │   │   ▞▚ ▞▚ ▞▚    │                     │   ║ │ ▒ │ ║   │   ║╰───────╯║   │ ·    │   ·  │      ╰─╯   ║",
    "║                                               [$]     ▐         │      │ ·    │   ╰───○───────○───╯                     ╰───────────────╯   ║ ┃  ● ●  ║   │      │ ·    │            ║",
    "║                                                       ▐ ╭──╮    ╰──────┴──────╯                                                             ╚═════════╝   ╰──────┴──────╯            ║",
    "║           ╥╥              ╥╥              ╥╥          ▐ │▒▒│ ╭──╮                                                                                                                    ║",
    "║▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄ │▒▒│ │▒▒│                                                                                                                    ║",
    "║███████████████████≡·THE·LATE·BAR·≡█████████████████████ ╰──╯ ╰──╯                                                                                                                    ║",
    "║                                                                                                                                                          (_)        (_)              ║",
    "║    (_)     (_)     (_)     (_)     (_)     (_)   (_)      ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░                 ╨          ╨               ║",
    "║     ╨       ╨       ╨       ╨       ╨       ╨     ╨       ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░            ╭────────────────────╮          ║",
    "║                                                           ░░░░░░░░░(_)░░░░░░░░░░░░░░░░░░░░░░░(_)░░░░░░░░░░░░░░░░░░░░░░░(_)░░░░░░░░░░░░░░░        ╭───╯▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒╰───╮      ║",
    "║ ▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄               ╔═══════════╗       ░░░░░░░░░░╨░░░░░░░░░░░░░░░░░░░░░░░░░╨░░░░░░░░░░░░░░░░░░░░░░░░░╨░░░░░░░░░░░░░░░░   (_)  │▒♠▒▒▒▒▒▒▒▒TABLES·4▒▒▒▒▒▒▒▒♥▒│  (_) ║",
    "║ █▒▒▒¡▒▒▒▒▒¡▒▒▒▒▒¡▒▒▒▒▒█ ╭──╮          ║▌▐│▌║▐▌│▐▌▐║       ░░░░░░ ╭──────╮ ░░░░░░░░░░░░░░░░ ╭──────╮ ░░░░░░░░░░░░░░░░ ╭──────╮ ░░░░░░░░░░░    ╨   ╰───╮▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒╭───╯   ╨  ║",
    "║ █▒╔═════════════════╗▒█  _ ▐          ╠═══════════╣       ░░(_)░╭╯  ¡   ╰╮░(_)░░░░░░░░(_)░╭╯  ¡   ╰╮░(_)░░░░░░░░(_)░╭╯  ¡   ╰╮░(_)░░░░░░░            ╰────────────────────╯          ║",
    "║ █▒║ )~( ^ )~( ~ ( ^ ║▒█ ╰──╯          ║▐│▌▐▌║▌▐│▌║║       ░░░╨░░╰╮      ╭╯░░╨░░░░░░░░░░╨░░╰╮      ╭╯░░╨░░░░░░░░░░╨░░╰╮      ╭╯░░╨░░░░░░░░                                            ║",
    "║ █▒║ (~) ^ (~) ( ^ ) ║▒█               ╚═══════════╝       ░░░░░░ ╰──────╯ ░░░░░░░░░░░░░░░░ ╰──────╯ ░░░░░░░░░░░░░░░░ ╰──────╯ ░░░░░░░░░░░                (_)        (_)              ║",
    "║ █▒╚═════════════════╝▒█ ╭──╮                              ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░                 ╨          ╨               ║",
    "║ ▀▀▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▀▀  _ ▐                              ░░░░░░░░░(_)░░░░░░░░░░░░░░░░░░░░░░░(_)░░░░░░░░░░░░░░░░░░░░░░░(_)░░░░░░░░░░░░░░░                                            ║",
    "║    ░░░░░░░░░░░░░░░░░    ╰──╯                              ░░░░░░░░░░╨░░░░░░░░░░░░░░░░░░░░░░░░░╨░░░░░░░░░░░░░░░░░░░░░░░░░╨░░░░░░░░░░░░░░░░                                            ║",
    "║    ░░░░░░░░░░░░░░░░░                                      ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░                                            ║",
    "║    ░░░░░░░░░░░░░░░░░                                      ░░░░░░░░░(_)░░░░░░░░░░░░░░░░░░░░░░░(_)░░░░░░░░░░░░░░░░░░░░░░░(_)░░░░░░░░░░░░░░░            (_)                             ║",
    "║  ♣♣♣   \\,_,/ )                                            ░░░░░░░░░░╨░░░░░░░░░░░░░░░░░░░░░░░░░╨░░░░░░░░░░░░░░░░░░░░░░░░░╨░░░░░░░░░░░░░░░░             ╨                              ║",
    "║ ♣♣♣♣♣ ( o.o )/                                            ░░░░░░ ╭──────╮ ░░░░░░░░░░░░░░░░ ╭──────╮ ░░░░░░░░░░░░░░░░ ╭──────╮ ░░░░░░░░░░░          ╭──────╮                          ║",
    "║  ♣♣♣   \\_u_/                                              ░░(_)░╭╯  ¡   ╰╮░(_)░░░░░░░░(_)░╭╯  ¡   ╰╮░(_)░░░░░░░░(_)░╭╯  ¡   ╰╮░(_)░░░░░░░     (_) ╭╯  ¡   ╰╮ (_)                     ║",
    "║  ╰─╯                           ╭─╮                        ░░░╨░░╰╮      ╭╯░░╨░░░░░░░░░░╨░░╰╮      ╭╯░░╨░░░░░░░░░░╨░░╰╮      ╭╯░░╨░░░░░░░░      ╨  ╰╮      ╭╯  ╨                      ║",
    "║                                ╰┬╯           ♣♣♣          ░░░░░░ ╰──────╯ ░░░░░░░░░░░░░░░░ ╰──────╯ ░░░░░░░░░░░░░░░░ ╰──────╯ ░░░░░░░░░░░          ╰──────╯                          ║",
    "║   ╔════════════╗                │           ♣♣♣♣♣         ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░                                            ║",
    "║   ║ ARTBOARD·5 ║                ┴            ♣♣♣          ░░░░░░░░░(_)░░░░░░░░░░░░░░░░░░░░░░░(_)░░░░░░░░░░░░░░░░░░░░░░░(_)░░░░░░░░░░░░░░░            (_)                             ║",
    "║   ║  ~   ·   ° ║                             ╰─╯          ░░░░░░░░░░╨░░░░░░░░░░░░░░░░░░░░░░░░░╨░░░░░░░░░░░░░░░░░░░░░░░░░╨░░░░░░░░░░░░░░░░             ╨                              ║",
    "║   ║ °   *   ·  ║                                          ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░                                            ║",
    "║   ╚════════════╝                                          ░░░░░░░░░(_)░░░░░░░░░░░░░░░░░░░░░░░(_)░░░░░░░░░░░░░░░░░░░░░░░(_)░░░░░░░░░░░░░░░                              (_)           ║",
    "║     ╱        ╲                                            ░░░░░░░░░░╨░░░░░░░░░░░░░░░░░░░░░░░░░╨░░░░░░░░░░░░░░░░░░░░░░░░░╨░░░░░░░░░░░░░░░░                               ╨            ║",
    "║    ╱          ╲          (_)                              ░░░░░░ ╭──────╮ ░░░░░░░░░░░░░░░░ ╭──────╮ ░░░░░░░░░░░░░░░░ ╭──────╮ ░░░░░░░░░░░            (_)             ╭──────╮        ║",
    "║                           ╨                               ░░(_)░╭╯  ¡   ╰╮░(_)░░░░░░░░(_)░╭╯  ¡   ╰╮░(_)░░░░░░░░(_)░╭╯  ¡   ╰╮░(_)░░░░░░░             ╨         (_) ╭╯  ¡   ╰╮ (_)   ║",
    "║                        ╭──────╮                           ░░░╨░░╰╮      ╭╯░░╨░░░░░░░░░░╨░░╰╮      ╭╯░░╨░░░░░░░░░░╨░░╰╮      ╭╯░░╨░░░░░░░░          ╭──────╮      ╨  ╰╮      ╭╯  ╨    ║",
    "║                   (_) ╭╯  ¡   ╰╮ (_)                      ░░░░░░ ╰──────╯ ░░░░░░░░░░░░░░░░ ╰──────╯ ░░░░░░░░░░░░░░░░ ╰──────╯ ░░░░░░░░░░░     (_) ╭╯  ¡   ╰╮ (_)     ╰──────╯        ║",
    "║                    ╨  ╰╮      ╭╯  ╨                       ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░      ╨  ╰╮      ╭╯  ╨                      ║",
    "║                        ╰──────╯                           ░░░░░░░░░(_)░░░░░░░░░░░░░░░░░░░░░░░(_)░░░░░░░░░░░░░░░░░░░░░░░(_)░░░░░░░░░░░░░░░          ╰──────╯            (_)           ║",
    "║                                                           ░░░░░░░░░░╨░░░░░░░░░░░░░░░░░░░░░░░░░╨░░░░░░░░░░░░░░░░░░░░░░░░░╨░░░░░░░░░░░░░░░░                               ╨            ║",
    "║                          (_)             ♣♣♣                                                                                                         (_)                             ║",
    "║                           ╨             ♣♣♣♣♣                                                                                              ♣♣♣        ╨                        ♣♣♣   ║",
    "║                                          ♣♣♣                                                                                              ♣♣♣♣♣                               ♣♣♣♣♣  ║",
    "║                                          ╰─╯                                  ░░░░░░░░░░░░░░░░░░░░░░░░                                     ♣♣♣                                 ♣♣♣   ║",
    "║                                                                               ░░░░░░░░░░░░░░░░░░░░░░░░                                     ╰─╯                                 ╰─╯   ║",
    "║                                                                                                                                                                                      ║",
    "╚═══════════════════════════════════════════════════════════════════════════════════════╡ door ╞═══════════════════════════════════════════════════════════════════════════════════════╝"
];

/// What kind of furniture a seat is; decides where the occupant's head goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeatKind {
    /// `(_)` with a leg: the occupant's head renders one row above.
    Stool,
    /// Boxy armchair: the occupant sits inside, on the anchor itself.
    Armchair,
}

/// A seat an active user can occupy; `(x, y)` is the anchor cell (the `_`).
/// Labels normally float above the head; seats under a table edge flip the
/// label below so names never overdraw their own table.
#[derive(Debug, Clone, Copy)]
pub struct Seat {
    pub x: u16,
    pub y: u16,
    pub label_below: bool,
    pub kind: SeatKind,
}

const fn s(x: u16, y: u16, label_below: bool, kind: SeatKind) -> Seat {
    Seat {
        x,
        y,
        label_below,
        kind,
    }
}

pub const SEATS: &[Seat] = &[
    // bar stools
    s(6, 12, true, SeatKind::Stool),
    s(14, 12, true, SeatKind::Stool),
    s(22, 12, true, SeatKind::Stool),
    s(30, 12, true, SeatKind::Stool),
    s(38, 12, true, SeatKind::Stool),
    s(46, 12, true, SeatKind::Stool),
    // fireplace armchairs
    s(27, 17, false, SeatKind::Armchair),
    s(27, 21, false, SeatKind::Armchair),
    // rug tables, three rows of three (N/S/W/E stools each)
    s(70, 14, false, SeatKind::Stool),
    s(70, 21, true, SeatKind::Stool),
    s(63, 17, false, SeatKind::Stool),
    s(78, 17, false, SeatKind::Stool),
    s(96, 14, false, SeatKind::Stool),
    s(96, 21, true, SeatKind::Stool),
    s(89, 17, false, SeatKind::Stool),
    s(104, 17, false, SeatKind::Stool),
    s(122, 14, false, SeatKind::Stool),
    s(122, 21, true, SeatKind::Stool),
    s(115, 17, false, SeatKind::Stool),
    s(130, 17, false, SeatKind::Stool),
    s(70, 24, false, SeatKind::Stool),
    s(70, 31, true, SeatKind::Stool),
    s(63, 27, false, SeatKind::Stool),
    s(78, 27, false, SeatKind::Stool),
    s(96, 24, false, SeatKind::Stool),
    s(96, 31, true, SeatKind::Stool),
    s(89, 27, false, SeatKind::Stool),
    s(104, 27, false, SeatKind::Stool),
    s(122, 24, false, SeatKind::Stool),
    s(122, 31, true, SeatKind::Stool),
    s(115, 27, false, SeatKind::Stool),
    s(130, 27, false, SeatKind::Stool),
    s(70, 34, false, SeatKind::Stool),
    s(70, 41, true, SeatKind::Stool),
    s(63, 37, false, SeatKind::Stool),
    s(78, 37, false, SeatKind::Stool),
    s(96, 34, false, SeatKind::Stool),
    s(96, 41, true, SeatKind::Stool),
    s(89, 37, false, SeatKind::Stool),
    s(104, 37, false, SeatKind::Stool),
    s(122, 34, false, SeatKind::Stool),
    s(122, 41, true, SeatKind::Stool),
    s(115, 37, false, SeatKind::Stool),
    s(130, 37, false, SeatKind::Stool),
    // the quiet table off the rug, south-west
    s(28, 36, false, SeatKind::Stool),
    s(28, 43, true, SeatKind::Stool),
    s(21, 39, false, SeatKind::Stool),
    s(36, 39, false, SeatKind::Stool),
    // poker table
    s(156, 11, false, SeatKind::Stool),
    s(167, 11, false, SeatKind::Stool),
    s(156, 19, true, SeatKind::Stool),
    s(167, 19, true, SeatKind::Stool),
    s(143, 15, false, SeatKind::Stool),
    s(180, 15, false, SeatKind::Stool),
    // games-corner tables, south-east
    s(152, 24, false, SeatKind::Stool),
    s(152, 31, true, SeatKind::Stool),
    s(145, 27, false, SeatKind::Stool),
    s(160, 27, false, SeatKind::Stool),
    s(170, 34, false, SeatKind::Stool),
    s(170, 41, true, SeatKind::Stool),
    s(163, 37, false, SeatKind::Stool),
    s(178, 37, false, SeatKind::Stool),
    s(152, 36, false, SeatKind::Stool),
    s(152, 43, true, SeatKind::Stool),
    s(145, 39, false, SeatKind::Stool),
    s(160, 39, false, SeatKind::Stool),
];

/// @graybeard's reserved corner stool at the end of the bar. Not part of the
/// general pool; he sits there whenever he is online (always).
pub const GRAYBEARD_SEAT: Seat = s(52, 12, true, SeatKind::Stool);

/// Standing room near the door for the overflow crowd, staggered across
/// three rows so name labels never overdraw a neighbor's avatar.
pub const STANDING_SPOTS: &[(u16, u16)] = &[
    (72, 44),
    (78, 46),
    (106, 44),
    (112, 46),
    (66, 45),
    (118, 45),
];

/// Where your avatar appears: on the welcome mat just inside the door.
pub const SPAWN: (u16, u16) = (92, 46);

/// The bartender's head cell, in the alley behind the counter (sealed off
/// from players); the torso renders one row below.
pub const BARTENDER: (u16, u16) = (28, 6);

/// Top-left of the dog sprawled beside the hearth rug (3 rows, 8 wide).
pub const DOG: (u16, u16) = (8, 25);

/// The dog's bounding box, for proximity and styling.
pub const DOG_ZONE: Zone = Zone {
    x0: 8,
    y0: 25,
    x1: 15,
    y1: 27,
};

/// Where the "+N at the door" overflow label is centered.
pub const DOOR_LABEL: (u16, u16) = (108, 47);

/// The bar counter players walk up to (both counter rows).
pub const BAR_COUNTER: Zone = Zone {
    x0: 1,
    y0: 9,
    x1: 56,
    y1: 10,
};
/// The back-bar shelf (bottles and hanging glasses), for the liquor glow.
pub const BACK_BAR: Zone = Zone {
    x0: 1,
    y0: 2,
    x1: 55,
    y1: 5,
};
pub const JUKEBOX: Zone = Zone {
    x0: 84,
    y0: 1,
    x1: 100,
    y1: 6,
};
/// The big wooden door to the door games (page 3).
pub const DOORS: Zone = Zone {
    x0: 122,
    y0: 1,
    x1: 138,
    y1: 6,
};
/// The arcade cabinet (page 2).
pub const ARCADE: Zone = Zone {
    x0: 142,
    y0: 1,
    x1: 152,
    y1: 7,
};
/// The cabinet's screen cells, shimmering with phosphor pixels.
pub const ARCADE_SCREEN: Zone = Zone {
    x0: 145,
    y0: 4,
    x1: 149,
    y1: 4,
};
/// The oval poker table (Tables, page 4).
pub const POKER_TABLE: Zone = Zone {
    x0: 147,
    y0: 13,
    x1: 176,
    y1: 17,
};
/// The easel (the Artboard, page 5).
pub const EASEL: Zone = Zone {
    x0: 4,
    y0: 30,
    x1: 17,
    y1: 36,
};
pub const FIREPLACE: Zone = Zone {
    x0: 2,
    y0: 15,
    x1: 24,
    y1: 21,
};
/// The decor bookshelf near the hearth, for the colorful book spines.
pub const BOOKSHELF: Zone = Zone {
    x0: 40,
    y0: 15,
    x1: 52,
    y1: 19,
};
/// The neon house sign on the north wall, for the glow/flicker styling.
pub const NEON_SIGN: Zone = Zone {
    x0: 103,
    y0: 2,
    x1: 120,
    y1: 4,
};
/// The two moonlit windows; their `·`/`*` panes twinkle.
pub const WINDOWS: [Zone; 2] = [
    Zone {
        x0: 66,
        y0: 1,
        x1: 80,
        y1: 7,
    },
    Zone {
        x0: 156,
        y0: 1,
        x1: 170,
        y1: 7,
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
    x0: 88,
    y0: 3,
    x1: 95,
    y1: 3,
};
/// Every `¡` candle in the room (table centers and the mantle); they flicker.
pub const CANDLES: [(u16, u16); 16] = [
    (70, 17),
    (96, 17),
    (122, 17),
    (70, 27),
    (96, 27),
    (122, 27),
    (70, 37),
    (96, 37),
    (122, 37),
    (28, 39),
    (152, 27),
    (170, 37),
    (152, 39),
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

/// Players can walk (climb, really) over everything: tables, stools, the
/// dog, the fire. Only the outer walls and the bartender's alley (the shelf
/// and workspace behind the counter) block movement — you may stand ON the
/// counter, but never behind it.
pub fn walkable(x: u16, y: u16) -> bool {
    if x == 0 || y == 0 || x >= MAP_W - 1 || y >= MAP_H - 1 {
        return false;
    }
    !(x <= BAR_COUNTER.x1 && y < BAR_COUNTER.y0)
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
        // shelf rows above, counter below, wall left, seal column right.
        let reachable = reachable_from_spawn();
        assert!(
            !reachable.contains(&BARTENDER),
            "players can reach the bartender's alley"
        );
        for y in 2..9u16 {
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
            // A stool's own parens and leg surround the anchor, so look at
            // the full 8-neighborhood for a walkable approach cell.
            let mut approachable = false;
            for dx in -1i32..=1 {
                for dy in -1i32..=1 {
                    if (dx, dy) == (0, 0) {
                        continue;
                    }
                    let cell = (
                        x.wrapping_add_signed(dx as i16),
                        y.wrapping_add_signed(dy as i16),
                    );
                    if reachable.contains(&cell) {
                        approachable = true;
                    }
                }
            }
            assert!(approachable, "no way to walk up to the seat at ({x}, {y})");
        }
        for &(x, y) in STANDING_SPOTS {
            assert!(reachable.contains(&(x, y)), "spot ({x}, {y}) unreachable");
        }
    }

    #[test]
    fn interactives_resolve_by_proximity() {
        // Standing in front of the bar.
        assert_eq!(nearest_interactive(28, 12), Some(Interactive::Bartender));
        // Next to the jukebox.
        assert_eq!(nearest_interactive(82, 4), Some(Interactive::Jukebox));
        // In front of the arcade cabinet.
        assert_eq!(nearest_interactive(154, 4), Some(Interactive::Arcade));
        // Under the big door to the door games.
        assert_eq!(nearest_interactive(130, 8), Some(Interactive::Doors));
        // Walking up to the poker table.
        assert_eq!(nearest_interactive(145, 15), Some(Interactive::Poker));
        // Admiring the easel.
        assert_eq!(nearest_interactive(19, 33), Some(Interactive::Easel));
        // Petting distance.
        assert_eq!(nearest_interactive(16, 26), Some(Interactive::Dog));
        // Warming up by the hearth, out of the dog's reach.
        assert_eq!(nearest_interactive(25, 23), Some(Interactive::Fireplace));
        // Middle of the rug: nothing.
        assert_eq!(nearest_interactive(100, 22), None);
    }

    #[test]
    fn only_walls_and_the_bar_alley_block_movement() {
        assert!(!walkable(0, 25)); // west wall
        assert!(!walkable(28, 5)); // behind the counter
        assert!(walkable(67, 16)); // right over a table
        assert!(walkable(28, 9)); // standing ON the counter is allowed
        assert!(walkable(100, 22)); // rug
        assert!(walkable(SPAWN.0, SPAWN.1)); // welcome mat
    }
}
