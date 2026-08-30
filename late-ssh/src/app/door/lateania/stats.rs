// D&D-style ability scores for Lateania characters.
//
// Six classic ability scores, rolled with 4d6-drop-lowest at character creation
// and rerollable on the selection screen until a class is chosen, then grown
// one point at a time: every `POINT_EVERY_LEVELS` levels the character earns
// a point to place on any score up to `SCORE_CAP`. Every score feeds one real
// mechanic through its D&D modifier (see `Score::rule`): Strength the swing,
// Dexterity crits, Constitution max HP, Intelligence spell power, Wisdom
// resource regen, Charisma prices and taming. The arithmetic for each hook
// lives here as a pure function so the screens that explain them and the
// combat code that applies them can never disagree. The struct
// serde-serializes into the saved-character blob and defaults every score to
// 10 (a +0 modifier), so characters saved before this system existed load
// unchanged.

use rand::Rng;
use serde::{Deserialize, Serialize};

use super::classes::Class;

/// A score can be raised this high by placed points (the D&D ceiling).
pub const SCORE_CAP: i32 = 20;
/// One attribute point is earned at every level that is a multiple of this.
pub const POINT_EVERY_LEVELS: i32 = 4;

/// Attribute points a character of `level` has earned over its whole career,
/// a pure function of level so a save can never drift from it.
pub fn points_earned(level: i32) -> i32 {
    level.clamp(0, Class::MAX_LEVEL) / POINT_EVERY_LEVELS
}

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

/// Which of the six scores. Used to ask a class for its key ability and to
/// address one score when placing a point.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Score {
    Strength,
    Dexterity,
    Constitution,
    Intelligence,
    Wisdom,
    Charisma,
}

impl Score {
    /// The six scores in sheet order, and the order the point screen keys 1-6.
    pub const ALL: [Score; 6] = [
        Score::Strength,
        Score::Dexterity,
        Score::Constitution,
        Score::Intelligence,
        Score::Wisdom,
        Score::Charisma,
    ];

    /// The three-letter label every attribute row is written with, and the one
    /// place a score's short name is spelled.
    pub fn label(self) -> &'static str {
        match self {
            Self::Strength => "STR",
            Self::Dexterity => "DEX",
            Self::Constitution => "CON",
            Self::Intelligence => "INT",
            Self::Wisdom => "WIS",
            Self::Charisma => "CHA",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Strength => "Strength",
            Self::Dexterity => "Dexterity",
            Self::Constitution => "Constitution",
            Self::Intelligence => "Intelligence",
            Self::Wisdom => "Wisdom",
            Self::Charisma => "Charisma",
        }
    }

    /// The rule behind the score, with its numbers, the way the creation
    /// screen states it. The per-character reading is `AbilityScores::effect`.
    pub fn rule(self) -> &'static str {
        match self {
            Self::Strength => "each +1 modifier: +2% swing damage",
            Self::Dexterity => {
                "each +1 modifier: +2% chance a swing crits for double (below 10: glancing blows for half)"
            }
            Self::Constitution => "each +1 modifier: +4 max HP, and +1 more every 2 levels",
            Self::Intelligence => "each +1 modifier: +2% spell power, on every ability",
            Self::Wisdom => "each +1 modifier: +1 resource regained every tick",
            Self::Charisma => "each +1 modifier: shops 3% cheaper, sells 3% dearer, taming +3%",
        }
    }
}

/// One row of the point screen: a score, what it does now, and what it would
/// do after the point. Built by the service, rendered by the UI.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScoreOfferView {
    pub label: String,
    pub name: String,
    pub value: i32,
    pub modifier: i32,
    /// What the score does right now, in numbers.
    pub now: String,
    /// What it would do one point higher; None at `SCORE_CAP`.
    pub after: Option<String>,
    /// The rule behind the score, from `Score::rule`.
    pub rule: String,
}

/// What a swing's Dexterity roll came up as. `crit_outcome` decides it; the
/// combat round applies it and says so.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CritOutcome {
    Plain,
    /// Double damage.
    Critical,
    /// Half damage, the price of a Dexterity below 10.
    Glancing,
}

/// The Dexterity roll for one swing: `crit_pct` is `AbilityScores::crit_pct`
/// (positive means a crit chance, negative a glance chance) and `roll` is a
/// uniform 0..100. Pure, so the odds are testable without dice.
pub fn crit_outcome(crit_pct: i32, roll: i32) -> CritOutcome {
    if crit_pct > 0 && roll < crit_pct {
        CritOutcome::Critical
    } else if crit_pct < 0 && roll < -crit_pct {
        CritOutcome::Glancing
    } else {
        CritOutcome::Plain
    }
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

    /// Place one earned point on `which`. False, and unchanged, at `SCORE_CAP`.
    pub fn raise(&mut self, which: Score) -> bool {
        let slot = match which {
            Score::Strength => &mut self.strength,
            Score::Dexterity => &mut self.dexterity,
            Score::Constitution => &mut self.constitution,
            Score::Intelligence => &mut self.intelligence,
            Score::Wisdom => &mut self.wisdom,
            Score::Charisma => &mut self.charisma,
        };
        if *slot >= SCORE_CAP {
            return false;
        }
        *slot += 1;
        true
    }

    /// Points the six scores can still take between them before every one
    /// is at `SCORE_CAP`. Bounds a character's unplaced points, so a point is
    /// never owed to a slot that does not exist.
    pub fn headroom(&self) -> i32 {
        Score::ALL
            .iter()
            .map(|&which| (SCORE_CAP - self.score(which)).max(0))
            .sum()
    }

    /// Strength: percent added to (or taken from) the auto-attack swing.
    pub fn swing_pct(&self) -> i32 {
        2 * modifier(self.strength)
    }

    /// Dexterity: the chance in percent a swing crits for double; negative
    /// below 10, where it is instead the chance a swing glances for half.
    pub fn crit_pct(&self) -> i32 {
        2 * modifier(self.dexterity)
    }

    /// Constitution: bonus max HP. Grows with level so a hardy (or frail)
    /// build matters more as the journey goes on, never so much that it
    /// eclipses the class HP curve.
    pub fn hp_bonus(&self, level: i32) -> i32 {
        let lvl = level.clamp(1, Class::MAX_LEVEL);
        modifier(self.constitution) * (4 + lvl / 2)
    }

    /// Intelligence: percent added to spell power, so to every ability.
    pub fn spell_power_pct(&self) -> i32 {
        2 * modifier(self.intelligence)
    }

    /// Wisdom: resource regained per tick on top of the class regen.
    pub fn regen_bonus(&self) -> i32 {
        modifier(self.wisdom)
    }

    /// Charisma: the percent shops knock off a purchase and add to a sale.
    pub fn price_pct(&self) -> i32 {
        3 * modifier(self.charisma)
    }

    /// Charisma: percent points added to a taming attempt's odds.
    pub fn tame_pct(&self) -> i32 {
        3 * modifier(self.charisma)
    }

    /// What `which` is doing for this character right now, in numbers, the
    /// line the sheet and the point screen show beside the score.
    pub fn effect(&self, which: Score, level: i32) -> String {
        match which {
            Score::Strength => format!("swings hit for {:+}%", self.swing_pct()),
            Score::Dexterity => {
                let pct = self.crit_pct();
                if pct > 0 {
                    format!("{pct}% of swings crit for double")
                } else if pct < 0 {
                    format!("{}% of swings glance for half", -pct)
                } else {
                    "no crits, no glances".to_string()
                }
            }
            Score::Constitution => format!("{:+} max HP at level {level}", self.hp_bonus(level)),
            Score::Intelligence => format!("spell power {:+}%", self.spell_power_pct()),
            Score::Wisdom => format!("{:+} resource every tick", self.regen_bonus()),
            Score::Charisma => {
                let pct = self.price_pct();
                if pct >= 0 {
                    format!(
                        "shops {pct}% cheaper, sells {pct}% dearer, taming {:+}%",
                        self.tame_pct()
                    )
                } else {
                    format!(
                        "shops {}% dearer, sells {}% cheaper, taming {:+}%",
                        -pct,
                        -pct,
                        self.tame_pct()
                    )
                }
            }
        }
    }

    /// The six scores in display order: (short label, value, modifier). Labels
    /// come from `Score::label` so a row and a class's key ability can never
    /// disagree about what a score is called.
    pub fn rows(&self) -> [(&'static str, i32, i32); 6] {
        Score::ALL.map(|which| {
            let value = self.score(which);
            (which.label(), value, modifier(value))
        })
    }
}

#[cfg(test)]
#[path = "stats_test.rs"]
mod stats_test;
