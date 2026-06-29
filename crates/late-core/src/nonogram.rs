use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use std::hash::{Hash, Hasher};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NonogramPuzzle {
    pub id: String,
    pub width: u16,
    pub height: u16,
    pub row_clues: Vec<Vec<u8>>,
    pub col_clues: Vec<Vec<u8>>,
    pub solution: Vec<Vec<u8>>,
    pub difficulty: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NonogramPack {
    pub size_key: String,
    pub width: u16,
    pub height: u16,
    pub puzzles: Vec<NonogramPuzzle>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NonogramPackIndexEntry {
    pub size_key: String,
    pub width: u16,
    pub height: u16,
    pub puzzle_count: usize,
    pub path: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NonogramPackIndex {
    pub version: u32,
    pub packs: Vec<NonogramPackIndexEntry>,
}

impl NonogramPuzzle {
    pub fn validate(&self) -> anyhow::Result<()> {
        if self.solution.len() != usize::from(self.height) {
            anyhow::bail!(
                "puzzle {} height mismatch: expected {}, got {}",
                self.id,
                self.height,
                self.solution.len()
            );
        }

        for row in &self.solution {
            if row.len() != usize::from(self.width) {
                anyhow::bail!(
                    "puzzle {} width mismatch: expected {}, got {}",
                    self.id,
                    self.width,
                    row.len()
                );
            }

            if row.iter().any(|cell| *cell > 1) {
                anyhow::bail!("puzzle {} contains non-binary solution values", self.id);
            }
        }

        let (row_clues, col_clues) = derive_clues(&self.solution);
        if row_clues != self.row_clues {
            anyhow::bail!("puzzle {} row clues do not match solution", self.id);
        }
        if col_clues != self.col_clues {
            anyhow::bail!("puzzle {} column clues do not match solution", self.id);
        }

        Ok(())
    }
}

impl NonogramPack {
    pub fn validate(&self) -> anyhow::Result<()> {
        for puzzle in &self.puzzles {
            if puzzle.width != self.width || puzzle.height != self.height {
                anyhow::bail!(
                    "puzzle {} dimensions do not match pack {}",
                    puzzle.id,
                    self.size_key
                );
            }
            puzzle.validate()?;
        }
        Ok(())
    }

    pub fn select_for_date(&self, date: NaiveDate) -> Option<&NonogramPuzzle> {
        if self.puzzles.is_empty() {
            return None;
        }

        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.size_key.hash(&mut hasher);
        date.format("%Y-%m-%d").to_string().hash(&mut hasher);
        "late-sh-nonogram-daily".hash(&mut hasher);
        let idx = (hasher.finish() as usize) % self.puzzles.len();
        self.puzzles.get(idx)
    }
}

pub fn derive_clues(solution: &[Vec<u8>]) -> (Vec<Vec<u8>>, Vec<Vec<u8>>) {
    if solution.is_empty() {
        return (Vec::new(), Vec::new());
    }

    let row_clues = solution
        .iter()
        .map(|row| derive_line_clues(row.iter().copied()))
        .collect();

    let width = solution[0].len();
    let mut col_clues = Vec::with_capacity(width);
    for col in 0..width {
        let line = solution.iter().map(|row| row[col]);
        col_clues.push(derive_line_clues(line));
    }

    (row_clues, col_clues)
}

fn derive_line_clues<I>(cells: I) -> Vec<u8>
where
    I: IntoIterator<Item = u8>,
{
    let mut clues = Vec::new();
    let mut run = 0u8;

    for cell in cells {
        if cell == 1 {
            run = run.saturating_add(1);
        } else if run > 0 {
            clues.push(run);
            run = 0;
        }
    }

    if run > 0 {
        clues.push(run);
    }

    clues
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derives_expected_clues() {
        let solution = vec![
            vec![0, 1, 1, 0, 1],
            vec![1, 1, 0, 0, 0],
            vec![0, 0, 0, 0, 0],
            vec![1, 0, 1, 1, 1],
            vec![0, 0, 1, 0, 0],
        ];

        let (rows, cols) = derive_clues(&solution);
        assert_eq!(rows, vec![vec![2, 1], vec![2], vec![], vec![1, 3], vec![1]]);
        assert_eq!(
            cols,
            vec![vec![1, 1], vec![2], vec![1, 2], vec![1], vec![1, 1]]
        );
    }

    #[test]
    fn daily_selection_is_deterministic() {
        let pack = NonogramPack {
            size_key: "5x5".to_string(),
            width: 5,
            height: 5,
            puzzles: (0..4)
                .map(|idx| NonogramPuzzle {
                    id: format!("5x5-{idx:06}"),
                    width: 5,
                    height: 5,
                    row_clues: vec![vec![1]; 5],
                    col_clues: vec![vec![1]; 5],
                    solution: vec![vec![1, 0, 0, 0, 0]; 5],
                    difficulty: "easy".to_string(),
                    source: None,
                    seed: Some(idx),
                })
                .collect(),
        };

        let date = NaiveDate::from_ymd_opt(2026, 3, 28).expect("date");
        let a = pack.select_for_date(date).expect("puzzle").id.clone();
        let b = pack.select_for_date(date).expect("puzzle").id.clone();
        assert_eq!(a, b);
    }
}
