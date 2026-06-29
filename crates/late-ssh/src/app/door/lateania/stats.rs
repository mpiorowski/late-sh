// D&D-style ability scores for Lateania characters.
//
// Six classic ability scores, rolled with 4d6-drop-lowest at character creation
// and rerollable on the selection screen until a class is chosen. Scores feed
// real mechanics through their D&D modifiers: Constitution hardens the body
// (bonus max HP) and each class's key ability sharpens its strikes (bonus
// attack). The struct serde-serializes into the saved-character blob and
// defaults every score to 10 (a +0 modifier), so characters saved before this
// system existed load unchanged.

use rand::Rng;
use serde::{Deserialize, Serialize};

use super::classes::Class;

fn ten() -> i32 {
    10
}

/// The six classic ability scores. A score of 10 is the unremarkable human
/// average and yields a +0 modifier.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AbilityScores {
    #[serde(default = "ten")]
    pub strength: i32,
    #[serde(default = "ten")]
    pub dexterity: i32,
    #[serde(default = "ten")]
    pub constitution: i32,
    #[serde(default = "ten")]
    pub intelligence: i32,
    #[serde(default = "ten")]
    pub wisdom: i32,
    #[serde(default = "ten")]
    pub charisma: i32,
}

impl Default for AbilityScores {
    fn default() -> Self {
        Self {
            strength: 10,
            dexterity: 10,
            constitution: 10,
            intelligence: 10,
            wisdom: 10,
            charisma: 10,
        }
    }
}

/// Which of the six scores. Used to ask a class for its key ability.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Score {
    Strength,
    Dexterity,
    Constitution,
    Intelligence,
    Wisdom,
    Charisma,
}

/// The D&D ability modifier for a score: floor((score - 10) / 2). div_euclid
/// floors toward negative infinity, so a score of 7 correctly yields -2.
pub fn modifier(score: i32) -> i32 {
    (score - 10).div_euclid(2)
}

/// Roll one ability score as 4d6, dropping the lowest die - the classic heroic
/// roll, which centers a touch above the flat 3d6 average.
fn roll_one(rng: &mut impl Rng) -> i32 {
    let mut dice = [
        rng.gen_range(1..=6),
        rng.gen_range(1..=6),
        rng.gen_range(1..=6),
        rng.gen_range(1..=6),
    ];
    dice.sort_unstable();
    dice[1] + dice[2] + dice[3] // sum the top three; drop dice[0], the lowest
}

impl AbilityScores {
    /// Roll a fresh set of six scores, 4d6-drop-lowest each.
    pub fn roll() -> Self {
        let mut rng = rand::thread_rng();
        Self {
            strength: roll_one(&mut rng),
            dexterity: roll_one(&mut rng),
            constitution: roll_one(&mut rng),
            intelligence: roll_one(&mut rng),
            wisdom: roll_one(&mut rng),
            charisma: roll_one(&mut rng),
        }
    }

    pub fn score(&self, which: Score) -> i32 {
        match which {
            Score::Strength => self.strength,
            Score::Dexterity => self.dexterity,
            Score::Constitution => self.constitution,
            Score::Intelligence => self.intelligence,
            Score::Wisdom => self.wisdom,
            Score::Charisma => self.charisma,
        }
    }

    /// Bonus max HP granted by Constitution. Scales gently with level so a hardy
    /// (or frail) build matters more as the journey goes on - and never so much
    /// that it eclipses the class HP curve.
    pub fn hp_bonus(&self, level: i32) -> i32 {
        let lvl = level.clamp(1, Class::MAX_LEVEL);
        modifier(self.constitution) * (2 + lvl / 4)
    }

    /// Bonus attack granted by the class's key ability score.
    pub fn attack_bonus(&self, class: Class) -> i32 {
        modifier(self.score(class.primary_score()))
    }

    /// The six scores in display order: (short label, value, modifier).
    pub fn rows(&self) -> [(&'static str, i32, i32); 6] {
        [
            ("STR", self.strength, modifier(self.strength)),
            ("DEX", self.dexterity, modifier(self.dexterity)),
            ("CON", self.constitution, modifier(self.constitution)),
            ("INT", self.intelligence, modifier(self.intelligence)),
            ("WIS", self.wisdom, modifier(self.wisdom)),
            ("CHA", self.charisma, modifier(self.charisma)),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modifier_follows_the_dnd_rule() {
        assert_eq!(modifier(10), 0);
        assert_eq!(modifier(11), 0);
        assert_eq!(modifier(12), 1);
        assert_eq!(modifier(8), -1);
        assert_eq!(modifier(7), -2);
        assert_eq!(modifier(18), 4);
        assert_eq!(modifier(3), -4);
    }

    #[test]
    fn rolls_are_in_the_4d6_drop_lowest_range() {
        // Top three of 4d6 can range 3..=18; check many rolls stay in-band.
        for _ in 0..2000 {
            let s = AbilityScores::roll();
            for (_, value, _) in s.rows() {
                assert!((3..=18).contains(&value), "score {value} out of 4d6 range");
            }
        }
    }

    #[test]
    fn defaults_are_neutral() {
        let s = AbilityScores::default();
        for (_, value, modifier) in s.rows() {
            assert_eq!(value, 10);
            assert_eq!(modifier, 0);
        }
    }
}
