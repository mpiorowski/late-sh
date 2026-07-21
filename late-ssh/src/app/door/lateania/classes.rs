// Character classes for Lateania.
//
// Twelve classes, each with a distinct resource, a passive class trait, a rich
// description, and a 50-level progression. Progression is formula-driven (data,
// not a hand-typed table) so balance lives in one place. Abilities unlock by
// level in abilities.rs.

/// The playable classes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Class {
    Warrior,
    Mage,
    Cleric,
    Rogue,
    Ranger,
    Druid,
    Necromancer,
    Bard,
    Monk,
    Paladin,
    Warlock,
    Berserker,
    Beastlord,
    Skald,
    Runemaster,
    Valewalker,
    Spiritmaster,
}

/// The resource a class spends on abilities.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Resource {
    Rage,
    Mana,
    Energy,
    Focus,
    Spirit,
    Souls,
    Tempo,
    Ki,
}

impl Resource {
    pub fn label(self) -> &'static str {
        match self {
            Self::Rage => "Rage",
            Self::Mana => "Mana",
            Self::Energy => "Energy",
            Self::Focus => "Focus",
            Self::Spirit => "Spirit",
            Self::Souls => "Souls",
            Self::Tempo => "Tempo",
            Self::Ki => "Ki",
        }
    }
}

/// Per-level stat shape for one class, computed from the level.
#[derive(Clone, Copy, Debug)]
pub struct ClassStats {
    pub max_hp: i32,
    pub max_resource: i32,
    pub attack: i32,
    /// Resource regained per world tick.
    pub resource_regen: i32,
}

impl Class {
    pub const ALL: [Class; 17] = [
        Class::Warrior,
        Class::Mage,
        Class::Cleric,
        Class::Rogue,
        Class::Ranger,
        Class::Druid,
        Class::Necromancer,
        Class::Bard,
        Class::Monk,
        Class::Paladin,
        Class::Warlock,
        Class::Berserker,
        Class::Beastlord,
        Class::Skald,
        Class::Runemaster,
        Class::Valewalker,
        Class::Spiritmaster,
    ];

    /// The hard level ceiling. Reaching it is the long game.
    pub const MAX_LEVEL: i32 = 50;

    pub fn name(self) -> &'static str {
        match self {
            Self::Warrior => "Warrior",
            Self::Mage => "Mage",
            Self::Cleric => "Cleric",
            Self::Rogue => "Rogue",
            Self::Ranger => "Ranger",
            Self::Druid => "Druid",
            Self::Necromancer => "Necromancer",
            Self::Bard => "Bard",
            Self::Monk => "Monk",
            Self::Paladin => "Paladin",
            Self::Warlock => "Warlock",
            Self::Berserker => "Berserker",
            Self::Beastlord => "Beastlord",
            Self::Skald => "Skald",
            Self::Runemaster => "Runemaster",
            Self::Valewalker => "Valewalker",
            Self::Spiritmaster => "Spiritmaster",
        }
    }

    /// The ability score that sharpens this class's strikes (its attack key).
    pub fn primary_score(self) -> super::stats::Score {
        use super::stats::Score;
        match self {
            Self::Warrior => Score::Strength,
            Self::Mage => Score::Intelligence,
            Self::Cleric => Score::Wisdom,
            Self::Rogue => Score::Dexterity,
            Self::Ranger => Score::Dexterity,
            Self::Druid => Score::Wisdom,
            Self::Necromancer => Score::Intelligence,
            Self::Bard => Score::Charisma,
            Self::Monk => Score::Dexterity,
            Self::Paladin => Score::Strength,
            Self::Warlock => Score::Charisma,
            Self::Berserker => Score::Strength,
            Self::Beastlord => Score::Wisdom,
            Self::Skald => Score::Charisma,
            Self::Runemaster => Score::Intelligence,
            Self::Valewalker => Score::Strength,
            Self::Spiritmaster => Score::Charisma,
        }
    }

    pub fn resource(self) -> Resource {
        match self {
            Self::Warrior => Resource::Rage,
            Self::Mage => Resource::Mana,
            Self::Cleric => Resource::Mana,
            Self::Rogue => Resource::Energy,
            Self::Ranger => Resource::Focus,
            Self::Druid => Resource::Spirit,
            Self::Necromancer => Resource::Souls,
            Self::Bard => Resource::Tempo,
            Self::Monk => Resource::Ki,
            Self::Paladin => Resource::Mana,
            Self::Warlock => Resource::Mana,
            Self::Berserker => Resource::Rage,
            Self::Beastlord => Resource::Spirit,
            Self::Skald => Resource::Tempo,
            Self::Runemaster => Resource::Mana,
            Self::Valewalker => Resource::Focus,
            Self::Spiritmaster => Resource::Souls,
        }
    }

    /// A one-line role summary for the character sheet.
    pub fn tagline(self) -> &'static str {
        match self {
            Self::Warrior => "Frontline bulwark - trades blows and outlasts.",
            Self::Mage => "Glass-cannon spellcaster - immense burst, fragile frame.",
            Self::Cleric => "Holy battle-healer - sustains, smites the undead.",
            Self::Rogue => "Lethal duelist - stealth, poison, and sudden death.",
            Self::Ranger => "Patient hunter - ranged pressure and field-craft.",
            Self::Druid => "Wild shapeshifter - nature's mercy and its teeth alike.",
            Self::Necromancer => "Master of death - drains the living, harvests the slain.",
            Self::Bard => "Battle-singer - buffs allies, jeers foes, never misses a beat.",
            Self::Monk => "Martial ascetic - flowing strikes and an unbreakable body.",
            Self::Paladin => "Holy bulwark - shields the line and mends it in one breath.",
            Self::Warlock => "Pact-bound caster - feeds foes to the dark for power.",
            Self::Berserker => "Reckless juggernaut - hits hardest as death draws near.",
            Self::Beastlord => "Beast-tamer warlord - fights beside an empowered companion.",
            Self::Skald => "Norse battle-singer - war-chants that quicken and embolden.",
            Self::Runemaster => "Rune-carver - burst nukes and a slow-burning graven rune.",
            Self::Valewalker => "Scythe-warrior of the wild - sustained melee, mends on the cut.",
            Self::Spiritmaster => "Spirit-binder - shadow curses that fester and feed you.",
        }
    }

    /// The flavorful long description shown when choosing or inspecting a class.
    pub fn description(self) -> &'static str {
        match self {
            Self::Warrior => {
                "Where the line breaks, the Warrior stands. Clad in iron and \
                certainty, they read a battle in the rhythm of falling blows and answer it \
                with their own. Rage is their fuel: it does not pool while they rest but \
                kindles in the fight itself, every wound taken and given stoking it higher \
                until they end the matter with a single, ruinous stroke. Warriors do not \
                dazzle. They endure, and what they endure, they outlive."
            }
            Self::Mage => {
                "The Mage holds the oldest and most dangerous bargain: power \
                without armor, knowledge without mercy. They unmake the world in syllables, \
                calling fire that clings, frost that locks the joints, and lightning that \
                forgets nothing it touches. Mana is their well, deep but not bottomless, and \
                a Mage caught between spells is a candle in a gale. Strike first, strike \
                hardest, and never let the enemy close the distance."
            }
            Self::Cleric => {
                "The Cleric carries the Dawn into dark places. Theirs is the \
                hardest road: to mend and to smite with the same hand, to stand in the ruin \
                and refuse to let a companion fall. Holy fire answers the wicked and \
                searing light judges the undead, while a whispered prayer knits torn flesh \
                whole. A party with a Cleric is a party that comes home; a Cleric alone is \
                a quiet, patient kind of unkillable."
            }
            Self::Rogue => {
                "The Rogue settles fights before they are fairly begun. They \
                trade plate for shadow and brawn for precision, finding the gap in the \
                guard, the vein that will not close, the breath of inattention that ends a \
                life. Energy floods back swiftly, rewarding the quick and the cruel with \
                flurry after flurry. A Rogue who is seen has already made a mistake; a Rogue \
                who is not will open you from hip to throat and be gone."
            }
            Self::Ranger => {
                "The Ranger belongs to the long marches and the patient kill. \
                Bow in hand and the wilds at their back, they wear the enemy down from a \
                distance no blade can answer, layering venom and volley and the cold \
                wisdom of a hundred camps. Focus is their discipline, spent on shots that \
                never waste and traps that never miss. Give a Ranger room and time, and the \
                fight is already lost - the quarry simply has not been told yet."
            }
            Self::Druid => {
                "The Druid keeps the old covenant with the wild, and the wild keeps it \
                back. They speak to root and storm and the slow green patience of growing \
                things, calling thorns from bare stone and rain from a clear sky, then \
                mending what the fight has torn as easily as breathing. Spirit is their \
                tether to the living world; while it holds, so do they. A Druid does not \
                so much win a battle as outlast the season of it - bending, never breaking, \
                until the land itself decides the matter."
            }
            Self::Necromancer => {
                "The Necromancer studies the one door everyone passes through, and has \
                learned to make it swing both ways. Where others see a corpse, they see \
                fuel; where others mourn, they harvest. Shadow answers their call, draining \
                the warmth from the living to feed their own cold endurance, and every foe \
                that falls before them yields up its Souls to be spent again. They are not \
                hated for cruelty so much as for candor - they simply refuse to pretend \
                that death is the end of anything useful."
            }
            Self::Bard => {
                "The Bard fights the way others can only dream of arguing - with timing, \
                with wit, and with a song that turns a doomed skirmish into a story worth \
                telling. Their power is Tempo, kept by ear and spent in verses that mend an \
                ally, hearten a line, or unstring a foe's nerve at the worst possible moment. \
                Underestimated until the chorus hits, a Bard is the reason the survivors have \
                something to sing about at all."
            }
            Self::Monk => {
                "The Monk has spent a lifetime making a weapon of the only thing they were \
                born with. Every breath is discipline, every strike a sentence finished \
                before the enemy hears it begin. Ki flows where attention goes, spent on \
                flurries that blur the eye and on a stillness so complete that blows simply \
                fail to land. They own nothing and need less, and that is exactly what makes \
                them so hard to stop."
            }
            Self::Paladin => {
                "The Paladin is an oath given flesh. Where the Cleric tends and the Warrior \
                endures, the Paladin does both at once - a wall of blessed steel that mends \
                the line it holds and brings holy fire down on whatever broke against it. \
                Their Mana is faith made usable, poured out in shields and smitings without \
                much daylight between the two. To stand beside a Paladin is to be told, \
                wordlessly, that you will not fall today."
            }
            Self::Warlock => {
                "The Warlock signed something, once, that they will not discuss. What they \
                got in return was leverage over the dark - curses that fester, flame born of \
                bargains, and a hunger that the gathered dead keep fed. Mana is the form their \
                pact takes, replenished by the dying, spent without restraint. A Warlock is \
                not reckless so much as certain: they have already paid the worst price, and \
                everything after is simply spending what they bought."
            }
            Self::Berserker => {
                "The Berserker has no plan and needs none. Where the Warrior reads a battle, \
                the Berserker becomes one - a rising tide of Rage that burns hotter the closer \
                they come to the end, until a creature half-dead is twice as dangerous as it \
                had any right to be. They do not parry, they do not retreat, they do not stop. \
                Win quickly, the wise say, or do not fight a Berserker at all."
            }
            Self::Beastlord => {
                "The Beastlord never walks alone. Where others learned the sword, they \
                learned the wild - to read the set of an ear, the meaning of a growl, and \
                to earn a loyalty no coin could buy. In the fight they are the lesser \
                danger: it is the thing at their side, made greater by the bond between \
                them, that opens the enemy's throat. Their Spirit is the tether of that \
                bond, and a Beastlord with a strong companion is worth two of anyone else."
            }
            Self::Skald => {
                "The Skald fights with axe in one hand and a saga in the other. Every blow \
                is a line of verse and every verse a spur, quickening the blood of those \
                who stand with them until a tired shield-wall finds it can swing all night. \
                Tempo is the meter they keep, spent on war-chants that hasten and embolden. \
                When the fight is over the Skald is already composing how it will be sung - \
                and they mean to be in the chorus, not the elegy."
            }
            Self::Runemaster => {
                "The Runemaster does not cast so much as carve. Each spell is a rune scored \
                into the air with a blade of will, holding raw force until it is loosed - a \
                blaze of power now, or a graven mark left to smoulder and then detonate. \
                Mana is the ink of that carving, deep and precise. Patient where the Mage is \
                frantic, a Runemaster sets their runes early and lets the battlefield come \
                apart on their own schedule."
            }
            Self::Valewalker => {
                "The Valewalker keeps the green marches with a scythe meant for more than \
                grain. Half warrior and half warden, they wade into the fight and take root, \
                each reaping stroke drawing a little of the wild's own vigour back into their \
                frame. Focus is their patience, spent on cuts that never tire. They do not \
                dazzle and they do not fall - they simply keep swinging, mending as they go, \
                until the harvest is in."
            }
            Self::Spiritmaster => {
                "The Spiritmaster walks with a retinue no one else can see. They bind the \
                lingering dead and the hungry spirits of dark places, setting them upon the \
                living to gnaw and wither and drink. Souls are the coin of that binding, paid \
                by the dying. A Spiritmaster rarely strikes a blow themselves; they need not, \
                when the air around their enemy is already full of teeth - and every scream \
                feeds them a little more."
            }
        }
    }

    /// The passive class trait: a defining, always-on edge.
    pub fn trait_name(self) -> &'static str {
        match self {
            Self::Warrior => "Unbreakable",
            Self::Mage => "Arcane Mastery",
            Self::Cleric => "Light of the Dawn",
            Self::Rogue => "Opportunist",
            Self::Ranger => "Hunter's Instinct",
            Self::Druid => "Nature's Renewal",
            Self::Necromancer => "Soul Harvest",
            Self::Bard => "Battle Hymn",
            Self::Monk => "Iron Body",
            Self::Paladin => "Aura of Devotion",
            Self::Warlock => "Pact of Souls",
            Self::Berserker => "Frenzy",
            Self::Beastlord => "Pack Bond",
            Self::Skald => "War-Chant",
            Self::Runemaster => "Runic Overflow",
            Self::Valewalker => "Reaping Harvest",
            Self::Spiritmaster => "Spirit Siphon",
        }
    }

    pub fn trait_desc(self) -> &'static str {
        match self {
            Self::Warrior => {
                "The first killing blow each fight is survived at 1 HP instead of falling."
            }
            Self::Mage => "Every offensive spell strikes for extra arcane damage.",
            Self::Cleric => "All healing is amplified, and the undead take added holy damage.",
            Self::Rogue => "The opening strike of a fight always lands as a critical hit.",
            Self::Ranger => "Strikes against a wounded foe (below half health) hit harder.",
            Self::Druid => "The living world mends you: you regenerate health every few moments.",
            Self::Necromancer => {
                "Each foe you slay yields its life force, restoring health and Souls."
            }
            Self::Bard => {
                "Your song keeps perfect time: Tempo returns faster than any other resource."
            }
            Self::Monk => "Your trained body blunts physical blows, taking reduced melee damage.",
            Self::Paladin => {
                "A holy aura mends you steadily, regenerating health every few moments."
            }
            Self::Warlock => "Each foe you slay feeds your pact, restoring a surge of Mana.",
            Self::Berserker => "The closer you are to death, the harder your blows land.",
            Self::Beastlord => {
                "Your bond empowers your companion: it hits harder, is hardier, and looses \
                its skills more often."
            }
            Self::Skald => {
                "Your song keeps perfect time: Tempo returns faster than any other resource."
            }
            Self::Runemaster => "Every offensive spell strikes for extra arcane damage.",
            Self::Valewalker => "Each melee strike you land mends a little of your own hurt.",
            Self::Spiritmaster => {
                "Each foe you slay yields its spirit, restoring health and Souls."
            }
        }
    }

    /// Full stat block at a given level. Linear-plus-curve growth keeps all five
    /// classes climbing meaningfully to level 50.
    pub fn stats_at(self, level: i32) -> ClassStats {
        let lvl = level.clamp(1, Self::MAX_LEVEL);
        let l = lvl - 1; // levels gained past 1
        match self {
            Self::Warrior => ClassStats {
                max_hp: 48 + l * 12,
                max_resource: 100,
                attack: 6 + l * 2,
                resource_regen: 6,
            },
            Self::Mage => ClassStats {
                max_hp: 30 + l * 7,
                max_resource: 60 + l * 4,
                attack: 5 + l * 2,
                resource_regen: 7,
            },
            Self::Cleric => ClassStats {
                max_hp: 38 + l * 9,
                max_resource: 55 + l * 4,
                attack: 5 + (l * 3) / 2,
                resource_regen: 6,
            },
            Self::Rogue => ClassStats {
                max_hp: 34 + l * 8,
                max_resource: 100,
                attack: 6 + l * 2,
                resource_regen: 12,
            },
            Self::Ranger => ClassStats {
                max_hp: 36 + l * 8,
                max_resource: 80 + l * 2,
                attack: 6 + l * 2,
                resource_regen: 9,
            },
            // Hybrid bruiser-healer: hardy and steady, like the Cleric but greener.
            Self::Druid => ClassStats {
                max_hp: 40 + l * 9,
                max_resource: 70 + l * 3,
                attack: 5 + (l * 3) / 2,
                resource_regen: 7,
            },
            // A caster a touch hardier than the Mage - undeath lends some grit.
            Self::Necromancer => ClassStats {
                max_hp: 32 + l * 8,
                max_resource: 60 + l * 4,
                attack: 5 + l * 2,
                resource_regen: 6,
            },
            // Support hybrid: middling frame, deep and fast-flowing Tempo.
            Self::Bard => ClassStats {
                max_hp: 36 + l * 8,
                max_resource: 80 + l * 3,
                attack: 5 + (l * 3) / 2,
                resource_regen: 10,
            },
            // Nimble martialist: hardy for its speed, attacks like a Rogue.
            Self::Monk => ClassStats {
                max_hp: 38 + l * 9,
                max_resource: 100,
                attack: 6 + l * 2,
                resource_regen: 11,
            },
            // Holy bulwark: nearly as tough as the Warrior, with Mana to spend.
            Self::Paladin => ClassStats {
                max_hp: 46 + l * 11,
                max_resource: 60 + l * 3,
                attack: 5 + (l * 3) / 2,
                resource_regen: 6,
            },
            // Pact caster: glass like the Mage, fueled by the dying.
            Self::Warlock => ClassStats {
                max_hp: 30 + l * 7,
                max_resource: 60 + l * 4,
                attack: 5 + l * 2,
                resource_regen: 6,
            },
            // Reckless glass cannon: a heavy swing and the game's hardest-hitting
            // Frenzy, paid for by a frame thinner than the Warrior's - the closer
            // to death, the more dangerous, because death is genuinely close.
            Self::Berserker => ClassStats {
                max_hp: 42 + l * 10,
                max_resource: 100,
                attack: 7 + l * 2,
                resource_regen: 7,
            },
            // Beastlord: a sturdy companion-fighter - hardy like the Ranger, its own
            // strikes middling because the empowered beast does the heavy work.
            Self::Beastlord => ClassStats {
                max_hp: 40 + l * 9,
                max_resource: 70 + l * 3,
                attack: 5 + (l * 3) / 2,
                resource_regen: 7,
            },
            // Skald: a support-bruiser like the Bard, with deep, fast-flowing Tempo.
            Self::Skald => ClassStats {
                max_hp: 38 + l * 8,
                max_resource: 80 + l * 3,
                attack: 6 + (l * 3) / 2,
                resource_regen: 10,
            },
            // Runemaster: a glass burst-caster like the Mage.
            Self::Runemaster => ClassStats {
                max_hp: 30 + l * 7,
                max_resource: 60 + l * 4,
                attack: 5 + l * 2,
                resource_regen: 7,
            },
            // Valewalker: a self-sustaining melee warrior, tough and steady.
            Self::Valewalker => ClassStats {
                max_hp: 42 + l * 10,
                max_resource: 90,
                attack: 6 + l * 2,
                resource_regen: 9,
            },
            // Spiritmaster: a DoT caster a touch hardier than the Mage, fed by the dying.
            Self::Spiritmaster => ClassStats {
                max_hp: 32 + l * 8,
                max_resource: 60 + l * 4,
                attack: 5 + l * 2,
                resource_regen: 6,
            },
        }
    }

    pub fn from_index(i: usize) -> Class {
        Self::ALL[i % Self::ALL.len()]
    }

    /// Stable lowercase key for persistence (never reorder these strings).
    /// Whether this class can call a fallen adventurer back from death. The
    /// holy and life-attuned callings (Cleric, Paladin, Druid) command the
    /// Resurrection rite; everyone else must find one who does.
    pub fn can_resurrect(self) -> bool {
        matches!(self, Self::Cleric | Self::Paladin | Self::Druid)
    }

    pub fn as_key(self) -> &'static str {
        match self {
            Self::Warrior => "warrior",
            Self::Mage => "mage",
            Self::Cleric => "cleric",
            Self::Rogue => "rogue",
            Self::Ranger => "ranger",
            Self::Druid => "druid",
            Self::Necromancer => "necromancer",
            Self::Bard => "bard",
            Self::Monk => "monk",
            Self::Paladin => "paladin",
            Self::Warlock => "warlock",
            Self::Berserker => "berserker",
            Self::Beastlord => "beastlord",
            Self::Skald => "skald",
            Self::Runemaster => "runemaster",
            Self::Valewalker => "valewalker",
            Self::Spiritmaster => "spiritmaster",
        }
    }

    pub fn from_key(key: &str) -> Option<Class> {
        match key {
            "warrior" => Some(Self::Warrior),
            "mage" => Some(Self::Mage),
            "cleric" => Some(Self::Cleric),
            "rogue" => Some(Self::Rogue),
            "ranger" => Some(Self::Ranger),
            "druid" => Some(Self::Druid),
            "necromancer" => Some(Self::Necromancer),
            "bard" => Some(Self::Bard),
            "monk" => Some(Self::Monk),
            "paladin" => Some(Self::Paladin),
            "warlock" => Some(Self::Warlock),
            "berserker" => Some(Self::Berserker),
            "beastlord" => Some(Self::Beastlord),
            "skald" => Some(Self::Skald),
            "runemaster" => Some(Self::Runemaster),
            "valewalker" => Some(Self::Valewalker),
            "spiritmaster" => Some(Self::Spiritmaster),
            _ => None,
        }
    }
}

/// Total experience required to reach a given level. Smoothly rising curve so
/// early levels arrive quickly, then the climb past the first story bosses
/// stretches into a longer campaign.
pub fn xp_for_level(level: i32) -> i64 {
    if level <= 1 {
        return 0;
    }
    let l = level as i64;
    let d = l - 1;
    let base = 25 * d * d + (15 * d * d * d) / 10;
    if level <= 8 {
        base
    } else {
        let late = d - 7;
        base + 220 * late * late + 8 * late * late * late
    }
}

/// The level a given total xp corresponds to (1..=MAX_LEVEL).
pub fn level_for_xp(xp: i64) -> i32 {
    let mut level = 1;
    while level < Class::MAX_LEVEL && xp >= xp_for_level(level + 1) {
        level += 1;
    }
    level
}

/// A named milestone reached every five levels - a standout moment on top of the
/// steady per-level stat growth. Returns the title at levels 5, 10, ... 50.
pub fn level_milestone(level: i32) -> Option<&'static str> {
    if !(5..=Class::MAX_LEVEL).contains(&level) || level % 5 != 0 {
        return None;
    }
    Some(match level {
        5 => "Blooded",
        10 => "Toughened",
        15 => "Seasoned",
        20 => "Veteran",
        25 => "Hardened",
        30 => "Grizzled",
        35 => "Indomitable",
        40 => "Renowned",
        45 => "Mythic",
        _ => "Ascended",
    })
}

/// Permanent bonus max HP from the milestones reached so far. A pure function of
/// level (which is already persisted), so it needs no extra save state: +5 HP at
/// each five-level milestone.
pub fn milestone_hp_bonus(level: i32) -> i32 {
    (level.clamp(0, Class::MAX_LEVEL) / 5) * 5
}

/// The most recent milestone title at or below `level` (for the character sheet).
pub fn current_milestone(level: i32) -> Option<&'static str> {
    level_milestone((level.clamp(0, Class::MAX_LEVEL) / 5) * 5)
}

// ---- Archetypes -----------------------------------------------------------
//
// At ARCHETYPE_LEVEL each class commits to one of two archetype paths. Each path
// declares a cross-class Role (Tank / Healer / DPS) and a small set of permanent
// modifiers applied at the existing combat hook points (no engine changes). Data
// lives in one table to keep the per-archetype boilerplate down.

/// The level at which a character chooses their archetype.
pub const ARCHETYPE_LEVEL: i32 = 10;

/// The cross-class role an archetype leans into.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Tank,
    Healer,
    Dps,
}

impl Role {
    pub fn label(self) -> &'static str {
        match self {
            Self::Tank => "Tank",
            Self::Healer => "Healer",
            Self::Dps => "DPS",
        }
    }
}

/// One archetype path: a permanent specialisation chosen at `ARCHETYPE_LEVEL`.
/// Modifiers are percentages applied at the combat hooks (see `svc.rs`).
#[derive(Clone, Copy, Debug)]
pub struct ArchetypeDef {
    /// Stable persistence key (never reorder/rename).
    pub key: &'static str,
    pub class: Class,
    pub name: &'static str,
    pub role: Role,
    pub desc: &'static str,
    /// Bonus to outgoing damage (auto-attacks and ability strikes), percent.
    pub attack_pct: i32,
    /// Reduction to incoming damage, percent.
    pub mitigation_pct: i32,
    /// Bonus to healing you receive, percent.
    pub heal_pct: i32,
    /// Bonus to max health, percent of the base pool.
    pub max_hp_pct: i32,
}

/// A Tank path: hardier and harder to kill.
const fn tank(
    key: &'static str,
    class: Class,
    name: &'static str,
    desc: &'static str,
) -> ArchetypeDef {
    ArchetypeDef {
        key,
        class,
        name,
        role: Role::Tank,
        desc,
        attack_pct: 0,
        mitigation_pct: 22,
        heal_pct: 0,
        max_hp_pct: 12,
    }
}
/// A Healer path: mending is far more potent.
const fn healer(
    key: &'static str,
    class: Class,
    name: &'static str,
    desc: &'static str,
) -> ArchetypeDef {
    ArchetypeDef {
        key,
        class,
        name,
        role: Role::Healer,
        desc,
        attack_pct: 0,
        mitigation_pct: 0,
        heal_pct: 35,
        max_hp_pct: 4,
    }
}
/// A DPS path: strikes land appreciably harder.
const fn dps(
    key: &'static str,
    class: Class,
    name: &'static str,
    desc: &'static str,
) -> ArchetypeDef {
    ArchetypeDef {
        key,
        class,
        name,
        role: Role::Dps,
        desc,
        attack_pct: 18,
        mitigation_pct: 0,
        heal_pct: 0,
        max_hp_pct: 0,
    }
}

/// Two archetypes per class. Order matters only for the `1`/`2` quick-pick.
pub const ARCHETYPES: &[ArchetypeDef] = &[
    dps(
        "warlord",
        Class::Warrior,
        "Warlord",
        "Trade the shield for the offensive - every blow lands harder.",
    ),
    tank(
        "juggernaut",
        Class::Warrior,
        "Juggernaut",
        "An immovable wall of iron that shrugs off what would fell others.",
    ),
    dps(
        "pyromancer",
        Class::Mage,
        "Pyromancer",
        "All-in on raw destruction - your spells burn fiercer.",
    ),
    tank(
        "frostweaver",
        Class::Mage,
        "Frostweaver",
        "Wards of ice blunt the blows that get through your magic.",
    ),
    dps(
        "templar",
        Class::Cleric,
        "Templar",
        "Carry the smiting to the foe; holy fire answers harder.",
    ),
    healer(
        "oracle",
        Class::Cleric,
        "Oracle",
        "The Dawn flows through you - your mending is greatly amplified.",
    ),
    dps(
        "assassin",
        Class::Rogue,
        "Assassin",
        "Pure lethality: every strike cuts for far more.",
    ),
    tank(
        "outlaw",
        Class::Rogue,
        "Outlaw",
        "A scrapper's grit - tougher and harder to put down.",
    ),
    dps(
        "marksman",
        Class::Ranger,
        "Marksman",
        "Every shot is a killing shot, with force to match.",
    ),
    tank(
        "warden",
        Class::Ranger,
        "Warden",
        "The survivalist's craft keeps you standing where others fall.",
    ),
    tank(
        "guardian",
        Class::Druid,
        "Guardian",
        "Take the shape of bark and stone and become a bulwark of the wild.",
    ),
    healer(
        "restoration",
        Class::Druid,
        "Restoration",
        "Channel the green tide - your healing blooms far stronger.",
    ),
    dps(
        "reaper",
        Class::Necromancer,
        "Reaper",
        "Bend all the dark to slaughter; your shadows bite deeper.",
    ),
    healer(
        "defiler",
        Class::Necromancer,
        "Defiler",
        "Wring more life from the world to mend your cold frame.",
    ),
    dps(
        "skald",
        Class::Bard,
        "Skald",
        "A war-song that sharpens your every strike.",
    ),
    healer(
        "minstrel",
        Class::Bard,
        "Minstrel",
        "A song of mending whose every refrain heals the harder.",
    ),
    dps(
        "windwalker",
        Class::Monk,
        "Windwalker",
        "Pure flowing offense - your flurries strike harder.",
    ),
    tank(
        "stoneform",
        Class::Monk,
        "Stoneform",
        "Still the body to stone and let the blows simply fail.",
    ),
    dps(
        "crusader",
        Class::Paladin,
        "Crusader",
        "Take the oath on the attack; your holy blows fall heavier.",
    ),
    tank(
        "protector",
        Class::Paladin,
        "Protector",
        "The wall of the line - blessed steel turns the worst aside.",
    ),
    dps(
        "destroyer",
        Class::Warlock,
        "Destroyer",
        "Spend the pact freely; your curses and bolts bite far deeper.",
    ),
    healer(
        "soulbinder",
        Class::Warlock,
        "Soulbinder",
        "Bind stolen life to yourself - your draining mends much more.",
    ),
    dps(
        "ravager",
        Class::Berserker,
        "Ravager",
        "Nothing but the attack - your reckless blows hit even harder.",
    ),
    tank(
        "warbringer",
        Class::Berserker,
        "Warbringer",
        "Rage made armor; weather the storm and keep on swinging.",
    ),
    dps(
        "packmaster",
        Class::Beastlord,
        "Packmaster",
        "Drive the hunt on the attack - you and your beast strike far harder.",
    ),
    tank(
        "warden_of_beasts",
        Class::Beastlord,
        "Wildwarden",
        "Stand as the shield your companion fights behind, hard to bring down.",
    ),
    dps(
        "berserker_skald",
        Class::Skald,
        "Warsinger",
        "A war-song turned to slaughter - your every blow bites deeper.",
    ),
    healer(
        "loresinger",
        Class::Skald,
        "Loresinger",
        "A saga of mending whose every verse knits your wounds the harder.",
    ),
    dps(
        "warcarver",
        Class::Runemaster,
        "Warcarver",
        "Carve runes of pure ruin - your graven force blazes fiercer.",
    ),
    tank(
        "wardcarver",
        Class::Runemaster,
        "Wardcarver",
        "Score runes of shelter into your skin to blunt the blows that land.",
    ),
    dps(
        "reaper_of_the_vale",
        Class::Valewalker,
        "Vale-Reaper",
        "Give the scythe its head; your reaping strokes fall heavier.",
    ),
    tank(
        "greenwarden",
        Class::Valewalker,
        "Greenwarden",
        "Root deep as an old oak and let the fight break against you.",
    ),
    dps(
        "spiritreaver",
        Class::Spiritmaster,
        "Spiritreaver",
        "Loose the bound dead without mercy - your spirits bite far deeper.",
    ),
    healer(
        "soulwarden",
        Class::Spiritmaster,
        "Soulwarden",
        "Turn stolen spirit inward - your siphoning mends you far more.",
    ),
];

/// The two archetype choices for a class, in quick-pick order.
pub fn archetypes_for(class: Class) -> Vec<&'static ArchetypeDef> {
    ARCHETYPES.iter().filter(|a| a.class == class).collect()
}

/// Look up an archetype by its stable persistence key.
pub fn archetype_by_key(key: &str) -> Option<&'static ArchetypeDef> {
    ARCHETYPES.iter().find(|a| a.key == key)
}

#[cfg(test)]
#[path = "classes_test.rs"]
mod classes_test;
