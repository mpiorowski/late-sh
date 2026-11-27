// Turn a push list into keys: walk to each boulder, then push it.

use crate::level::{Dir, Level, State};
use crate::solve::Push;

/// One key and the position once the game has taken it.
pub struct Step {
    pub key: char,
    pub after: State,
}

/// A run of pushes of one boulder the same way, with the walk before it.
pub struct Batch {
    /// Map column and row of the boulder when the batch starts.
    pub from: (usize, usize),
    pub dir: Dir,
    pub pushes: u32,
    pub keys: String,
    pub after: State,
}

pub fn steps(level: &Level, start: State, pushes: &[Push]) -> Vec<Step> {
    let mut out = vec![];
    let mut s = start;
    for p in pushes {
        let stand = level.step(p.boulder, p.dir.opposite()).expect("solver pushes from a floor square");
        for d in level.path(&s, stand).expect("solver pushes from a reachable square") {
            s.hero = level.step(s.hero, d).expect("paths stay on the map");
            out.push(Step { key: d.key(), after: s });
        }
        s = level.push(&s, p.boulder, p.dir).expect("solver pushes are legal");
        out.push(Step { key: p.dir.key(), after: s });
    }
    out
}

pub fn batches(level: &Level, start: State, pushes: &[Push]) -> Vec<Batch> {
    let mut out: Vec<Batch> = vec![];
    let mut s = start;
    let mut moving: Option<usize> = None; // where the last pushed boulder now sits
    for p in pushes {
        let stand = level.step(p.boulder, p.dir.opposite()).expect("solver pushes from a floor square");
        let walk: String = level.path(&s, stand).expect("solver pushes from a reachable square").iter().map(|d| d.key()).collect();
        let next = level.push(&s, p.boulder, p.dir).expect("solver pushes are legal");
        let continues = walk.is_empty() && moving == Some(p.boulder) && out.last().is_some_and(|b| b.dir == p.dir);
        match out.last_mut() {
            Some(last) if continues => {
                last.pushes += 1;
                last.keys.push(p.dir.key());
                last.after = next;
            }
            _ => out.push(Batch {
                from: level.pos[p.boulder],
                dir: p.dir,
                pushes: 1,
                keys: format!("{walk}{}", p.dir.key()),
                after: next,
            }),
        }
        moving = level.step(p.boulder, p.dir).filter(|&to| next.boulders.has(to));
        s = next;
    }
    out
}

#[cfg(test)]
#[path = "keys_test.rs"]
mod keys_test;
