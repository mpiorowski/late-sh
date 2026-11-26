// A Sokoban level parsed from a text map, and the pure rules of one push.
//
// Map legend: `0` boulder, `@` hero, `^` open hole or pit, `R` rolling
// boulder trap, `<` `>` stairs, `|` `-` `+` `#` and
// blanks are walls; `M` is a known boulder mimic (floor to the solver, once
// it's dead; `play` stops before touching it); any other glyph (`.`, items)
// is floor. A filled hole is plain floor, so a board copied off the screen
// mid-level parses as is.

use std::collections::VecDeque;

pub const DIRS: [Dir; 4] = [Dir::West, Dir::East, Dir::North, Dir::South];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dir {
    West,
    East,
    North,
    South,
}

impl Dir {
    pub fn delta(self) -> (i32, i32) {
        match self {
            Dir::West => (-1, 0),
            Dir::East => (1, 0),
            Dir::North => (0, -1),
            Dir::South => (0, 1),
        }
    }

    pub fn key(self) -> char {
        match self {
            Dir::West => 'h',
            Dir::East => 'l',
            Dir::North => 'k',
            Dir::South => 'j',
        }
    }

    pub fn word(self) -> &'static str {
        match self {
            Dir::West => "left",
            Dir::East => "right",
            Dir::North => "up",
            Dir::South => "down",
        }
    }

    fn index(self) -> usize {
        match self {
            Dir::West => 0,
            Dir::East => 1,
            Dir::North => 2,
            Dir::South => 3,
        }
    }

    pub fn opposite(self) -> Dir {
        match self {
            Dir::West => Dir::East,
            Dir::East => Dir::West,
            Dir::North => Dir::South,
            Dir::South => Dir::North,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cell {
    Floor,
    Stairs,
    Hole,
    RollingTrap,
}

pub const MAX_CELLS: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct Bits([u64; 4]);

impl Bits {
    pub fn has(&self, i: usize) -> bool {
        self.0[i / 64] >> (i % 64) & 1 == 1
    }
    pub fn set(&mut self, i: usize) {
        self.0[i / 64] |= 1 << (i % 64);
    }
    pub fn clear(&mut self, i: usize) {
        self.0[i / 64] &= !(1 << (i % 64));
    }
    pub fn iter(&self) -> impl Iterator<Item = usize> + '_ {
        (0..MAX_CELLS).filter(|&i| self.has(i))
    }
    pub fn count(&self) -> u32 {
        self.0.iter().map(|w| w.count_ones()).sum()
    }
}

/// Boulders, filled holes and the hero's square: everything a push changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct State {
    pub boulders: Bits,
    pub filled: Bits,
    pub hero: usize,
}

pub struct Level {
    /// The map text as given, walls included; `render` draws over it.
    grid: Vec<Vec<char>>,
    /// Column and row of each cell.
    pub pos: Vec<(usize, usize)>,
    pub cells: Vec<Cell>,
    /// Neighbour cell index per direction, `None` for a wall.
    next: Vec<[Option<usize>; 4]>,
    pub holes: Vec<usize>,
    /// Cells marked `M`: a giant mimic posing as a boulder.
    pub mimics: Vec<usize>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ParseError {
    NoHero,
    TwoHeroes,
    /// Every hole is filled: nothing left to solve.
    NoHoles,
    TooBig(usize),
}

pub fn parse(text: &str) -> Result<(Level, State), ParseError> {
    let rows: Vec<Vec<char>> = text.lines().map(|l| l.chars().collect()).collect();
    let height = rows.len();
    let width = rows.iter().map(|r| r.len()).max().unwrap_or(0);
    let grid: Vec<Vec<char>> = rows.iter().map(|r| (0..width).map(|x| r.get(x).copied().unwrap_or(' ')).collect()).collect();
    let mut index = vec![vec![None; width]; height];
    let mut pos = vec![];
    let mut cells = vec![];
    let mut boulders = Bits::default();
    let mut hero = None;
    let mut mimics = vec![];
    for (y, row) in grid.iter().enumerate() {
        for (x, &ch) in row.iter().enumerate() {
            let cell = match ch {
                '|' | '-' | '+' | ' ' | '#' => continue,
                '^' => Cell::Hole,
                'R' => Cell::RollingTrap,
                '<' | '>' => Cell::Stairs,
                _ => Cell::Floor,
            };
            let i = cells.len();
            if i >= MAX_CELLS {
                return Err(ParseError::TooBig(i));
            }
            match ch {
                '0' => boulders.set(i),
                '@' if hero.is_some() => return Err(ParseError::TwoHeroes),
                '@' => hero = Some(i),
                'M' => mimics.push(i),
                _ => {}
            }
            index[y][x] = Some(i);
            pos.push((x, y));
            cells.push(cell);
        }
    }
    let hero = hero.ok_or(ParseError::NoHero)?;
    let next = pos
        .iter()
        .map(|&(x, y)| {
            DIRS.map(|d| {
                let (dx, dy) = d.delta();
                let (nx, ny) = (x as i32 + dx, y as i32 + dy);
                if nx < 0 || ny < 0 || ny as usize >= height || nx as usize >= width {
                    return None;
                }
                index[ny as usize][nx as usize]
            })
        })
        .collect();
    let holes: Vec<usize> = (0..cells.len()).filter(|&i| cells[i] == Cell::Hole).collect();
    if holes.is_empty() {
        return Err(ParseError::NoHoles);
    }
    let level = Level { grid, pos, cells, next, holes, mimics };
    let state = State { boulders, filled: Bits::default(), hero };
    Ok((level, state))
}

impl Level {
    pub fn step(&self, i: usize, d: Dir) -> Option<usize> {
        self.next[i][d.index()]
    }

    fn open_hole(&self, i: usize, s: &State) -> bool {
        self.cells[i] == Cell::Hole && !s.filled.has(i)
    }

    /// Squares the hero may walk on: no boulder, no open hole. A rolling
    /// boulder trap is fine: in Sokoban it has no boulder to launch and no
    /// confirmation prompt `[src trap.c immune_to_trap, mkroll_launch]`.
    pub fn walkable(&self, i: usize, s: &State) -> bool {
        !s.boulders.has(i)
            && match self.cells[i] {
                Cell::Floor | Cell::Stairs | Cell::RollingTrap => true,
                Cell::Hole => s.filled.has(i),
            }
    }

    pub fn reach(&self, s: &State) -> Bits {
        let mut seen = Bits::default();
        seen.set(s.hero);
        let mut stack = vec![s.hero];
        while let Some(p) = stack.pop() {
            for d in DIRS {
                if let Some(n) = self.step(p, d) {
                    if !seen.has(n) && self.walkable(n, s) {
                        seen.set(n);
                        stack.push(n);
                    }
                }
            }
        }
        seen
    }

    /// Push the boulder on `b` one square toward `d`, hero standing behind it.
    /// `None` when the boulder can't move. Stairs don't stop boulders (seen
    /// in game: Sokoban arrival stairs sit on the boulders' path). A
    /// boulder dropped on an open hole fills it. One pushed onto a rolling
    /// boulder trap rolls on the same way until it drops into the first open
    /// hole or reaches a wall; a boulder in its path takes over the roll, so
    /// boulders don't stop it `[src hack.c moverock_core, trap.c launch_obj]`.
    pub fn push(&self, s: &State, b: usize, d: Dir) -> Option<State> {
        let to = self.step(b, d)?;
        if s.boulders.has(to) {
            return None;
        }
        let mut n = *s;
        n.boulders.clear(b);
        n.hero = b;
        if self.open_hole(to, s) {
            n.filled.set(to);
            return Some(n);
        }
        let mut rest = to;
        if self.cells[to] == Cell::RollingTrap {
            while let Some(m) = self.step(rest, d) {
                if self.open_hole(m, &n) {
                    n.filled.set(m);
                    return Some(n);
                }
                rest = m;
            }
        }
        // a cradle roll ending on another boulder stacks two on one square;
        // never plan that.
        if n.boulders.has(rest) {
            return None;
        }
        n.boulders.set(rest);
        Some(n)
    }

    pub fn solved(&self, s: &State) -> bool {
        s.filled.count() as usize == self.holes.len()
    }

    /// Hero path (directions) to `to`, around boulders and open holes.
    pub fn path(&self, s: &State, to: usize) -> Option<Vec<Dir>> {
        let mut prev: Vec<Option<(usize, Dir)>> = vec![None; self.cells.len()];
        let mut seen = Bits::default();
        seen.set(s.hero);
        let mut q = VecDeque::from([s.hero]);
        while let Some(p) = q.pop_front() {
            if p == to {
                let mut dirs = vec![];
                let mut c = to;
                while let Some((from, d)) = prev[c] {
                    dirs.push(d);
                    c = from;
                }
                dirs.reverse();
                return Some(dirs);
            }
            for d in DIRS {
                if let Some(n) = self.step(p, d) {
                    if !seen.has(n) && self.walkable(n, s) {
                        seen.set(n);
                        prev[n] = Some((p, d));
                        q.push_back(n);
                    }
                }
            }
        }
        None
    }

    /// The map text with this state drawn in: parses back to the same state.
    pub fn render(&self, s: &State) -> String {
        let mut grid = self.grid.clone();
        for (i, &(x, y)) in self.pos.iter().enumerate() {
            grid[y][x] = if s.boulders.has(i) {
                '0'
            } else if i == s.hero {
                '@'
            } else {
                match self.cells[i] {
                    Cell::Floor if self.mimics.contains(&i) => 'M',
                    Cell::Floor => '.',
                    Cell::Stairs => self.grid[y][x],
                    Cell::Hole if s.filled.has(i) => '.',
                    Cell::Hole => '^',
                    Cell::RollingTrap => 'R',
                }
            };
        }
        grid.iter().map(|r| r.iter().collect::<String>().trim_end().to_string()).collect::<Vec<_>>().join("\n")
    }
}

/// Mirror a text map left-right and/or top-bottom, to match a flipped level.
pub fn flip(text: &str, horizontal: bool, vertical: bool) -> String {
    let mut rows: Vec<Vec<char>> = text.lines().map(|l| l.chars().collect()).collect();
    let width = rows.iter().map(|r| r.len()).max().unwrap_or(0);
    for r in rows.iter_mut() {
        r.resize(width, ' ');
        if horizontal {
            r.reverse();
        }
    }
    if vertical {
        rows.reverse();
    }
    rows.iter().map(|r| r.iter().collect::<String>().trim_end().to_string()).collect::<Vec<_>>().join("\n")
}

#[cfg(test)]
#[path = "level_test.rs"]
mod level_test;
