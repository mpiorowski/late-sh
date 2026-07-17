#!/usr/bin/env python3
"""Generate the Clubhouse tavern floor plan (late-ssh/src/app/clubhouse/map.rs).

The map is authored here as prop stamps on a grid, validated (row widths,
seat anchors, flood fill: sealed bar alley, reachable seats/spots/zones),
and written out as the plain Rust literal that gets committed. Nobody should
hand-edit the 184-column MAP strings; tweak this script and re-run:

    python3 scripts/gen_clubhouse_map.py --print   # preview in the terminal
    python3 scripts/gen_clubhouse_map.py --write   # regenerate map.rs

NOTE: the zone constants and test probe coordinates in RUST_TEMPLATE below
are hand-synced. If you move or resize a prop, `--write` prints the fresh
zone numbers — paste them into RUST_TEMPLATE (and adjust the probe points in
`interactives_resolve_by_proximity`) before committing.
"""

import sys
from collections import deque

W, H = 184, 50
grid = [[' '] * W for _ in range(H)]

seats = []          # (x, y, label_below, kind)
def put(x, y, s, transparent=False):
    assert 0 <= y < H, (x, y, s)
    assert x >= 0 and x + len(s) <= W, (x, y, len(s), s)
    for i, ch in enumerate(s):
        if transparent and ch == ' ':
            continue
        grid[y][x + i] = ch

def stamp(x, y, rows, transparent=True):
    for dy, r in enumerate(rows):
        put(x, y + dy, r, transparent)

def stool(x, y, below=False):
    """A stool with a leg; x/y is the anchor (the `_`)."""
    put(x - 1, y, '(_)')
    put(x, y + 1, '╨')
    seats.append((x, y, below, 'Stool'))

def armchair_facing_left(x, y):
    """4x3 armchair, open side west; anchor is the seat `_`."""
    stamp(x, y, ['╭──╮', ' _ ▐', '╰──╯'], transparent=False)
    seats.append((x + 1, y + 1, False, 'Armchair'))

def center(s, w):
    pad = w - len(s)
    return ' ' * (pad // 2) + s + ' ' * (pad - pad // 2)

# ---------------------------------------------------------------- walls
grid[0] = list('═' * W); grid[0][0] = '╔'; grid[0][W - 1] = '╗'
grid[H - 1] = list('═' * W); grid[H - 1][0] = '╚'; grid[H - 1][W - 1] = '╝'
for y in range(1, H - 1):
    grid[y][0] = '║'; grid[y][W - 1] = '║'
SIGN = '╡ ☾ THE LATE LOUNGE ☽ ╞'
put((W - len(SIGN)) // 2, 0, SIGN)
DOOR_SIGN = '╡ door ╞'
DOOR_X = (W - len(DOOR_SIGN)) // 2
put(DOOR_X, H - 1, DOOR_SIGN)

# ---------------------------------------------------------------- the bar
# Bottle shelf, hanging glasses, sealed alley, taps, two-row counter.
BAR_X1 = 56
put(1, 1, '▔' * BAR_X1)
neck_pattern = ['¡', '!', '¡', '°', '!', '¡', '!', '°']
for i, bx in enumerate(range(4, 54, 4)):
    grid[2][bx] = neck_pattern[i % len(neck_pattern)]
    grid[3][bx] = '█'
put(2, 4, '─' * 53)
for gx in range(7, 54, 6):                 # hanging glasses under the shelf
    grid[5][gx] = 'Y'
put(48, 6, '[$]')
for tx in (12, 28, 44):
    put(tx, 8, '╥╥')
put(1, 9, '▄' * BAR_X1)
put(1, 10, '█' * BAR_X1)
BAR_SIGN = '≡·THE·LATE·BAR·≡'
put(28 - len(BAR_SIGN) // 2, 10, BAR_SIGN)
for y in range(2, 9):
    grid[y][BAR_X1] = '▐'
BARTENDER = (28, 6)                        # head; torso renders at y+1
for sx in (6, 14, 22, 30, 38, 46):
    stool(sx, 12, below=True)              # labels drop below, onto the floor
GRAYBEARD = (52, 12)
put(51, 12, '(_)')
put(52, 13, '╨')

# barrels at the end of the bar
stamp(58, 7, ['╭──╮', '│▒▒│', '│▒▒│', '╰──╯'], transparent=False)
stamp(63, 8, ['╭──╮', '│▒▒│', '╰──╯'], transparent=False)

# ---------------------------------------------------------------- windows
def window(x, y, has_moon):
    rows = [
        '╭──────┬──────╮',
        '│  ·   │    · │',
        '│    ☾ │  ·   │' if has_moon else '│ ·    │    · │',
        '├──────┼──────┤',
        '│ ·    │   ·  │',
        '│      │ ·    │',
        '╰──────┴──────╯',
    ]
    stamp(x, y, rows, transparent=False)
    return (x, y, x + 14, y + 6)

WINDOW_A = window(66, 1, True)
WINDOW_B = window(156, 1, False)

# ---------------------------------------------------------------- jukebox
JUKE_X, JUKE_Y = 84, 1
JW = 17
hood = center('╭' + '─' * 11 + '╮', JW)
shoulders = '╭╯' + center('♪ JUKEBOX ♪', JW - 4) + '╰╮'
feet = list('─' * (JW - 2))
feet[3] = '○'; feet[11] = '○'
juke = [
    hood,
    shoulders,
    '│' + center('▂▄▆█▇▆▄▂', JW - 2) + '│',
    '│' + center('[·······]', JW - 2) + '│',
    '│' + center('▞▚ ▞▚ ▞▚', JW - 2) + '│',
    '╰' + ''.join(feet) + '╯',
]
stamp(JUKE_X, JUKE_Y, juke, transparent=True)
JUKEBOX_ZONE = (JUKE_X, JUKE_Y, JUKE_X + JW - 1, JUKE_Y + 5)
eq_line = ''.join(grid[JUKE_Y + 2][JUKE_X:JUKE_X + JW])
eq_start = JUKE_X + eq_line.index('▂')
JUKEBOX_EQ = (eq_start, JUKE_Y + 2, eq_start + 7, JUKE_Y + 2)

# ---------------------------------------------------------------- neon sign
NEON_TEXT = '☾ late·sh 24/7'
neon = [
    '╭' + '─' * (len(NEON_TEXT) + 2) + '╮',
    '│ ' + NEON_TEXT + ' │',
    '╰' + '─' * (len(NEON_TEXT) + 2) + '╯',
]
NEON_X, NEON_Y = 103, 2
stamp(NEON_X, NEON_Y, neon, transparent=False)
NEON_ZONE = (NEON_X, NEON_Y, NEON_X + len(neon[0]) - 1, NEON_Y + 2)

# ---------------------------------------------------------------- door games door
DOORS_X, DOORS_Y = 122, 1
DW = 17
doors = [
    center('╭' + '─' * 11 + '╮', DW),
    '╭╯' + center('DOORS·3', DW - 4) + '╰╮',
    '│' + center('║ │ ▒ │ ║', DW - 2) + '│',
    '│' + center('║ │ ○ │ ║', DW - 2) + '│',
    '│' + center('║ │ ▒ │ ║', DW - 2) + '│',
    '╰' + '─' * (DW - 2) + '╯',
]
stamp(DOORS_X, DOORS_Y, doors, transparent=True)
DOORS_ZONE = (DOORS_X, DOORS_Y, DOORS_X + DW - 1, DOORS_Y + 5)

# ---------------------------------------------------------------- arcade cabinet
ARC_X, ARC_Y = 142, 1
arcade = [
    '╔═════════╗',
    '║' + center('ARCADE·2', 9) + '║',
    '║╭───────╮║',
    '║│ ▄▀▄ · │║',
    '║╰───────╯║',
    '║ ┃  ● ●  ║',
    '╚═════════╝',
]
stamp(ARC_X, ARC_Y, arcade, transparent=False)
ARCADE_ZONE = (ARC_X, ARC_Y, ARC_X + 10, ARC_Y + 6)
ARCADE_SCREEN = (ARC_X + 3, ARC_Y + 3, ARC_X + 7, ARC_Y + 3)

# ---------------------------------------------------------------- fireplace
FIRE_X, FIRE_Y = 2, 15
fire = [
    '▄' * 23,
    '█▒▒▒¡▒▒▒▒▒¡▒▒▒▒▒¡▒▒▒▒▒█',
    '█▒╔' + '═' * 17 + '╗▒█',
    '█▒║' + ' )~( ^ )~( ~ ( ^ ' + '║▒█',
    '█▒║' + ' (~) ^ (~) ( ^ ) ' + '║▒█',
    '█▒╚' + '═' * 17 + '╝▒█',
    '▀▀▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▀▀',
]
for r in fire:
    assert len(r) == 23, (len(r), r)
stamp(FIRE_X, FIRE_Y, fire, transparent=False)
FIREPLACE_ZONE = (FIRE_X, FIRE_Y, FIRE_X + 22, FIRE_Y + 6)
FIRE_CELLS = (FIRE_X + 3, FIRE_Y + 3, FIRE_X + 19, FIRE_Y + 4)
MANTLE_CANDLES = [(FIRE_X + 4, FIRE_Y + 1), (FIRE_X + 10, FIRE_Y + 1), (FIRE_X + 16, FIRE_Y + 1)]
for ry in (22, 23, 24):
    put(5, ry, '░' * 17)
# The dog is not map art: it wanders as shared lobby state and ui.rs draws
# it live. Its home cell and waypoints are hand-authored in RUST_TEMPLATE
# (DOG_HOME / DOG_WAYPOINTS).
DOG_HOME = (11, 26)
armchair_facing_left(26, 16)
armchair_facing_left(26, 20)

# decor bookshelf between the hearth corner and the rug
BOOK_X, BOOK_Y = 40, 15
books = [
    '╔═══════════╗',
    '║▌▐│▌║▐▌│▐▌▐║',
    '╠═══════════╣',
    '║▐│▌▐▌║▌▐│▌║║',
    '╚═══════════╝',
]
stamp(BOOK_X, BOOK_Y, books, transparent=False)
BOOKSHELF_ZONE = (BOOK_X, BOOK_Y, BOOK_X + 12, BOOK_Y + 4)

# floor lamp in the south-west lane
stamp(33, 28, ['╭─╮', '╰┬╯', ' │', ' ┴'], transparent=True)

# ---------------------------------------------------------------- easel (artboard)
EAS_X, EAS_Y = 4, 30
easel = [
    '╔════════════╗',
    '║ ARTBOARD·4 ║',
    '║  ~   ·   ° ║',
    '║ °   *   ·  ║',
    '╚════════════╝',
    '  ╱        ╲',
    ' ╱          ╲',
]
stamp(EAS_X, EAS_Y, easel, transparent=True)
EASEL_ZONE = (EAS_X, EAS_Y, EAS_X + 13, EAS_Y + 6)

# ---------------------------------------------------------------- rug + tables
RUG = (60, 12, 138, 42)
for y in range(RUG[1], RUG[3] + 1):
    put(RUG[0], y, '░' * (RUG[2] - RUG[0] + 1))

CANDLES = []
def table(x, y):
    """10x4 oval table with a candle and four legged stools."""
    stamp(x, y, [
        ' ╭──────╮ ',
        '╭╯  ¡   ╰╮',
        '╰╮      ╭╯',
        ' ╰──────╯ ',
    ], transparent=False)
    CANDLES.append((x + 4, y + 1))
    stool(x + 4, y - 2)
    stool(x + 4, y + 5, True)
    stool(x - 3, y + 1)
    stool(x + 12, y + 1)

for ty, xs in ((16, (66, 92, 118)), (26, (66, 92, 118)), (36, (66, 92, 118))):
    for tx in xs:
        table(tx, ty)
table(24, 38)          # the quiet one off the rug, south-west

# ---------------------------------------------------------------- poker table
POK_X, POK_Y = 146, 13
PW = 32
felt_edge = '▒' * 20
mid = list('▒' * 28)
text = 'LOBBY'
start = (28 - len(text)) // 2
for i, ch in enumerate(text):
    mid[start + i] = ch
mid[1] = '♠'; mid[26] = '♥'
poker = [
    ' ' * 5 + '╭' + '─' * 20 + '╮' + ' ' * 5,
    ' ╭───╯' + felt_edge + '╰───╮ ',
    ' │' + ''.join(mid) + '│ ',
    ' ╰───╮' + felt_edge + '╭───╯ ',
    ' ' * 5 + '╰' + '─' * 20 + '╯' + ' ' * 5,
]
for r in poker:
    assert len(r) == PW, (len(r), r)
stamp(POK_X, POK_Y, poker, transparent=True)
POKER_ZONE = (POK_X + 1, POK_Y, POK_X + PW - 2, POK_Y + 4)
stool(POK_X + 10, POK_Y - 2)
stool(POK_X + 21, POK_Y - 2)
stool(POK_X + 10, POK_Y + 6, True)
stool(POK_X + 21, POK_Y + 6, True)
stool(POK_X - 3, POK_Y + 2)
stool(POK_X + PW + 2, POK_Y + 2)

# a couple more tables in the games corner, south-east
table(148, 26)
table(166, 36)
table(148, 38)

# ---------------------------------------------------------------- door + mat
put(80, 46, '░' * 24)
put(80, 47, '░' * 24)
SPAWN = (92, 46)
DOOR_LABEL = (108, 47)
STANDING = [(72, 44), (78, 46), (106, 44), (112, 46), (66, 45), (118, 45)]

# ---------------------------------------------------------------- plants
def plant(x, y):
    stamp(x, y, [' ♣♣♣', '♣♣♣♣♣', ' ♣♣♣', ' ╰─╯'], transparent=True)

plant(176, 2)
plant(2, 25)
plant(46, 29)
plant(42, 43)
plant(140, 44)
plant(176, 44)

# ================================================================ checks
def walkable(x, y):
    # Mirrors map.rs: everything is climbable except the outer walls, the
    # counter, and the bartender's alley (x <= BAR_X1, y <= counter bottom).
    if x <= 0 or y <= 0 or x >= W - 1 or y >= H - 1:
        return False
    return not (x <= BAR_X1 and y <= 10)

for y, row in enumerate(grid):
    assert len(row) == W, y
    assert '"' not in ''.join(row), y

seen = {SPAWN}
q = deque([SPAWN])
while q:
    x, y = q.popleft()
    for dx, dy in ((1, 0), (-1, 0), (0, 1), (0, -1)):
        n = (x + dx, y + dy)
        if n not in seen and walkable(*n):
            seen.add(n)
            q.append(n)

assert BARTENDER not in seen, 'bar alley reachable!'
assert not any((x, y) in seen for y in range(2, 9) for x in range(1, BAR_X1)), 'alley leak'

def near_reachable(x, y):
    return any((x + dx, y + dy) in seen
               for dx in (-1, 0, 1) for dy in (-1, 0, 1) if (dx, dy) != (0, 0))

for (x, y, _b, _k) in seats:
    assert grid[y][x] == '_', ('seat anchor not _', x, y, grid[y][x])
    assert near_reachable(x, y), ('seat unreachable', x, y)
assert grid[GRAYBEARD[1]][GRAYBEARD[0]] == '_' and near_reachable(*GRAYBEARD)
for x, y in STANDING:
    assert (x, y) in seen, ('standing spot unreachable', x, y)
assert walkable(*SPAWN)
assert len(seats) == len(set((x, y) for x, y, _, _ in seats)), 'duplicate seats'

def zone_near_reachable(z, dist):
    x0, y0, x1, y1 = z
    return any(max(max(x0 - x, x - x1, 0), max(y0 - y, y - y1, 0)) <= dist for (x, y) in seen)

zones = [('bar', (1, 9, BAR_X1, 10), 2), ('juke', JUKEBOX_ZONE, 2),
         ('doors', DOORS_ZONE, 2), ('arcade', ARCADE_ZONE, 2),
         ('poker', POKER_ZONE, 2), ('easel', EASEL_ZONE, 2),
         ('fire', FIREPLACE_ZONE, 2)]
for name, z, d in zones:
    assert zone_near_reachable(z, d), name
assert zone_near_reachable((DOG_HOME[0] - 1, DOG_HOME[1], DOG_HOME[0] + 1, DOG_HOME[1]), 1), 'dog home'

print(f'OK: {len(seats)} seats, {len(STANDING)} standing, reachable: {len(seen)}')

# ================================================================ print
if '--print' in sys.argv:
    for lo, hi in ((0, 92), (92, 184)):
        print(f'--- columns {lo}..{hi} ---')
        for y, row in enumerate(grid):
            print(f'{y:2d}|' + ''.join(row[lo:hi]))

# ================================================================ emit
RUST_TEMPLATE = '''//! The Late Lounge floor plan: static ASCII furniture plus the metadata the
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
__MAP__
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
__SEATS__
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
                    let cell = (x.wrapping_add_signed(dx as i16), y.wrapping_add_signed(dy as i16));
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
'''

if '--write' in sys.argv or '--emit' in sys.argv:
    import os

    def esc(s):
        return s.replace('\\', '\\\\')

    map_lines = ',\n'.join('    "' + esc(''.join(row).rstrip()) + '"' for row in grid)
    seat_lines = [
        f'    s({x}, {y}, {str(b).lower()}, SeatKind::{k}),' for (x, y, b, k) in seats
    ]
    groups = [
        ('bar stools', 6),
        ('fireplace armchairs', 2),
        ('rug tables, three rows of three (N/S/W/E stools each)', 36),
        ('the quiet table off the rug, south-west', 4),
        ('poker table', 6),
        ('games-corner tables, south-east', 12),
    ]
    assert sum(n for _, n in groups) == len(seat_lines), (sum(n for _, n in groups), len(seat_lines))
    out_seats = []
    idx = 0
    for label, n in groups:
        out_seats.append(f'    // {label}')
        out_seats.extend(seat_lines[idx:idx + n])
        idx += n

    out = RUST_TEMPLATE.replace('__MAP__', map_lines).replace('__SEATS__', '\n'.join(out_seats))
    path = os.path.join(os.path.dirname(__file__), '..', 'late-ssh', 'src', 'app', 'clubhouse', 'map.rs')
    with open(path, 'w') as f:
        f.write(out)
    print(f'wrote {os.path.normpath(path)}')
    print('fresh zone numbers (sync RUST_TEMPLATE if any moved):')
    print('BARTENDER', BARTENDER, 'GRAYBEARD', GRAYBEARD, 'SPAWN', SPAWN)
    print('DOG_HOME', DOG_HOME, 'DOOR_LABEL', DOOR_LABEL, 'STANDING', STANDING)
    print('BAR_COUNTER', (1, 9, BAR_X1, 10), 'BACK_BAR', (1, 2, BAR_X1 - 1, 5))
    print('JUKEBOX', JUKEBOX_ZONE, 'EQ', JUKEBOX_EQ)
    print('DOORS', DOORS_ZONE, 'ARCADE', ARCADE_ZONE, 'SCREEN', ARCADE_SCREEN)
    print('POKER', POKER_ZONE, 'EASEL', EASEL_ZONE)
    print('FIREPLACE', FIREPLACE_ZONE, 'FIRE_CELLS', FIRE_CELLS)
    print('CANDLES', CANDLES + MANTLE_CANDLES)
    print('NEON', NEON_ZONE, 'WINDOWS', WINDOW_A, WINDOW_B)
    print('BOOKSHELF', BOOKSHELF_ZONE)
