//! Forest special events: the non-combat vignettes LoGD fires before a fight on
//! roughly 15% of searches (`forestchance`). This is the stock-core set of eight
//! forest-hooked event modules from `jimlunsford/lotgd` — findgold, findgem,
//! goldmine, fairy, glowingstream, crazyaudrey, foilwench, darkhorse.
//!
//! **Licensing.** The effect mechanics (gold/gem ranges, roll tables, heal/turn
//! deltas) are transcribed 1=1 from those modules — pure numbers, uncopyrightable
//! — exactly like the creature stat blocks in [`super::data`]. All prose here is
//! **original to late.sh**; no module text is copied. One adaptation:
//! `glowingstream` rolls 1..=10 and cases 8..=10 fall through to a plain full
//! heal (the module's `default:`), kept as-is. `darkhorse` opens the real
//! tavern room (the gambler's three games, `state`'s `Mode::Tavern`); its
//! PvP enemy-intel and comment board wait for the multiplayer phase.

use rand::Rng;

use super::model::Character;

/// The eight stock forest events.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ForestEvent {
    /// Find a pile of gold (`findgold`).
    FindGold,
    /// Find a loose gem (`findgem`).
    FindGem,
    /// An abandoned mine: spend a turn to gamble for gold and gems (`goldmine`).
    GoldMine,
    /// A fairy who trades a gem for a random boon (`fairy`).
    Fairy,
    /// A glowing stream: drink for a high-variance outcome (`glowingstream`).
    GlowingStream,
    /// Mad Juna's basket game, a forest-fight gamble (`crazyaudrey`).
    PettingZoo,
    /// Lady Filigree, who trades a gem for specialty training (`foilwench`).
    Filigree,
    /// The Crooked Wheel (`darkhorse`): accepting opens the real room —
    /// the gambler's games — via the state machine, not this resolver.
    Tavern,
}

/// Every event, in declaration order.
pub const ALL: [ForestEvent; 8] = [
    ForestEvent::FindGold,
    ForestEvent::FindGem,
    ForestEvent::GoldMine,
    ForestEvent::Fairy,
    ForestEvent::GlowingStream,
    ForestEvent::PettingZoo,
    ForestEvent::Filigree,
    ForestEvent::Tavern,
];

/// Percent of forest searches that fire a special event instead of a fight,
/// matching LoGD's `forestchance` default of 15.
pub const FOREST_EVENT_PERCENT: u32 = 15;

/// Roll a uniformly-random forest event (each stock module installs at weight
/// 100, so the choice is even). The caller gates this behind
/// [`FOREST_EVENT_PERCENT`].
pub fn roll(rng: &mut impl Rng) -> ForestEvent {
    ALL[rng.gen_range(0..ALL.len())]
}

/// How an event is shown before the player acts: a title, framing narration, and
/// an optional two-way choice. A `None` choice is an instant event — the caller
/// resolves it immediately and shows the outcome with the intro.
pub struct Presentation {
    pub title: &'static str,
    pub intro: Vec<&'static str>,
    /// `(accept label, decline label)`, or `None` for an instant event.
    pub choice: Option<(&'static str, &'static str)>,
}

impl ForestEvent {
    /// The screen title for this event.
    pub fn title(self) -> &'static str {
        match self {
            ForestEvent::FindGold => "A Glint in the Dirt",
            ForestEvent::FindGem => "A Glint in the Dirt",
            ForestEvent::GoldMine => "The Abandoned Mine",
            ForestEvent::Fairy => "A Fairy in the Glade",
            ForestEvent::GlowingStream => "The Glowing Stream",
            ForestEvent::PettingZoo => "Mad Juna's Baskets",
            ForestEvent::Filigree => "Lady Filigree's Hut",
            ForestEvent::Tavern => "The Crooked Wheel",
        }
    }

    /// Build the pre-action presentation, given the current character (some
    /// events read gems/specialty to frame the choice).
    pub fn present(self, ch: &Character) -> Presentation {
        use super::model::Specialty;
        match self {
            ForestEvent::FindGold | ForestEvent::FindGem => Presentation {
                title: self.title(),
                intro: vec!["Something half-buried in the leaf litter catches the light."],
                choice: None,
            },
            ForestEvent::GoldMine => Presentation {
                title: self.title(),
                intro: vec![
                    "You stumble on the mouth of an old mine, its timbers grey with rot.",
                    "Working it would be slow going, a whole forest fight's worth of effort,",
                    "and the cave-in scars on the walls promise it isn't safe.",
                ],
                choice: Some(("Work the mine (costs a forest fight)", "Leave it be")),
            },
            ForestEvent::Fairy => {
                let intro = if ch.gems > 0 {
                    vec![
                        "A fairy darts out from under a fern.",
                        "\"A gem!\" she demands, hovering. \"Give me a gem and I'll make it worth your while.\"",
                    ]
                } else {
                    vec![
                        "A fairy darts out from under a fern.",
                        "\"A gem!\" she demands, but a glance at your empty purse leaves her unimpressed.",
                    ]
                };
                Presentation {
                    title: self.title(),
                    intro,
                    choice: Some(("Give her a gem", "Refuse")),
                }
            }
            ForestEvent::GlowingStream => Presentation {
                title: self.title(),
                intro: vec![
                    "A thread of faintly glowing water runs over pale, round stones.",
                    "The magic in it is unmistakable, and unpredictable. It might gift you power, or kill you where you kneel.",
                ],
                choice: Some(("Drink", "Walk on")),
            },
            ForestEvent::PettingZoo => Presentation {
                title: self.title(),
                intro: vec![
                    "In a too-quiet clearing sit three lidded baskets, something rustling inside.",
                    "A wild-eyed woman appears, ranting, and offers you a game: if two of her creatures match, you win her salve; if none do, you'll be sent home early.",
                ],
                choice: Some(("Play her game", "Back away")),
            },
            ForestEvent::Filigree => {
                if ch.specialty == Specialty::None {
                    // No specialty to train: a pure flavor dead-end, matching the
                    // module's "you have no direction in the world" branch.
                    Presentation {
                        title: self.title(),
                        intro: vec![
                            "You find a strange hut, but the old woman inside takes one look at you and shoos you off; you've no direction in the world for her to sharpen.",
                        ],
                        choice: None,
                    }
                } else {
                    let intro = if ch.gems > 0 {
                        vec![
                            "Inside a crooked hut, a battle-scarred crone names herself master of all skills.",
                            "\"Give me a gem,\" she says, \"and I'll teach you to advance in your craft.\"",
                        ]
                    } else {
                        vec![
                            "Inside a crooked hut, a battle-scarred crone names herself master of all skills.",
                            "She eyes your empty purse. \"Come back with a real gem, simpleton.\"",
                        ]
                    };
                    Presentation {
                        title: self.title(),
                        intro,
                        choice: Some(("Give her a gem", "Refuse")),
                    }
                }
            }
            ForestEvent::Tavern => Presentation {
                title: self.title(),
                intro: vec![
                    "A mist rolls in, and when it clears a log tavern stands before you, smoke curling from its chimney.",
                    "Through the shutters you catch lamplight, laughter, and the unmistakable rattle of dice.",
                ],
                choice: Some(("Step inside", "Move on")),
            },
        }
    }

    /// Apply the event's outcome to the character and return the lines to log.
    /// `accepted` is true for an instant event or when the player took the
    /// affirmative choice; false when they declined.
    pub fn resolve(self, accepted: bool, ch: &mut Character, rng: &mut impl Rng) -> Vec<String> {
        match self {
            ForestEvent::FindGold => {
                let gold = rng.gen_range(ch.level as u64 * 10..=ch.level as u64 * 50);
                ch.gold = ch.gold.saturating_add(gold);
                vec![format!("Fortune smiles: you dig out {gold} gold.")]
            }
            ForestEvent::FindGem => {
                ch.gems = ch.gems.saturating_add(1);
                vec!["Fortune smiles: you prise a gem from the soil.".into()]
            }
            ForestEvent::GoldMine => self.resolve_goldmine(accepted, ch, rng),
            ForestEvent::Fairy => self.resolve_fairy(accepted, ch, rng),
            ForestEvent::GlowingStream => self.resolve_stream(accepted, ch, rng),
            ForestEvent::PettingZoo => self.resolve_baskets(accepted, ch, rng),
            ForestEvent::Filigree => self.resolve_filigree(accepted, ch),
            ForestEvent::Tavern => self.resolve_tavern(accepted, ch),
        }
    }

    fn resolve_goldmine(
        self,
        accepted: bool,
        ch: &mut Character,
        rng: &mut impl Rng,
    ) -> Vec<String> {
        if !accepted {
            return vec![
                "You decide the slow way to riches isn't worth your day, and move on.".into(),
            ];
        }
        let lose_turn = |ch: &mut Character| {
            ch.turns = ch.turns.saturating_sub(1);
        };
        let lvl = ch.level as u64;
        match rng.gen_range(1..=20) {
            1..=5 => {
                lose_turn(ch);
                vec!["Hours of swinging the pick turn up nothing but worthless stones and an old skull. You lose a forest fight for the effort.".into()]
            }
            6..=10 => {
                let gold = rng.gen_range(lvl * 5..=lvl * 20);
                ch.gold = ch.gold.saturating_add(gold);
                lose_turn(ch);
                vec![format!(
                    "You chip {gold} gold out of the rock. The work costs you a forest fight."
                )]
            }
            11..=15 => {
                // Upstream gem ceiling is round(level/7)+1 (PHP round, half-up).
                let gems = rng.gen_range(1..=((ch.level as f64 / 7.0).round() as u64 + 1));
                ch.gems = ch.gems.saturating_add(gems);
                lose_turn(ch);
                vec![format!(
                    "The seam gives up {gems} gem(s). The work costs you a forest fight."
                )]
            }
            16..=18 => {
                let gold = rng.gen_range(lvl * 10..=lvl * 40);
                let gems = rng.gen_range(1..=((ch.level as f64 / 3.0).round() as u64 + 1));
                ch.gold = ch.gold.saturating_add(gold);
                ch.gems = ch.gems.saturating_add(gems);
                lose_turn(ch);
                vec![format!(
                    "A rich pocket! You haul out {gold} gold and {gems} gem(s), losing a forest fight to the labor."
                )]
            }
            // 19..=20: greed brings the roof down. The race decides whether it
            // kills (`raceminedeath`, rolled `e_rand(1,100) < chance`: 90
            // default, 5 for the Deepfolk). Death still credits 10% experience
            // ("you learned about mining") and leaves gold/gems be; a lucky
            // escape shakes you too badly to fight again today (`turns = 0`).
            _ => {
                if rng.gen_range(1..=100) < ch.race.mine_death_percent() {
                    let learned = (ch.experience as f64 * 0.1).round() as u64;
                    ch.experience = ch.experience.saturating_add(learned);
                    ch.alive = false;
                    ch.hitpoints = 0;
                    vec![format!(
                        "You spot a huge gem and swing too hard. The roof comes down in a roar of dust. In your last moments you grasp what went wrong (+{learned} experience), and that is the end of you."
                    )]
                } else {
                    ch.turns = 0;
                    let escape = if ch.race == super::model::Race::Deepfolk {
                        "You spot a huge gem and swing too hard. The roof comes down in a roar of dust, but your people were born under stone: you read the groan of the timbers and roll clear."
                    } else {
                        "You spot a huge gem and swing too hard. The roof comes down in a roar of dust, and by sheer luck you stumble clear of the fall."
                    };
                    vec![format!(
                        "{escape} The close call leaves you too shaken to face anything else today (all forest fights lost)."
                    )]
                }
            }
        }
    }

    fn resolve_fairy(self, accepted: bool, ch: &mut Character, rng: &mut impl Rng) -> Vec<String> {
        if !accepted {
            return vec!["You tell the fairy your gems are your own. She sticks out her tongue and vanishes.".into()];
        }
        if ch.gems == 0 {
            // Upstream docks a forest fight for wasting her time.
            ch.turns = ch.turns.saturating_sub(1);
            return vec!["You mime handing over a gem. The fairy, insulted at the waste of her time, hexes your feet leaden — you lose a forest fight.".into()];
        }
        ch.gems -= 1;
        // e_rand(1,7): 1 -> +1 turn, 2-3 -> +2 gems, 4-5 -> +1 max HP, 6-7 -> skill.
        match rng.gen_range(1..=7) {
            1 => {
                ch.turns = ch.turns.saturating_add(1);
                vec!["Golden dust settles over you, and you find the vigor for an extra forest fight!".into()]
            }
            2..=3 => {
                ch.gems = ch.gems.saturating_add(2);
                vec!["She points a tiny finger: two more gems glitter at your feet!".into()]
            }
            4..=5 => {
                ch.dragon_hp_bonus = ch.dragon_hp_bonus.saturating_add(1);
                ch.hitpoints = ch.hitpoints.saturating_add(1);
                vec!["A warmth spreads through you; your maximum hitpoints rise by 1!".into()]
            }
            _ => match ch.increment_specialty() {
                Some(skill) => vec![format!(
                    "She whispers a secret of your craft. Your specialty skill rises to {skill}!"
                )],
                None => vec![
                    "She whispers a secret of a craft you haven't chosen; it slips away unused."
                        .into(),
                ],
            },
        }
    }

    fn resolve_stream(self, accepted: bool, ch: &mut Character, rng: &mut impl Rng) -> Vec<String> {
        if !accepted {
            return vec![
                "You decide your luck is better spent elsewhere and leave the stream behind."
                    .into(),
            ];
        }
        // e_rand(1,10): 1 death, 2 near-death, 3 heal+turn, 4 gem, 5-7 turn+heal,
        // 8-10 (default) full heal.
        match rng.gen_range(1..=10) {
            1 => {
                ch.alive = false;
                ch.hitpoints = 0;
                // The stream's victims keep their gold and lose no experience.
                vec!["A clammy cold floods your chest. Too late you see the white stones are bleached skulls. You collapse, and do not rise.".into()]
            }
            2 => {
                ch.turns = ch.turns.saturating_sub(1);
                let floor = (ch.max_hitpoints() as f64 * 0.1).round() as u32;
                ch.hitpoints = ch.hitpoints.min(floor.max(1));
                vec!["The reaper's grip closes, then a passing fairy's dust drags you back. You survive, but barely, and a forest fight is lost.".into()]
            }
            3 => {
                ch.hitpoints = ch.max_hitpoints();
                ch.turns = ch.turns.saturating_add(1);
                vec!["You feel INVIGORATED: fully healed, and ready for one more fight in the forest!".into()]
            }
            4 => {
                ch.gems = ch.gems.saturating_add(1);
                vec!["You feel PERCEPTIVE, and notice a gem winking up from the streambed!".into()]
            }
            5..=7 => {
                // Upstream grants only the extra fight here — no heal.
                ch.turns = ch.turns.saturating_add(1);
                vec!["You feel ENERGETIC: the spring in your step is good for one more fight in the forest.".into()]
            }
            _ => {
                ch.hitpoints = ch.max_hitpoints();
                vec!["A clean warmth runs through you. Your hitpoints are restored to full.".into()]
            }
        }
    }

    fn resolve_baskets(
        self,
        accepted: bool,
        ch: &mut Character,
        rng: &mut impl Rng,
    ) -> Vec<String> {
        if !accepted {
            return vec!["You run, very quickly, away from the mad woman and her baskets.".into()];
        }
        // 1-in-20 jackpot forces all three to match (the "hedgehogs!" case).
        if rng.gen_range(1..=20) == 1 {
            ch.turns = ch.turns.saturating_add(5);
            return vec!["All three baskets burst open with the same creature! Juna shrieks with joy and drops a whole BAG of salve. You gain FIVE forest fights!".into()];
        }
        let (c1, c2, c3): (u8, u8, u8) = (
            rng.gen_range(0..4),
            rng.gen_range(0..4),
            rng.gen_range(0..4),
        );
        if c1 == c2 && c2 == c3 {
            ch.turns = ch.turns.saturating_add(2);
            vec!["All three match! Juna grudgingly grants you two salves. You gain TWO forest fights!".into()]
        } else if c1 == c2 || c2 == c3 || c1 == c3 {
            ch.turns = ch.turns.saturating_add(1);
            vec!["Two of a kind! Juna hands over a single salve. You gain a forest fight!".into()]
        } else if ch.turns > 0 {
            ch.turns -= 1;
            vec!["No two alike. \"Off to bed early for you!\" Juna cackles, and you lose a forest fight.".into()]
        } else {
            // No fight left to dock: upstream takes a charm point instead.
            ch.charm = ch.charm.saturating_sub(1);
            vec!["No two alike, and no fight left to lose. Juna settles for mocking you until your pride stings (-1 charm).".into()]
        }
    }

    fn resolve_filigree(self, accepted: bool, ch: &mut Character) -> Vec<String> {
        use super::model::Specialty;
        if ch.specialty == Specialty::None {
            return vec!["The crone has nothing to teach someone with no direction.".into()];
        }
        if !accepted {
            return vec!["You tell her to earn her own riches, and stomp out.".into()];
        }
        if ch.gems == 0 {
            return vec!["You offer an imaginary gem. \"Come back with a real one, simpleton,\" she says, and throws you out.".into()];
        }
        ch.gems -= 1;
        match ch.increment_specialty() {
            Some(skill) => vec![format!(
                "She presses a slip of instruction into your hand. Your specialty skill rises to {skill}!"
            )],
            None => vec!["She sighs and waves you off.".into()],
        }
    }

    fn resolve_tavern(self, accepted: bool, _ch: &mut Character) -> Vec<String> {
        // Accepting never reaches this resolver: the state machine intercepts
        // it and opens the tavern room (`Mode::Tavern`) instead.
        if !accepted {
            return vec!["You leave the strange tavern to its noise and walk on.".into()];
        }
        Vec::new()
    }
}


