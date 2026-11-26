// Fill the holes one at a time. Each fill is a best-first search over pushes
// (the hero walks for free, a push costs 1); the search keeps the cheapest
// few ways to fill the next hole and backtracks into the next one when a
// branch dead-ends.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashSet, VecDeque};

use crate::level::{Bits, DIRS, Dir, Level, State};

/// Ways to fill the next hole kept for backtracking.
const ALTERNATIVES: usize = 3;
/// States searched over the whole run before giving up.
const LIMIT: usize = 20_000_000;

pub struct Push {
    pub boulder: usize,
    pub dir: Dir,
}

#[derive(Debug, PartialEq, Eq)]
pub enum SolveError {
    /// Searched `LIMIT` states without a solution.
    LimitReached,
    /// Every reachable state was searched: this position can't be solved.
    Unsolvable,
}

const INF: u32 = u32::MAX;

/// Pushes needed to bring a lone boulder from each cell onto the nearest hole
/// square, filled or not, ignoring other boulders. Distance to the hole line
/// rather than to the next open hole: a boulder that reaches the line rolls
/// (or is pushed) along it, and measuring to the next open hole would make
/// every filled hole raise the estimate for all the other boulders.
fn hole_distances(level: &Level) -> Vec<u32> {
    let mut dist = vec![INF; level.cells.len()];
    let mut q = VecDeque::new();
    for &hole in &level.holes {
        dist[hole] = 0;
        q.push_back(hole);
    }
    while let Some(to) = q.pop_front() {
        for d in DIRS {
            // the boulder arrived on `to` moving `d`, from `from`, pushed by a
            // hero standing behind it. A rolling boulder trap may sit there:
            // the boulder crosses it rolling, so this stays a lower bound.
            let Some(from) = level.step(to, d.opposite()) else { continue };
            if level.step(from, d.opposite()).is_none() {
                continue;
            }
            if dist[from] != INF {
                continue;
            }
            dist[from] = dist[to] + 1;
            q.push_back(from);
        }
    }
    dist
}

fn blocked(s: &State, c: Option<usize>) -> bool {
    match c {
        None => true,
        Some(i) => s.boulders.has(i),
    }
}

/// A boulder that is a corner of a 2x2 block of walls and boulders can never
/// move again.
fn frozen(level: &Level, s: &State, b: usize) -> bool {
    let corners = [
        (Dir::West, Dir::North),
        (Dir::East, Dir::North),
        (Dir::West, Dir::South),
        (Dir::East, Dir::South),
    ];
    corners.iter().any(|&(h, v)| {
        let side = level.step(b, h);
        let up = level.step(b, v);
        let diag = side.and_then(|c| level.step(c, v)).or_else(|| up.and_then(|c| level.step(c, h)));
        blocked(s, side) && blocked(s, up) && blocked(s, diag)
    })
}

/// Lower bound on pushes left; `None` when too few boulders can still reach a
/// hole to fill the rest.
fn estimate(level: &Level, dist: &[u32], s: &State) -> Option<u32> {
    let open = level.holes.len() - s.filled.count() as usize;
    let mut costs: Vec<u32> = s
        .boulders
        .iter()
        .filter(|&b| !frozen(level, s, b))
        .map(|b| dist[b])
        .filter(|&c| c != INF)
        .collect();
    if costs.len() < open {
        return None;
    }
    costs.sort_unstable();
    Some(costs[..open].iter().sum())
}

/// Boulders the hero would have to get past to stand on each square: a 0-1
/// BFS where open squares cost 0 and boulder squares 1. The boulder on
/// `target` is the one about to be pushed: the hero can't go through it.
fn access_cost(level: &Level, s: &State, target: usize) -> Vec<u32> {
    let mut cost = vec![INF; level.cells.len()];
    cost[s.hero] = 0;
    let mut q = VecDeque::from([s.hero]);
    while let Some(p) = q.pop_front() {
        for d in DIRS {
            let Some(n) = level.step(p, d) else { continue };
            if n == target {
                continue;
            }
            let (step, open) = match (s.boulders.has(n), level.walkable(n, s)) {
                (true, _) => (1, false),
                (false, true) => (0, true),
                (false, false) => continue,
            };
            if cost[p] + step < cost[n] {
                cost[n] = cost[p] + step;
                if open {
                    q.push_front(n);
                } else {
                    q.push_back(n);
                }
            }
        }
    }
    cost
}

/// Lower-bound-ish pushes until the next hole fills: for the best boulder,
/// its distance to the hole line plus the boulders in the hero's way to a
/// square that pushes it closer.
fn next_fill_estimate(level: &Level, dist: &[u32], s: &State) -> u32 {
    let mut best = INF;
    let mut candidates: Vec<usize> = s.boulders.iter().filter(|&b| dist[b] != INF && !frozen(level, s, b)).collect();
    candidates.sort_by_key(|&b| dist[b]);
    for b in candidates {
        if dist[b] >= best {
            break;
        }
        let access = access_cost(level, s, b);
        for d in DIRS {
            let (Some(stand), Some(to)) = (level.step(b, d.opposite()), level.step(b, d)) else { continue };
            if dist[to] >= dist[b] || access[stand] == INF {
                continue;
            }
            best = best.min(dist[b] + access[stand]);
        }
    }
    best
}

/// The state with the hero moved to the first square of their reachable
/// area: two states that differ only by where the hero stands inside the
/// same area are the same position.
fn region(level: &Level, s: &State) -> State {
    let reach: Bits = level.reach(s);
    State { hero: reach.iter().next().unwrap_or(s.hero), ..*s }
}

/// Positions with one more hole filled, cheapest first: the first
/// `ALTERNATIVES` the search reaches. Each comes with the pushes that get
/// there from `start`.
fn next_fills(level: &Level, dist: &[u32], start: State, budget: &mut usize) -> Vec<(State, Vec<Push>)> {
    let goal = start.filled.count() + 1;
    let mut states = vec![start];
    let mut parent: Vec<Option<(usize, usize, Dir)>> = vec![None];
    let mut seen = HashSet::from([region(level, &start)]);
    let mut heap = BinaryHeap::new();
    heap.push(Reverse((0u32, Reverse(0u32), 0usize)));
    let mut found = vec![];
    while let Some(Reverse((_, Reverse(g), id))) = heap.pop() {
        let s = states[id];
        if s.filled.count() == goal {
            let mut pushes = vec![];
            let mut cur = id;
            while let Some((p, boulder, dir)) = parent[cur] {
                pushes.push(Push { boulder, dir });
                cur = p;
            }
            pushes.reverse();
            found.push((s, pushes));
            if found.len() == ALTERNATIVES {
                break;
            }
            continue;
        }
        if *budget == 0 {
            break;
        }
        *budget -= 1;
        let reach = level.reach(&s);
        for b in s.boulders.iter() {
            for d in DIRS {
                let Some(stand) = level.step(b, d.opposite()) else { continue };
                if !reach.has(stand) {
                    continue;
                }
                let Some(n) = level.push(&s, b, d) else { continue };
                if !seen.insert(region(level, &n)) || estimate(level, dist, &n).is_none() {
                    continue;
                }
                let h = if n.filled.count() == goal { 0 } else { next_fill_estimate(level, dist, &n) };
                if h == INF {
                    continue;
                }
                states.push(n);
                parent.push(Some((id, b, d)));
                heap.push(Reverse((g + 1 + h, Reverse(g + 1), states.len() - 1)));
            }
        }
    }
    found
}

/// The pushes that fill every open hole from `start`.
pub fn solve(level: &Level, start: State) -> Result<Vec<Push>, SolveError> {
    let dist = hole_distances(level);
    if estimate(level, &dist, &start).is_none() {
        return Err(SolveError::Unsolvable);
    }
    let mut budget = LIMIT;
    let mut dead = HashSet::new();
    match fill_from(level, &dist, start, &mut budget, &mut dead) {
        Some(pushes) => Ok(pushes),
        None if budget == 0 => Err(SolveError::LimitReached),
        None => Err(SolveError::Unsolvable),
    }
}

fn fill_from(level: &Level, dist: &[u32], s: State, budget: &mut usize, dead: &mut HashSet<State>) -> Option<Vec<Push>> {
    if level.solved(&s) {
        return Some(vec![]);
    }
    if !dead.insert(region(level, &s)) {
        return None;
    }
    for (next, mut pushes) in next_fills(level, dist, s, budget) {
        if let Some(rest) = fill_from(level, dist, next, budget, dead) {
            pushes.extend(rest);
            return Some(pushes);
        }
        if *budget == 0 {
            return None;
        }
    }
    None
}

#[cfg(test)]
#[path = "solve_test.rs"]
mod solve_test;
