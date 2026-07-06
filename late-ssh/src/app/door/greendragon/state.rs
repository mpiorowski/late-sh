//! Per-session Green Dragon state: the authoritative character (this is a
//! single-player game, so the session owns the truth), a small mode machine for
//! which screen is open, the active combat encounter, and a short message log.
//!
//! All game actions live here as methods that mutate the character and push log
//! lines; `input.rs` maps keys to these and `ui.rs` renders the getters. Every
//! mutating action persists the character through the service, fire-and-forget.

use std::collections::VecDeque;

use rand::Rng;
use uuid::Uuid;

use super::combat::{Buff, Combatant, resolve_extra_foe_strike, resolve_round_buffed};
use super::data;
use super::events::{self, ForestEvent};
use super::inn;
use super::model::{self, Character, DragonPointKind, ForestHunt, Race, SlainFoe, Specialty};
use super::specialty::{self, SkillEffect};
use super::svc::{CharacterLoad, FiveSixLoad, GreenDragonService, NewsLoad};
use super::tavern;

/// Which Green Dragon screen the session is looking at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Still waiting for the character to load from the DB.
    Loading,
    /// The village square: the main menu of destinations.
    Village,
    /// The forest: choose a hunting intensity.
    Forest,
    /// An active fight (creature, master, or the dragon).
    Fight,
    /// Ironroost Weapons.
    WeaponShop,
    /// Duskmail Armoury.
    ArmorShop,
    /// The Mendery (healer).
    Healer,
    /// The Coinvault (bank).
    Bank,
    /// The Proving Yard (the master fight gate).
    Training,
    /// A forest special event awaiting the player's accept/decline choice.
    Event,
    /// The one-time address-style chooser: picks the DK-title column, the
    /// romance partner, and one bard outcome (upstream reads `sex` for all
    /// three; ours is a flavor choice). Armed on load while unchosen, between
    /// the dragon-point and race gates.
    ChooseStyle,
    /// The forced one-time ancestry chooser (LoGD's race gate): armed on load
    /// while the race is unset, after any pending dragon points are spent
    /// (upstream `newday.php` gates dragon points, then race, then specialty).
    ChooseRace,
    /// The one-time specialty chooser (Mystical / Dark Arts / Thief).
    ChooseSpecialty,
    /// The graveyard: the dead realm's hub, replacing the village until the
    /// player revives (torment fights, the mausoleum, resurrection).
    Graveyard,
    /// The forced dragon-point spend gate: play is blocked while points from a
    /// dragon kill sit unallocated (LoGD's new-day gate).
    SpendDragonPoints,
    /// The village's daily news, paged one day at a time (`news.php`).
    News,
    /// The stables: buy, trade in, or sell a mount (`stables.php`).
    Stables,
    /// The mercenary camp: hire a companion or patch up the wounded ones
    /// (`mercenarycamp.php`).
    MercCamp,
    /// The Sleeping Stag's common room: the inn hub (`inn.php`).
    Inn,
    /// Taking a room for the night: the purse or the bank (`inn_room.php`).
    InnRoom,
    /// The barkeep's counter: bribes for a quiet word (`inn_bartender.php`).
    Barkeep,
    /// The prize of a successful bribe: switching the specialty path.
    SwitchSpecialty,
    /// The barkeep's back shelf of potions (`cedrikspotions.php`).
    Potions,
    /// The taps (`modules/drinks.php`).
    Drinks,
    /// The corner table with the romance partner (`modules/lovers.php`).
    Romance,
    /// The forest outhouse's two stalls (`modules/outhouse.php`).
    Outhouse,
    /// After the stall: wash up or slip out. `true` = the paid private stall.
    OuthouseWash(bool),
    /// The Dark Horse Tavern, stumbled on in the forest (`darkhorse.php`);
    /// its sub-views (the games) live in [`TavernView`].
    Tavern,
}

/// Which corner of the Dark Horse the session is in (all under
/// [`Mode::Tavern`]): the taproom, or one of the gambler's games mid-hand.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TavernView {
    Hub,
    /// Staking the dice game.
    DiceBet,
    /// A dice hand in progress.
    Dice(tavern::DiceGame),
    /// Calling like or unlike pairs for stones.
    StonesSide,
    /// Staking the stones game.
    StonesBet { like_pair: bool },
    /// A stones game in progress.
    Stones(tavern::StonesGame),
}

/// What kind of foe the current encounter is, deciding the victory handler.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FoeKind {
    Creature,
    Master,
    Dragon,
    /// A graveyard torment fight, fought dead on the soulpoint pool; its
    /// "reward" is favor with the death overlord.
    Torment,
}

/// One foe in a live encounter. Master and dragon fights hold exactly one;
/// forest multi-fights (unlocked at 10 dragon kills) can hold up to a pack.
#[derive(Clone, Debug)]
pub struct Foe {
    pub name: String,
    pub weapon: String,
    pub combatant: Combatant,
    pub hp: u32,
    pub max_hp: u32,
    pub reward_gold: u32,
    pub reward_exp: u32,
    pub level: u8,
}

/// A live combat encounter: the player strikes the first living foe each
/// round; every living foe strikes back.
#[derive(Clone, Debug)]
pub struct Encounter {
    pub foes: Vec<Foe>,
    pub kind: FoeKind,
    /// Active specialty buffs, ticked each round by [`resolve_round_buffed`].
    pub buffs: Vec<Buff>,
    /// Whether the player has taken any damage this fight (drives flawless
    /// bonuses: the dragon's extra loot, the forest's turn refund).
    pub took_damage: bool,
    /// Foes already slain this fight, banked for the victory settlement.
    pub slain: Vec<SlainFoe>,
}

impl Encounter {
    /// A single-foe encounter (masters, the dragon, ordinary forest fights).
    fn single(foe: Foe, kind: FoeKind) -> Self {
        Encounter {
            foes: vec![foe],
            kind,
            buffs: Vec::new(),
            took_damage: false,
            slain: Vec::new(),
        }
    }

    /// Index of the player's current target: the first living foe.
    pub fn target(&self) -> Option<usize> {
        self.foes.iter().position(|f| f.hp > 0)
    }

    /// Living foes remaining.
    pub fn living(&self) -> usize {
        self.foes.iter().filter(|f| f.hp > 0).count()
    }
}

const LOG_CAP: usize = 7;

pub struct State {
    user_id: Uuid,
    svc: GreenDragonService,
    load_rx: tokio::sync::watch::Receiver<CharacterLoad>,
    character: Option<Character>,
    mode: Mode,
    cursor: usize,
    log: VecDeque<String>,
    encounter: Option<Encounter>,
    /// The forest event awaiting an accept/decline choice, while in [`Mode::Event`].
    pending_event: Option<ForestEvent>,
    /// Days back the news view is showing (0 = today).
    news_offset: i64,
    /// The in-flight news page load, drained by [`State::tick`].
    news_rx: Option<tokio::sync::watch::Receiver<NewsLoad>>,
    /// The loaded news page for `news_offset`, newest first.
    news_lines: Option<std::sync::Arc<Vec<String>>>,
    /// Which corner of the Dark Horse is open while in [`Mode::Tavern`].
    tavern_view: TavernView,
    /// The Five Sixes pot as last read (for the signboard), if known.
    fivesix_pot: Option<u64>,
    /// The in-flight pot read kicked off on entering the tavern.
    fivesix_pot_rx: Option<tokio::sync::watch::Receiver<Option<u64>>>,
    /// An in-flight Five Sixes settlement: the sixes rolled, and the pot
    /// round-trip. Drained by [`State::tick`].
    fivesix_rx: Option<(u32, tokio::sync::watch::Receiver<FiveSixLoad>)>,
}

impl State {
    /// Open a Green Dragon session for `user_id`, kicking off the character
    /// load. `name` is the player's display name, used only if they have no
    /// save yet.
    pub fn new(svc: GreenDragonService, user_id: Uuid, name: String) -> Self {
        let load_rx = svc.load_character(user_id, name);
        State {
            user_id,
            svc,
            load_rx,
            character: None,
            mode: Mode::Loading,
            cursor: 0,
            log: VecDeque::new(),
            encounter: None,
            pending_event: None,
            news_offset: 0,
            news_rx: None,
            news_lines: None,
            tavern_view: TavernView::Hub,
            fivesix_pot: None,
            fivesix_pot_rx: None,
            fivesix_rx: None,
        }
    }

    /// Drain pending async loads (the initial character, a news page). Called
    /// every app tick.
    pub fn tick(&mut self) {
        self.tick_news();
        self.tick_tavern();
        if self.character.is_some() {
            return;
        }
        // Clone the loaded character out and drop the watch borrow before
        // touching `self` again.
        let ready = match &*self.load_rx.borrow_and_update() {
            CharacterLoad::Ready(character) => Some((**character).clone()),
            CharacterLoad::Loading => None,
        };
        if let Some(mut character) = ready {
            // A never-titled save (fresh characters, pre-title saves) gets its
            // rank off the ladder before anything renders.
            if character.title.is_empty() {
                character.reroll_title(&mut rand::thread_rng());
            }
            // The new-day gates, in upstream's order (`newday.php`): unspent
            // dragon points first, then the one-time style and race choices.
            self.mode = if character.dragon_points_unspent > 0 {
                Mode::SpendDragonPoints
            } else if character.style == model::AddressStyle::Unchosen {
                Mode::ChooseStyle
            } else if character.race == Race::None {
                Mode::ChooseRace
            } else if character.alive {
                Mode::Village
            } else {
                Mode::Graveyard
            };
            self.push_log(format!(
                "Welcome to Duskmere, {}. The Green Dragon awaits the brave.",
                character.name
            ));
            self.character = Some(character);
            self.cursor = 0;
        }
    }

    // --- getters for the UI -------------------------------------------------

    pub fn mode(&self) -> Mode {
        self.mode
    }

    pub fn character(&self) -> Option<&Character> {
        self.character.as_ref()
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn encounter(&self) -> Option<&Encounter> {
        self.encounter.as_ref()
    }

    /// The forest event currently awaiting a choice, if any (for rendering its
    /// framing text in [`Mode::Event`]).
    pub fn pending_event(&self) -> Option<ForestEvent> {
        self.pending_event
    }

    /// The news page being viewed: `(days back, lines)`. `None` lines mean the
    /// page is still loading.
    pub fn news_page(&self) -> (i64, Option<&[String]>) {
        (
            self.news_offset,
            self.news_lines.as_ref().map(|l| l.as_slice()),
        )
    }

    pub fn log_lines(&self) -> impl Iterator<Item = &str> {
        self.log.iter().map(String::as_str)
    }

    /// The selectable rows for the current mode, as `(label, enabled)`.
    pub fn menu(&self) -> Vec<(String, bool)> {
        let Some(c) = self.character.as_ref() else {
            return Vec::new();
        };
        match self.mode {
            Mode::Village => village_menu(c),
            Mode::Forest => forest_menu(c),
            Mode::WeaponShop => shop_menu(c, true),
            Mode::ArmorShop => shop_menu(c, false),
            Mode::Healer => healer_menu(c),
            Mode::Bank => bank_menu(c),
            Mode::Training => training_menu(c),
            Mode::Fight => fight_menu(c),
            Mode::Event => event_menu(c, self.pending_event),
            Mode::ChooseStyle => style_menu(),
            Mode::ChooseRace => race_menu(),
            Mode::ChooseSpecialty => specialty_menu(),
            Mode::Graveyard => graveyard_menu(c),
            Mode::SpendDragonPoints => dragon_point_menu(),
            Mode::News => news_menu(self.news_offset),
            Mode::Stables => stables_menu(c),
            Mode::MercCamp => merc_camp_menu(c),
            Mode::Inn => inn_menu(c),
            Mode::InnRoom => inn_room_menu(c),
            Mode::Barkeep => barkeep_menu(c),
            Mode::SwitchSpecialty => switch_specialty_menu(c),
            Mode::Potions => potions_menu(c),
            Mode::Drinks => drinks_menu(c),
            Mode::Romance => romance_menu(c),
            Mode::Outhouse => outhouse_menu(c),
            Mode::OuthouseWash(_) => outhouse_wash_menu(),
            Mode::Tavern => tavern_menu(c, self.tavern_view, self.fivesix_pot, self.fivesix_rx.is_some()),
            Mode::Loading => Vec::new(),
        }
    }

    // --- cursor + selection -------------------------------------------------

    pub fn move_cursor(&mut self, delta: i32) {
        let len = self.menu().len();
        if len == 0 {
            return;
        }
        let cur = self.cursor as i32;
        self.cursor = (cur + delta).rem_euclid(len as i32) as usize;
    }

    /// Activate the highlighted row. Returns false only when the row is the
    /// "leave the game" sentinel handled by the caller.
    pub fn select(&mut self) -> Selection {
        let menu = self.menu();
        if self.cursor >= menu.len() {
            return Selection::Stay;
        }
        if !menu[self.cursor].1 {
            self.push_log("You can't do that yet.".into());
            return Selection::Stay;
        }
        match self.mode {
            Mode::Village => self.select_village(),
            Mode::Forest => self.select_forest(),
            Mode::WeaponShop => self.buy_gear(true),
            Mode::ArmorShop => self.buy_gear(false),
            Mode::Healer => self.select_healer(),
            Mode::Bank => self.select_bank(),
            Mode::Training => self.select_training(),
            Mode::Fight => self.select_fight(),
            Mode::Event => self.select_event(),
            Mode::ChooseStyle => self.select_style(),
            Mode::ChooseRace => self.select_race(),
            Mode::ChooseSpecialty => self.select_specialty(),
            Mode::Graveyard => self.select_graveyard(),
            Mode::SpendDragonPoints => self.select_dragon_point(),
            Mode::News => self.select_news(),
            Mode::Stables => self.select_stables(),
            Mode::MercCamp => self.select_merc_camp(),
            Mode::Inn => self.select_inn(),
            Mode::InnRoom => self.select_inn_room(),
            Mode::Barkeep => self.select_barkeep(),
            Mode::SwitchSpecialty => self.select_switch_specialty(),
            Mode::Potions => self.select_potions(),
            Mode::Drinks => self.select_drinks(),
            Mode::Romance => self.select_romance(),
            Mode::Outhouse => self.select_outhouse(),
            Mode::OuthouseWash(paid) => self.select_outhouse_wash(paid),
            Mode::Tavern => self.select_tavern(),
            Mode::Loading => Selection::Stay,
        }
    }

    /// Back out one level: leaf screens return to the village; the village
    /// leaves the game.
    pub fn back(&mut self) -> Selection {
        match self.mode {
            Mode::Village | Mode::Loading => Selection::Leave,
            // Esc during a fight attempts to flee (a 1-in-3 roll, like the
            // Flee row). Leaving mid-fight is never free.
            Mode::Fight => {
                self.attempt_flee();
                Selection::Stay
            }
            Mode::Event => {
                // Esc on an event declines it (the no-choice branch), then
                // returns to the forest.
                self.cursor = 1;
                self.select_event()
            }
            // The gates can't be backed out of into play — but leaving the
            // door entirely is fine; they re-arm on re-entry.
            Mode::SpendDragonPoints | Mode::ChooseStyle | Mode::ChooseRace => Selection::Leave,
            // The dead realm is the hub while dead: Esc leaves the game, the
            // village stays out of reach until a revival.
            Mode::Graveyard => Selection::Leave,
            // The inn's side rooms back out to the common room first.
            Mode::InnRoom
            | Mode::Barkeep
            | Mode::SwitchSpecialty
            | Mode::Potions
            | Mode::Drinks
            | Mode::Romance => {
                self.goto(Mode::Inn);
                Selection::Stay
            }
            // The forest amenities back out to the forest. Slipping out of
            // the stall unwashed is a real (and newsworthy) choice, so Esc
            // takes the explicit no-wash exit.
            Mode::Outhouse => {
                self.goto(Mode::Forest);
                Selection::Stay
            }
            Mode::OuthouseWash(paid) => {
                self.cursor = 1;
                self.select_outhouse_wash(paid)
            }
            // A game in progress folds (the stake was never taken); the
            // taproom itself lets out into the forest.
            Mode::Tavern => {
                if self.tavern_view == TavernView::Hub {
                    self.goto(Mode::Forest);
                } else {
                    self.tavern_view = TavernView::Hub;
                    self.cursor = 0;
                }
                Selection::Stay
            }
            _ => {
                self.goto(Mode::Village);
                Selection::Stay
            }
        }
    }

    fn goto(&mut self, mode: Mode) {
        self.mode = mode;
        self.cursor = 0;
    }

    // --- village ------------------------------------------------------------

    fn select_village(&mut self) -> Selection {
        let c = self.character.as_ref().unwrap();
        let rows = village_menu(c);
        match rows[self.cursor].0.as_str() {
            s if s.starts_with("The Forest") => self.goto(Mode::Forest),
            s if s.starts_with("Choose a Specialty") => self.goto(Mode::ChooseSpecialty),
            s if s.starts_with("The Proving Yard") => self.goto(Mode::Training),
            s if s.starts_with("Seek Out the Green Dragon") => self.start_dragon(),
            s if s.starts_with("Ironroost") => self.goto(Mode::WeaponShop),
            s if s.starts_with("Duskmail") => self.goto(Mode::ArmorShop),
            s if s.starts_with("The Mendery") => {
                // Over-healed visitors are clipped back to max, free of charge
                // (healer.php's forced over-max branch).
                if self.character.as_mut().unwrap().normalize_overheal() {
                    self.push_log(
                        "The healer eyes your unnatural vigor and drains it off, no charge.".into(),
                    );
                    self.save();
                }
                self.goto(Mode::Healer)
            }
            s if s.starts_with("The Coinvault") => self.goto(Mode::Bank),
            s if s.starts_with("The Stables") => self.goto(Mode::Stables),
            s if s.starts_with("The Mercenary Camp") => self.goto(Mode::MercCamp),
            s if s.starts_with(data::INN_NAME) => self.goto(Mode::Inn),
            s if s.starts_with("The Daily News") => self.open_news(0),
            s if s.starts_with("Leave") => return Selection::Leave,
            _ => {}
        }
        Selection::Stay
    }

    // --- forest -------------------------------------------------------------

    fn select_forest(&mut self) -> Selection {
        let hunt = match self.cursor {
            0 => ForestHunt::Slumming,
            1 => ForestHunt::Hunt,
            2 => ForestHunt::Thrillseeking,
            3 => {
                self.goto(Mode::Outhouse);
                return Selection::Stay;
            }
            _ => return Selection::Stay,
        };
        self.start_forest_fight(hunt);
        Selection::Stay
    }

    fn start_forest_fight(&mut self, hunt: ForestHunt) {
        let c = self.character.as_mut().unwrap();
        if c.turns == 0 {
            self.push_log("You are too tired to fight. Come back tomorrow.".into());
            return;
        }
        // Facing death sobers you up a little: every search shaves 10% off
        // the drunkenness (the `soberup` hook `forest.php` fires).
        if c.drunkenness > 0 {
            c.sober_up();
        }
        // A fraction of searches turn up a special event instead of a fight. The
        // event itself doesn't spend the forest turn (some, like the mine, spend
        // it as their own effect), so roll before decrementing.
        let mut rng = rand::thread_rng();
        if rng.gen_range(0..100) < events::FOREST_EVENT_PERCENT {
            let event = events::roll(&mut rng);
            self.start_event(event);
            return;
        }
        c.turns -= 1;
        let player_level = c.level as i32;

        // The base level jitter (`forest.php`): a third of searches roll a
        // nudge, +1 with odds 1/5 and -1 with odds 1/3; slumming shifts down
        // one, thrillseeking up one.
        let (mut plev, mut nlev) = (0i32, 0i32);
        if rng.gen_range(0..=2) == 1 {
            plev = i32::from(rng.gen_range(1..=5) == 1);
            nlev = i32::from(rng.gen_range(1..=3) == 1);
        }
        match hunt {
            ForestHunt::Slumming => nlev += 1,
            ForestHunt::Thrillseeking => plev += 1,
            ForestHunt::Hunt => {}
        }
        let mut target = player_level + plev - nlev;
        let mut min_target = target;

        // Multi-fights unlock at 10 dragon kills: a quarter of searches spawn
        // 2-3 foes, slumming shaving the count and level floor, thrillseeking
        // raising both.
        let mut multi = 1i32;
        if c.dragon_kills >= 10 && rng.gen_range(1..=100) <= 25 {
            multi = rng.gen_range(2..=3);
            match hunt {
                ForestHunt::Slumming => {
                    multi -= rng.gen_range(0..=1);
                    min_target = target - if rng.gen_range(0..=1) == 1 { 1 } else { 2 };
                }
                ForestHunt::Thrillseeking => {
                    multi += rng.gen_range(1..=2);
                    if rng.gen_range(0..=1) == 1 {
                        target += 1;
                    }
                    min_target = target - 1;
                }
                ForestHunt::Hunt => {}
            }
            multi = multi.min(player_level);
        }
        let mut multi = multi.max(1);
        target = target.max(1);
        min_target = min_target.clamp(1, target);
        // Overflow past the table's cap converts to extra foes (upstream caps
        // at its level-17 rows; our table ends at 16 — see PARITY.md).
        if target > 16 {
            multi += target - 16;
            target = 16;
        }

        // A pack (1-in-6 when multi) clones one creature: the stat block and
        // name are drawn once from the level range, while each clone's nominal
        // level is rolled separately (it feeds the exp-bonus and flawless
        // math). Otherwise each foe is an independent creature in the range.
        let pack = multi > 1 && rng.gen_range(0..=5) == 0;
        let pack_level = rng.gen_range(min_target..=target) as u8;
        let pack_name = {
            let names = data::CREATURE_NAMES[(pack_level - 1) as usize];
            names[rng.gen_range(0..names.len())]
        };
        let mut foes = Vec::with_capacity(multi as usize);
        for _ in 0..multi {
            let level = if multi > 1 {
                rng.gen_range(min_target..=target) as u8
            } else {
                target as u8
            };
            let (name, weapon, stat_level) = if pack {
                (pack_name.0, pack_name.1, pack_level)
            } else {
                let names = data::CREATURE_NAMES[(level - 1) as usize];
                let (n, w) = names[rng.gen_range(0..names.len())];
                (n, w, level)
            };
            // Investment scaling + flux (buffbadguy), then the Deepfolk gold
            // nose (upstream's creatureencounter hook fires inside buffbadguy,
            // before the thrill bonus), then the thrill bonus.
            let mut tier = c.buff_foe(data::creature_tier(stat_level), &mut rng);
            tier.gold = c.race.creature_gold(tier.gold);
            if matches!(hunt, ForestHunt::Thrillseeking) {
                tier.gold = (tier.gold as f64 * 1.10).round() as u32;
                tier.exp = (tier.exp as f64 * 1.10).round() as u32;
            }
            foes.push(Foe {
                name: name.to_string(),
                weapon: weapon.to_string(),
                combatant: Combatant {
                    attack: tier.attack,
                    defense: tier.defense,
                },
                hp: tier.hp,
                max_hp: tier.hp,
                reward_gold: tier.gold,
                reward_exp: tier.exp,
                level,
            });
        }
        if foes.len() > 1 {
            self.push_log(format!(
                "A band of {} foes closes in, led by {}!",
                foes.len(),
                foes[0].name
            ));
        } else {
            let (name, weapon) = (&foes[0].name, &foes[0].weapon);
            self.push_log(format!("You encounter {name} wielding {weapon}!"));
        }
        self.encounter = Some(Encounter {
            foes,
            kind: FoeKind::Creature,
            buffs: Vec::new(),
            took_damage: false,
            slain: Vec::new(),
        });
        self.inject_persistent_buffs();
        self.goto(Mode::Fight);
        // Persist the spent forest turn now, so a disconnect mid-fight can't
        // refund it on reconnect.
        self.save();
    }

    /// Carry the character's persistent buffs (drinks, the lover's ward,
    /// sickness) and any mounted rounds into the fight that just opened. The
    /// encounter ticks them like any buff; [`State::writeback_buffs`] banks
    /// what's left when it ends. The dead carry nothing (upstream strips
    /// buffs at the graveyard).
    fn inject_persistent_buffs(&mut self) {
        let Some(enc) = self.encounter.as_mut() else {
            return;
        };
        if enc.kind == FoeKind::Torment {
            return;
        }
        let c = self.character.as_ref().unwrap();
        for pb in &c.persistent_buffs {
            enc.buffs.push(pb.as_buff());
        }
        if c.mount_rounds_left > 0
            && let Some(mount) = c.mount_data()
        {
            let mut buff = Buff::new(mount.name, c.mount_rounds_left);
            buff.player_atk_mod = data::MOUNT_ATK_MOD;
            buff.wearoff = format!("Your {} is winded and falls behind.", mount.name);
            enc.buffs.push(buff);
        }
    }

    /// Bank the leftover rounds of persistent buffs (and the mount) when a
    /// fight ends. A buff missing from the encounter expired mid-fight.
    fn writeback_buffs(&mut self, enc: &Encounter) {
        if enc.kind == FoeKind::Torment {
            return;
        }
        let c = self.character.as_mut().unwrap();
        c.persistent_buffs.retain_mut(|pb| {
            match enc.buffs.iter().find(|b| b.name == pb.name) {
                Some(b) if b.rounds_left > 0 => {
                    pb.rounds_left = b.rounds_left;
                    true
                }
                _ => false,
            }
        });
        if c.mount_rounds_left > 0
            && let Some(mount) = c.mount_data()
        {
            c.mount_rounds_left = enc
                .buffs
                .iter()
                .find(|b| b.name == mount.name)
                .map(|b| b.rounds_left)
                .unwrap_or(0);
        }
    }

    // --- forest special events ----------------------------------------------

    /// Begin a forest event: log its framing, then either resolve it instantly
    /// (no choice) or open [`Mode::Event`] to await the player's decision.
    fn start_event(&mut self, event: ForestEvent) {
        let c = self.character.as_ref().unwrap();
        let pres = event.present(c);
        if pres.choice.is_none() {
            // Instant event: narration and outcome both go to the log, then we
            // drop straight back to the forest.
            for line in &pres.intro {
                self.push_log((*line).to_string());
            }
            let mut rng = rand::thread_rng();
            let lines = event.resolve(true, self.character.as_mut().unwrap(), &mut rng);
            for line in lines {
                self.push_log(line);
            }
            self.after_event();
        } else {
            // Choice event: the framing is shown in the panel (Mode::Event), so
            // it isn't logged until the outcome lands.
            self.pending_event = Some(event);
            self.goto(Mode::Event);
        }
    }

    /// Resolve the pending event with the player's choice (cursor 0 = accept).
    fn select_event(&mut self) -> Selection {
        let Some(event) = self.pending_event.take() else {
            self.goto(Mode::Forest);
            return Selection::Stay;
        };
        let accepted = self.cursor == 0;
        // Stepping into the Dark Horse opens the real room (the games, the
        // pot) rather than an instant effect.
        if event == ForestEvent::Tavern && accepted {
            self.enter_tavern();
            return Selection::Stay;
        }
        let mut rng = rand::thread_rng();
        let lines = event.resolve(accepted, self.character.as_mut().unwrap(), &mut rng);
        for line in lines {
            self.push_log(line);
        }
        // Event deaths make the paper (`goldmine.php` / `glowingstream.php`
        // both addnews their kills; neither carries a taunt upstream).
        let c = self.character.as_ref().unwrap();
        if !c.alive {
            let who = c.titled_name();
            match event {
                ForestEvent::GoldMine => self.news(format!(
                    "{who} was buried alive digging too greedily in an abandoned mine."
                )),
                ForestEvent::GlowingStream => self.news(format!(
                    "{who} drank from a glowing stream deep in the forest and was never seen again."
                )),
                _ => {}
            }
        }
        self.after_event();
        Selection::Stay
    }

    /// Land somewhere sensible after an event: the graveyard if it killed you
    /// (the mine cave-in, the stream), otherwise back to the forest to hunt on.
    fn after_event(&mut self) {
        self.pending_event = None;
        let alive = self.character.as_ref().unwrap().alive;
        self.goto(if alive { Mode::Forest } else { Mode::Graveyard });
        self.save();
    }

    // --- the daily news -------------------------------------------------------

    /// Open the news page `days_back` days ago (0 = today), kicking off the
    /// async page load; [`State::tick`] lands it.
    fn open_news(&mut self, days_back: i64) {
        self.news_offset = days_back.max(0);
        self.news_lines = None;
        self.news_rx = Some(self.svc.load_news(self.news_offset));
        self.goto(Mode::News);
    }

    /// Drain a finished news page load into the view.
    fn tick_news(&mut self) {
        let Some(rx) = self.news_rx.as_mut() else {
            return;
        };
        let ready = match &*rx.borrow_and_update() {
            NewsLoad::Ready(lines) => Some(lines.clone()),
            NewsLoad::Loading => None,
        };
        if let Some(lines) = ready {
            self.news_lines = Some(lines);
            self.news_rx = None;
        }
    }

    /// Page the news view (older / newer / back to the village).
    fn select_news(&mut self) -> Selection {
        match self.cursor {
            0 => self.open_news(self.news_offset + 1),
            1 if self.news_offset > 0 => self.open_news(self.news_offset - 1),
            2 => self.goto(Mode::Village),
            _ => {}
        }
        Selection::Stay
    }

    /// Write a line to the village's daily news (LoGD `addnews`), attributed
    /// to this character.
    fn news(&self, body: String) {
        self.svc.publish_news(Some(self.user_id), body);
    }

    // --- style gate -----------------------------------------------------------

    /// Apply the one-time address-style choice, re-stamp the title off the
    /// chosen column, and fall through to the next gate (race, then play).
    fn select_style(&mut self) -> Selection {
        let style = match self.cursor {
            0 => model::AddressStyle::First,
            1 => model::AddressStyle::Second,
            _ => return Selection::Stay,
        };
        let c = self.character.as_mut().unwrap();
        c.style = style;
        c.reroll_title(&mut rand::thread_rng());
        let (title, race, alive) = (c.title.clone(), c.race, c.alive);
        self.push_log(format!(
            "So it is settled: the realm will know you as {title} and its like."
        ));
        self.save();
        self.goto(if race == Race::None {
            Mode::ChooseRace
        } else if alive {
            Mode::Village
        } else {
            Mode::Graveyard
        });
        Selection::Stay
    }

    // --- race gate ------------------------------------------------------------

    /// Apply the one-time ancestry choice (`lib/newday/setrace.php`) and drop
    /// into play: the village, or the graveyard if the gate caught a dead
    /// character at load.
    fn select_race(&mut self) -> Selection {
        let Some(&race) = model::RACES.get(self.cursor) else {
            return Selection::Stay;
        };
        let c = self.character.as_mut().unwrap();
        c.race = race;
        let alive = c.alive;
        self.push_log(format!(
            "You remember who you are: {} blood runs in your veins.",
            race.name()
        ));
        self.save();
        self.goto(if alive { Mode::Village } else { Mode::Graveyard });
        Selection::Stay
    }

    // --- specialty chooser --------------------------------------------------

    /// Apply the one-time specialty choice and return to the village.
    fn select_specialty(&mut self) -> Selection {
        let choice = match self.cursor {
            0 => Specialty::Mystical,
            1 => Specialty::DarkArts,
            2 => Specialty::Thief,
            _ => return Selection::Stay,
        };
        let c = self.character.as_mut().unwrap();
        c.choose_specialty(choice);
        self.push_log(format!("You devote yourself to the {}.", choice.name()));
        self.save();
        self.goto(Mode::Village);
        Selection::Stay
    }

    // --- the graveyard (the dead realm's hub) --------------------------------

    /// Activate the highlighted graveyard row: torment, the mausoleum, the
    /// paid resurrection, or waiting out the day (which leaves the door).
    fn select_graveyard(&mut self) -> Selection {
        match self.cursor {
            0 => self.start_torment_fight(),
            1 => {
                let c = self.character.as_mut().unwrap();
                match c.restore_soul() {
                    Some(cost) => {
                        let soul = c.soulpoints;
                        self.push_log(format!(
                            "{} scoffs at your frailty, takes {cost} favor, and knits your soul whole ({soul}).",
                            data::DEATH_OVERLORD
                        ));
                        self.save();
                    }
                    None => self.push_log(format!(
                        "{} turns away. Earn more favor before asking for restoration.",
                        data::DEATH_OVERLORD
                    )),
                }
            }
            2 => {
                // The paid resurrection is an extra new day: roll its bank
                // interest like any other dawn.
                let mut rng = rand::thread_rng();
                let interest =
                    rng.gen_range(model::MIN_INTEREST_PERCENT..=model::MAX_INTEREST_PERCENT);
                let c = self.character.as_mut().unwrap();
                if let Some(fx) = c.resurrect(interest, &mut rng) {
                    let (turns, who) = (c.turns, c.titled_name());
                    self.push_log(format!(
                        "Life burns back into your bones! You rise with {turns} turns left in the day."
                    ));
                    // Resurrections make the paper (`newday.php`'s addnews).
                    self.news(format!(
                        "{} has bartered {who} back from the dead.",
                        data::DEATH_OVERLORD
                    ));
                    // The newday module effects fire on this day too.
                    if fx.hangover {
                        self.push_log(
                            "You come back hungover, of all things. It costs you a turn.".into(),
                        );
                    }
                    if fx.divorced {
                        let (partner, who) = {
                            let c = self.character.as_ref().unwrap();
                            (data::partner(c.style), c.titled_name())
                        };
                        self.push_log(format!(
                            "{partner} has had enough of loving the briefly dead. The marriage is over."
                        ));
                        self.news(format!(
                            "{partner} has left {who} to pursue other interests."
                        ));
                    }
                    self.goto(Mode::Village);
                    self.save();
                } else {
                    self.push_log(format!(
                        "{} will not barter your life back for so little favor.",
                        data::DEATH_OVERLORD
                    ));
                }
            }
            3 => return Selection::Leave,
            _ => {}
        }
        Selection::Stay
    }

    /// Spend a grave fight to torment a lost soul (`case_battle_search.php`).
    /// While dead the soul pool *is* the HP pool: `hitpoints` holds the
    /// soulpoints for the fight's duration and is written back when it ends
    /// (victory, defeat, or a paid escape).
    fn start_torment_fight(&mut self) {
        let c = self.character.as_mut().unwrap();
        if c.grave_fights == 0 {
            self.push_log("The dead will suffer no more of you today.".into());
            return;
        }
        c.grave_fights -= 1;
        c.hitpoints = c.soulpoints;
        let mut rng = rand::thread_rng();
        let (name, weapon) =
            data::GRAVEYARD_CREATURES[rng.gen_range(0..data::GRAVEYARD_CREATURES.len())];
        let (attack, defense, hp) = data::graveyard_creature_stats(c.level);
        let (favor_lo, favor_hi) = data::graveyard_favor_range(c.level);
        let favor = rng.gen_range(favor_lo..=favor_hi);
        let level = c.level;
        self.encounter = Some(Encounter::single(
            Foe {
                name: name.to_string(),
                weapon: weapon.to_string(),
                combatant: Combatant { attack, defense },
                hp,
                max_hp: hp,
                reward_gold: 0,
                // The favor payout rides the exp slot, exactly like upstream
                // stuffs it into `creatureexp`.
                reward_exp: favor,
                level,
            },
            FoeKind::Torment,
        ));
        self.push_log(format!("You corner {name} among the graves!"));
        self.goto(Mode::Fight);
        // Persist the spent grave fight now, so a disconnect mid-fight can't
        // refund it on reconnect (same rationale as forest turns).
        self.save();
    }

    // --- training (master fight) -------------------------------------------

    fn select_training(&mut self) -> Selection {
        let c = self.character.as_ref().unwrap();
        if !c.can_challenge_master() {
            self.push_log("Your master shakes their head. Gain more experience first.".into());
            return Selection::Stay;
        }
        let Some((master, foe, hp)) = c.scaled_master(&mut rand::thread_rng()) else {
            return Selection::Stay;
        };
        self.encounter = Some(Encounter::single(
            Foe {
                name: master.name.to_string(),
                weapon: master.weapon.to_string(),
                combatant: foe,
                hp,
                max_hp: hp,
                reward_gold: 0,
                reward_exp: 0,
                level: c.level,
            },
            FoeKind::Master,
        ));
        self.inject_persistent_buffs();
        self.push_log(format!("{} steps forward to test you!", master.name));
        self.goto(Mode::Fight);
        Selection::Stay
    }

    // --- dragon -------------------------------------------------------------

    fn start_dragon(&mut self) {
        let c = self.character.as_mut().unwrap();
        if !c.can_seek_dragon() {
            self.push_log("You are not ready to face the Green Dragon.".into());
            return;
        }
        c.seen_dragon = true;
        let level = c.level;
        let (attack, defense, hp) = c.scaled_dragon(&mut rand::thread_rng());
        self.encounter = Some(Encounter::single(
            Foe {
                name: "The Green Dragon".to_string(),
                weapon: "Fearsome Claws and Flame".to_string(),
                combatant: Combatant { attack, defense },
                hp,
                max_hp: hp,
                reward_gold: 0,
                reward_exp: 0,
                level,
            },
            FoeKind::Dragon,
        ));
        self.inject_persistent_buffs();
        self.push_log("You step into the dragon's lair. The air turns to fire.".into());
        self.goto(Mode::Fight);
        // Persist `seen_dragon` now so the once-per-run dragon seek can't be
        // retried by disconnecting before the fight resolves.
        self.save();
    }

    // --- fight resolution ---------------------------------------------------

    fn fight_menu_action(&self) -> usize {
        self.cursor
    }

    /// The player's combat stats for this encounter: the usual gear-derived
    /// combatant, or the level-only dead stats with the soul pool's ceiling
    /// during graveyard torments.
    fn player_fight_stats(&self, kind: FoeKind) -> (Combatant, u32) {
        let c = self.character.as_ref().unwrap();
        match kind {
            FoeKind::Torment => (c.dead_combatant(), c.max_soulpoints()),
            _ => (c.combatant(), c.max_hitpoints()),
        }
    }

    fn select_fight(&mut self) -> Selection {
        let c = self.character.as_ref().unwrap();
        // The dead fight with bare essence: no specialty skills in the menu
        // (upstream's graveyard passes `fightnav(false, ...)`).
        let skill_count = if c.alive {
            specialty::skills(c.specialty).len()
        } else {
            0
        };
        let cursor = self.fight_menu_action();
        // Layout: [0] Attack, [1..=skill_count] skills, [last] Flee.
        if cursor == 0 {
            self.attack_round();
            Selection::Stay
        } else if cursor <= skill_count {
            self.cast_specialty_skill(cursor - 1)
        } else {
            self.attempt_flee(); // Flee
            Selection::Stay
        }
    }

    /// Try to flee the fight: a 1-in-3 roll (`forest.php` / `graveyard.php`
    /// `op=run`). Success drops the encounter — a torment escape additionally
    /// costs `min(favor, 5 + e_rand(0, level))` favor for the cowardice —
    /// while failure means the foes still get their round.
    fn attempt_flee(&mut self) {
        let Some(kind) = self.encounter.as_ref().map(|e| e.kind) else {
            self.goto(Mode::Village);
            return;
        };
        let mut rng = rand::thread_rng();
        if rng.gen_range(0..3) == 0 {
            // A successful escape banks whatever buff rounds are left.
            if let Some(enc) = self.encounter.take() {
                self.writeback_buffs(&enc);
                self.encounter = Some(enc);
            }
            if kind == FoeKind::Torment {
                let c = self.character.as_mut().unwrap();
                let cost = (5 + rng.gen_range(0..=c.level as u32)).min(c.favor);
                c.favor -= cost;
                // Write the battered soul pool back and rest the body again.
                c.soulpoints = c.hitpoints;
                c.hitpoints = 0;
                self.push_log(format!(
                    "You slip back among the graves. {} curses your cowardice: -{cost} favor.",
                    data::DEATH_OVERLORD
                ));
                self.encounter = None;
                self.goto(Mode::Graveyard);
            } else {
                self.push_log("You slip away and flee back to the village.".into());
                self.encounter = None;
                self.goto(Mode::Village);
            }
            self.save();
            return;
        }
        self.push_log("You try to flee, but your foe cuts off your escape!".into());
        let Some(mut enc) = self.encounter.take() else {
            return;
        };
        self.foes_strike(&mut enc, None);
        if self.character.as_ref().unwrap().hitpoints == 0 {
            self.defeat(&enc);
            return;
        }
        self.encounter = Some(enc);
        self.save();
    }

    /// Each living healer companion restores up to its rating: to the player
    /// while wounded, else the most wounded companion, else itself (LoGD's
    /// `heal` ability order). Logs what was bandaged.
    fn companion_heals(&mut self, player_max: u32) {
        let c = self.character.as_mut().unwrap();
        let mut logs = Vec::new();
        for i in 0..c.companions.len() {
            let super::combat::CompanionAbility::Heal(rating) = c.companions[i].ability else {
                continue;
            };
            if c.companions[i].hitpoints == 0 || rating == 0 {
                continue;
            }
            let medic = c.companions[i].name.clone();
            let missing = player_max.saturating_sub(c.hitpoints);
            if c.hitpoints > 0 && missing > 0 {
                let healed = rating.min(missing);
                c.hitpoints += healed;
                logs.push(format!("{medic} binds your wounds for {healed} HP."));
                continue;
            }
            // The most wounded companion (itself included).
            if let Some(j) = (0..c.companions.len())
                .filter(|&j| {
                    c.companions[j].hitpoints > 0
                        && c.companions[j].hitpoints < c.companions[j].max_hitpoints
                })
                .max_by_key(|&j| c.companions[j].max_hitpoints - c.companions[j].hitpoints)
            {
                let comp = &mut c.companions[j];
                let healed = rating.min(comp.max_hitpoints - comp.hitpoints);
                comp.hitpoints += healed;
                let target = comp.name.clone();
                if j == i {
                    logs.push(format!("{medic} patches their own wounds for {healed} HP."));
                } else {
                    logs.push(format!("{medic} tends {target} for {healed} HP."));
                }
            }
        }
        for line in logs {
            self.push_log(line);
        }
    }

    /// Every living foe (except `skip`, which already struck through the main
    /// resolver) takes its swing at the player. Marks `took_damage` and floors
    /// HP at zero; the caller checks for death.
    fn foes_strike(&mut self, enc: &mut Encounter, skip: Option<usize>) {
        let mut rng = rand::thread_rng();
        let (player, player_max) = self.player_fight_stats(enc.kind);
        for i in 0..enc.foes.len() {
            if Some(i) == skip || enc.foes[i].hp == 0 {
                continue;
            }
            let dmg = resolve_extra_foe_strike(&mut rng, player, enc.foes[i].combatant, &enc.buffs);
            if dmg > 0 {
                enc.took_damage = true;
            }
            let c = self.character.as_mut().unwrap();
            c.hitpoints = apply_signed(c.hitpoints, dmg, player_max);
            let hp = c.hitpoints;
            let name = enc.foes[i].name.clone();
            if dmg >= 0 {
                self.push_log(format!("{name} hits you for {dmg} ({hp} HP left)."));
            } else {
                self.push_log(format!("{name} fumbles its strike ({hp} HP left)."));
            }
            if hp == 0 {
                return;
            }
        }
    }

    fn attack_round(&mut self) {
        let Some(mut enc) = self.encounter.take() else {
            return;
        };
        let Some(target) = enc.target() else {
            self.victory(&enc);
            return;
        };
        let mut rng = rand::thread_rng();
        let (player, player_max) = self.player_fight_stats(enc.kind);
        // Field-medics bandage before the blades cross (upstream activates
        // `heal` first each round): the player first, then the most wounded
        // companion, then themselves. They still swing in the resolver below.
        self.companion_heals(player_max);
        // Companions live on the character and fight each round; the resolver
        // mutates their HP and removes any that fall. The player and their
        // companions all strike the current target.
        let outcome = {
            let c = self.character.as_mut().unwrap();
            resolve_round_buffed(
                &mut rng,
                player,
                enc.foes[target].combatant,
                &mut enc.buffs,
                &mut c.companions,
            )
        };

        if outcome.player_crit {
            self.push_log("A critical strike! You triple your power!".into());
        }
        if let Some(pm) = outcome.power_move {
            self.push_log(pm.label().into());
        }
        // Buff/companion flavor for this round.
        for msg in &outcome.messages {
            self.push_log(msg.clone());
        }

        // Damage is signed: a glancing blow (negative) heals the target.
        let foe = &mut enc.foes[target];
        foe.hp = apply_signed(foe.hp, outcome.damage_to_enemy, foe.max_hp);
        let (foe_name, foe_hp) = (foe.name.clone(), foe.hp);
        if outcome.damage_to_enemy >= 0 {
            self.push_log(format!(
                "You hit {foe_name} for {} ({foe_hp} HP left).",
                outcome.damage_to_enemy
            ));
        } else {
            self.push_log(format!(
                "Your blow glances off {foe_name}; it recovers {} HP ({foe_hp} left).",
                -outcome.damage_to_enemy
            ));
        }
        if foe_hp == 0 {
            let foe = &enc.foes[target];
            enc.slain.push(SlainFoe {
                level: foe.level,
                gold: foe.reward_gold,
                exp: foe.reward_exp,
            });
            self.push_log(format!("{foe_name} falls!"));
            if enc.living() == 0 {
                self.victory(&enc);
                return;
            }
        }

        // The target's counterstrike came out of the main resolver; every
        // other living foe swings too. Any landed hit spoils flawless.
        if outcome.damage_to_player > 0 {
            enc.took_damage = true;
        }
        {
            let c = self.character.as_mut().unwrap();
            c.hitpoints = apply_signed(c.hitpoints, outcome.damage_to_player, player_max);
            if outcome.player_heal > 0 {
                // Regen tops up to max, but never clips an active overheal.
                let cap = player_max.max(c.hitpoints);
                c.hitpoints = (c.hitpoints + outcome.player_heal).min(cap);
            }
        }
        let hp = self.character.as_ref().unwrap().hitpoints;
        if outcome.damage_to_player > 0 {
            let parting = if enc.foes[target].hp == 0 {
                " with a parting blow"
            } else {
                ""
            };
            self.push_log(format!(
                "{foe_name} hits you{parting} for {} ({hp} HP left).",
                outcome.damage_to_player
            ));
        } else if enc.foes[target].hp > 0 {
            self.push_log(format!("{foe_name} fumbles its strike ({hp} HP left)."));
        }
        if outcome.player_heal > 0 {
            self.push_log(format!(
                "You knit {} HP back together.",
                outcome.player_heal
            ));
        }
        if hp == 0 {
            self.defeat(&enc);
            return;
        }
        self.foes_strike(&mut enc, Some(target));
        if self.character.as_ref().unwrap().hitpoints == 0 {
            self.defeat(&enc);
            return;
        }
        self.encounter = Some(enc);
        self.save();
    }

    /// Cast the specialty skill at `skill_index` (rows after Attack/Flee in the
    /// fight menu): spend its uses, apply its buff to the encounter, then resolve
    /// a round with it active. Mirrors LoGD, where invoking a skill *is* the
    /// round's action.
    fn cast_specialty_skill(&mut self, skill_index: usize) -> Selection {
        let c = self.character.as_ref().unwrap();
        let skills = specialty::skills(c.specialty);
        let Some(skill) = skills.get(skill_index) else {
            return Selection::Stay;
        };
        let (level, attack) = (c.level as u32, c.attack());
        let (name, cost) = (skill.name, skill.cost);
        let effect = skill.effect(level, attack);
        if !self.character.as_mut().unwrap().spend_specialty_uses(cost) {
            self.push_log("You haven't the focus left for that skill.".into());
            return Selection::Stay;
        }
        match effect {
            SkillEffect::Buff(buff) => {
                if let Some(enc) = self.encounter.as_mut() {
                    enc.buffs.push(buff);
                }
            }
            SkillEffect::Summon(companion) => {
                self.push_log(format!(
                    "{} claws up from the earth to fight at your side.",
                    companion.name
                ));
                self.character.as_mut().unwrap().companions.push(companion);
            }
        }
        self.push_log(format!("You invoke {name}!"));
        self.attack_round();
        Selection::Stay
    }

    fn victory(&mut self, enc: &Encounter) {
        self.writeback_buffs(enc);
        match enc.kind {
            FoeKind::Creature => {
                let flawless = !enc.took_damage;
                let mut rng = rand::thread_rng();
                let c = self.character.as_mut().unwrap();
                let v = c.forest_victory(&enc.slain, flawless, &mut rng);
                self.push_log(format!(
                    "Victory! +{} gold, +{} experience.",
                    v.gold, v.exp
                ));
                if v.gem {
                    self.push_log("Something glitters in the remains: A GEM!".into());
                }
                if v.flawless {
                    if v.turn_refunded {
                        self.push_log(
                            "A flawless fight - you press on without spending a turn!".into(),
                        );
                    } else {
                        self.push_log(
                            "A flawless fight - a worthier foe would have spared the turn.".into(),
                        );
                    }
                }
                self.encounter = None;
                // Stay in the forest to fight again if turns remain.
                self.goto(Mode::Forest);
            }
            FoeKind::Master => {
                let c = self.character.as_mut().unwrap();
                c.advance_level();
                let lvl = c.level;
                let who = c.titled_name();
                self.push_log(format!(
                    "You defeat {}! You advance to level {} and are fully healed.",
                    enc.foes[0].name, lvl
                ));
                // Level-ups make the paper (`train.php`'s victory addnews).
                self.news(format!(
                    "{who} bested {} at the Proving Yard and rose to level {lvl}.",
                    enc.foes[0].name
                ));
                self.encounter = None;
                self.goto(Mode::Village);
            }
            FoeKind::Torment => {
                let favor = enc.foes[0].reward_exp;
                let name = enc.foes[0].name.clone();
                let c = self.character.as_mut().unwrap();
                c.favor = c.favor.saturating_add(favor);
                // The fight ran on the soul pool; write what's left back and
                // lay the body down again (graveyard.php's post-battle swap).
                c.soulpoints = c.hitpoints;
                c.hitpoints = 0;
                self.push_log(format!(
                    "{name} breaks beneath your torment. {} grants you {favor} favor.",
                    data::DEATH_OVERLORD
                ));
                self.encounter = None;
                self.goto(Mode::Graveyard);
            }
            FoeKind::Dragon => {
                let flawless = !enc.took_damage;
                let c = self.character.as_mut().unwrap();
                c.slay_dragon(flawless);
                // Every kill re-rolls the title off the ladder (`dragon.php`).
                let old_title = std::mem::take(&mut c.title);
                c.reroll_title(&mut rand::thread_rng());
                let (kills, title) = (c.dragon_kills, c.title.clone());
                let mut msg = format!(
                    "THE GREEN DRAGON IS SLAIN! Dragon kill #{kills}. A dragon point is yours to spend."
                );
                if flawless {
                    msg.push_str(" Flawless - not a scratch on you! Bonus gold and a gem.");
                }
                self.push_log(msg);
                if title != old_title {
                    self.push_log(format!("The realm knows you now as {title}."));
                }
                // The kill and the earned title both make the paper
                // (`dragon.php`'s two addnews calls).
                let who = self.character.as_ref().unwrap().titled_name();
                let name = self.character.as_ref().unwrap().name.clone();
                if kills == 1 {
                    self.news(format!("{who} has slain the terrible Green Dragon!"));
                } else {
                    self.news(format!(
                        "{who} has slain the terrible Green Dragon! It is their dragon kill #{kills}."
                    ));
                }
                if title != old_title {
                    self.news(format!("{name} has earned the title {title}."));
                }
                self.encounter = None;
                // The kill banks a dragon point; the spend gate opens at once.
                self.goto(Mode::SpendDragonPoints);
            }
        }
        self.save();
    }

    fn defeat(&mut self, enc: &Encounter) {
        self.writeback_buffs(enc);
        let c = self.character.as_mut().unwrap();
        let (who, level) = (c.titled_name(), c.level);
        // The killer for the log: the first foe still standing.
        let killer = enc
            .foes
            .iter()
            .find(|f| f.hp > 0)
            .map(|f| f.name.clone())
            .unwrap_or_else(|| enc.foes[0].name.clone());
        // Every defeat makes the paper with a taunt appended, exactly the
        // upstream set (forest, dragon, graveyard, master — all taunted).
        let taunt = data::taunt(&mut rand::thread_rng());
        match enc.kind {
            FoeKind::Master => {
                // A training loss isn't lethal in LoGD: the master halts before
                // the final blow and mends your wounds (heal to full), sending
                // you off to train harder. No death, no penalty.
                c.hitpoints = c.max_hitpoints();
                self.push_log(format!(
                    "{killer} bests you, then stays the final blow and heals your wounds. Train harder."
                ));
                self.news(format!(
                    "{who} challenged {killer} at the Proving Yard and was sent home schooled. {taunt}"
                ));
                self.encounter = None;
                self.goto(Mode::Village);
            }
            FoeKind::Torment => {
                // A graveyard defeat only drains the pool and ends today's
                // torments — gold, experience, and the bank are already
                // beyond a dead man's losing (`gravefights = 0`, no penalty).
                c.soulpoints = c.hitpoints; // zero: the pool was the fight
                c.grave_fights = 0;
                self.push_log(format!(
                    "{killer} scatters your essence. You can torment no more souls today."
                ));
                self.news(format!(
                    "{who}'s restless spirit was scattered by {killer} among the graves. {taunt}"
                ));
                self.encounter = None;
                self.goto(Mode::Graveyard);
            }
            _ => {
                c.die();
                self.push_log(format!(
                    "{killer} has slain you! Your gold is lost and you are dragged to the graveyard."
                ));
                if enc.kind == FoeKind::Dragon {
                    self.news(format!(
                        "{who} (level {level}) was burned to ash beneath the Green Dragon's flame. {taunt}"
                    ));
                } else {
                    self.news(format!(
                        "{who} (level {level}) was slain in the forest by {killer}. {taunt}"
                    ));
                }
                self.encounter = None;
                self.goto(Mode::Graveyard);
            }
        }
        self.save();
    }

    // --- shops --------------------------------------------------------------

    fn buy_gear(&mut self, weapon: bool) -> Selection {
        let c = self.character.as_ref().unwrap();
        let tiers = available_tiers(c, weapon);
        if self.cursor >= tiers.len() {
            return Selection::Stay;
        }
        let (tier, _cost) = tiers[self.cursor];
        let c = self.character.as_mut().unwrap();
        let ok = if weapon {
            c.buy_weapon(tier)
        } else {
            c.buy_armor(tier)
        };
        if ok {
            let name = if weapon {
                data::weapon_name(tier)
            } else {
                data::armor_name(tier)
            };
            self.push_log(format!("You equip the {name}."));
            self.save();
        } else {
            self.push_log("You can't afford that.".into());
        }
        Selection::Stay
    }

    // --- healer -------------------------------------------------------------

    fn select_healer(&mut self) -> Selection {
        let c = self.character.as_mut().unwrap();
        if c.hitpoints >= c.max_hitpoints() {
            self.push_log("You are already at full health.".into());
            return Selection::Stay;
        }
        // Rows run 100%, 90%, ... 10% (healer.php's potion shelf).
        let pct = 100u32.saturating_sub(self.cursor as u32 * 10);
        if !(10..=100).contains(&pct) {
            return Selection::Stay;
        }
        let cost = c.heal_cost(pct);
        match c.buy_heal(pct) {
            Some(healed) => {
                self.push_log(format!(
                    "The healer's draught knits {healed} HP back for {cost} gold."
                ));
                self.save();
            }
            None => self.push_log("You can't afford that draught.".into()),
        }
        Selection::Stay
    }

    // --- bank ---------------------------------------------------------------

    fn select_bank(&mut self) -> Selection {
        let c = self.character.as_mut().unwrap();
        match self.cursor {
            0 => {
                let amount = c.gold;
                c.deposit(amount);
                if c.gold_in_bank < 0 {
                    let debt = -c.gold_in_bank;
                    self.push_log(format!(
                        "You pay {amount} gold toward your debt ({debt} still owed)."
                    ));
                } else {
                    self.push_log(format!("You deposit {amount} gold."));
                }
            }
            1 => {
                let amount = c.gold_in_bank.max(0) as u64;
                c.withdraw(amount);
                self.push_log(format!("You withdraw {amount} gold."));
            }
            2 => {
                let amount = c.borrow(c.borrow_available());
                if amount > 0 {
                    self.push_log(format!(
                        "The banker counts out a loan of {amount} gold. Debt gathers interest daily."
                    ));
                } else {
                    self.push_log("The bank won't extend you any more credit.".into());
                }
            }
            _ => return Selection::Stay,
        }
        self.save();
        Selection::Stay
    }

    // --- the stables ------------------------------------------------------------

    /// Buy (or trade in for) the highlighted mount, or sell the current one
    /// (`stables.php`: purchases count the ⅔ trade-in refund; the new mount
    /// joins today's fights at once).
    fn select_stables(&mut self) -> Selection {
        let mounts = data::MOUNTS.len();
        if self.cursor < mounts {
            let traded_in = self.character.as_ref().unwrap().mount_data().map(|m| m.name);
            let c = self.character.as_mut().unwrap();
            if c.buy_mount(self.cursor as u8 + 1) {
                let mount = c.mount_data().unwrap().name;
                match traded_in {
                    Some(old) => self.push_log(format!(
                        "{} takes the {old} in part-exchange and saddles the {mount} for you.",
                        data::OSTLER
                    )),
                    None => self.push_log(format!(
                        "{} saddles the {mount} for you. It is eager for today's hunts.",
                        data::OSTLER
                    )),
                }
                self.save();
            } else {
                self.push_log("You can't afford that mount.".into());
            }
        } else if self.character.as_ref().unwrap().mount != 0 {
            let name = self.character.as_ref().unwrap().mount_data().unwrap().name;
            let c = self.character.as_mut().unwrap();
            if let Some(refund) = c.sell_mount() {
                self.push_log(format!(
                    "{} leads the {name} away and counts {refund} gems into your palm.",
                    data::OSTLER
                ));
                self.save();
            }
        }
        Selection::Stay
    }

    // --- the mercenary camp -------------------------------------------------

    /// Hire the highlighted mercenary, or pay the camp sawbones to mend a
    /// wounded companion (`mercenarycamp.php`).
    fn select_merc_camp(&mut self) -> Selection {
        let listings = merc_listings(self.character.as_ref().unwrap());
        if self.cursor < listings.len() {
            let merc = listings[self.cursor];
            let c = self.character.as_mut().unwrap();
            if c.hire_mercenary(merc) {
                self.push_log(format!(
                    "{} shoulders their kit and falls in at your side.",
                    merc.name
                ));
                self.save();
            } else {
                self.push_log("The camp won't take your terms.".into());
            }
        } else {
            let wounded = wounded_companions(self.character.as_ref().unwrap());
            let Some(&idx) = wounded.get(self.cursor - listings.len()) else {
                return Selection::Stay;
            };
            let c = self.character.as_mut().unwrap();
            if let Some(cost) = c.heal_companion(idx) {
                let name = c.companions[idx].name.clone();
                self.push_log(format!(
                    "The camp's sawbones patches {name} back to full for {cost} gold."
                ));
                self.save();
            } else {
                self.push_log("You can't afford the sawbones' fee.".into());
            }
        }
        Selection::Stay
    }

    // --- the inn --------------------------------------------------------------

    /// The common room: pick a destination inside the Sleeping Stag.
    fn select_inn(&mut self) -> Selection {
        match self.cursor {
            0 => self.goto(Mode::InnRoom),
            1 => self.goto(Mode::Barkeep),
            2 => {
                // One song a day (`sethsong.php`); the row gates the flag.
                let mut rng = rand::thread_rng();
                let lines = inn::bard_song(self.character.as_mut().unwrap(), &mut rng);
                for line in lines {
                    self.push_log(line);
                }
                self.save();
            }
            3 => self.goto(Mode::Drinks),
            4 => self.goto(Mode::Romance),
            _ => {}
        }
        Selection::Stay
    }

    /// Pay for the night's room (`inn_room.php`): the purse at cost, the bank
    /// at cost plus its 5% fee.
    fn select_inn_room(&mut self) -> Selection {
        let from_bank = match self.cursor {
            0 => false,
            1 => true,
            _ => {
                self.goto(Mode::Inn);
                return Selection::Stay;
            }
        };
        let c = self.character.as_mut().unwrap();
        match c.lodge(from_bank) {
            Some(cost) => {
                let source = if from_bank { "the bank's ledger" } else { "your purse" };
                self.push_log(format!(
                    "{cost} gold from {source}, and {} slides a heavy iron key across the bar. A warm bed is yours tonight.",
                    data::BARKEEP
                ));
                self.save();
                self.goto(Mode::Inn);
            }
            None => self.push_log("You can't cover the room.".into()),
        }
        Selection::Stay
    }

    /// Bribe the barkeep (`inn_bartender.php`): gems at 30/60/90%, gold at
    /// 25/~47/75% — paid up front, gone either way. Success opens the quiet
    /// word (the specialty switch).
    fn select_barkeep(&mut self) -> Selection {
        let mut rng = rand::thread_rng();
        let success = match self.cursor {
            0..=2 => {
                let gems = self.cursor as u32 + 1;
                let c = self.character.as_mut().unwrap();
                c.gems -= gems as u64;
                (rng.gen_range(0..=100)) < model::gem_bribe_chance(gems)
            }
            3..=5 => {
                let c = self.character.as_mut().unwrap();
                let amount = c.bribe_gold_amounts()[self.cursor - 3];
                let chance = model::gold_bribe_chance(amount, c.level);
                c.gold -= amount;
                (rng.gen_range(0..=100) as f64) < chance
            }
            _ => {
                self.goto(Mode::Potions);
                return Selection::Stay;
            }
        };
        self.save();
        if success {
            self.push_log(format!(
                "{} makes the bribe disappear and leans in for a quiet word.",
                data::BARKEEP
            ));
            self.goto(Mode::SwitchSpecialty);
        } else {
            self.push_log(format!(
                "{} makes the bribe disappear and suddenly remembers other customers.",
                data::BARKEEP
            ));
        }
        Selection::Stay
    }

    /// The bribed barkeep's prize: switch the specialty path. Each path's
    /// skill and uses are benched and resumed separately (upstream keeps them
    /// in per-module prefs).
    fn select_switch_specialty(&mut self) -> Selection {
        let options = switchable_specialties(self.character.as_ref().unwrap());
        let Some(&target) = options.get(self.cursor) else {
            self.goto(Mode::Inn);
            return Selection::Stay;
        };
        let c = self.character.as_mut().unwrap();
        let old = c.specialty;
        if c.switch_specialty(target) {
            let skill = c.specialty_skill;
            if old == Specialty::None {
                self.push_log(format!("You take up the {}.", target.name()));
            } else {
                self.push_log(format!(
                    "You set aside the {} and take up the {} (skill {skill}).",
                    old.name(),
                    target.name()
                ));
            }
            self.save();
        }
        self.goto(Mode::Inn);
        Selection::Stay
    }

    /// Buy a dose off the back shelf (`cedrikspotions.php`).
    fn select_potions(&mut self) -> Selection {
        let Some(&kind) = model::POTIONS.get(self.cursor) else {
            return Selection::Stay;
        };
        let c = self.character.as_mut().unwrap();
        if !c.buy_potion(kind) {
            self.push_log("You can't buy that dose.".into());
            return Selection::Stay;
        }
        let line = match kind {
            model::PotionKind::Charm => {
                "The tonic tastes of roses. You feel more charming already (+1 charm).".to_string()
            }
            model::PotionKind::Vitality => {
                "Oakblood settles deep in your bones: +1 max hitpoint, for good.".to_string()
            }
            model::PotionKind::Mending => {
                let hp = c.hitpoints;
                format!("Every ache vanishes at once, and then some ({hp} HP).")
            }
            model::PotionKind::Forgetting => {
                "Your craft slips away like a dream on waking. (A new path can be chosen in the village.)"
                    .to_string()
            }
            model::PotionKind::Transmutation => {
                "Your blood forgets itself, and your stomach objects violently. Your ancestry will be chosen anew, once the sickness passes."
                    .to_string()
            }
        };
        self.push_log(line);
        self.save();
        Selection::Stay
    }

    /// Order a drink off the taps (`modules/drinks.php`).
    fn select_drinks(&mut self) -> Selection {
        let Some(d) = data::DRINKS.get(self.cursor) else {
            return Selection::Stay;
        };
        let mut rng = rand::thread_rng();
        let lines = self.character.as_mut().unwrap().drink(d, &mut rng);
        for line in lines {
            self.push_log(line);
        }
        self.save();
        Selection::Stay
    }

    /// The corner table: flirt up the ladder (or the married visit), or just
    /// talk (`modules/lovers.php`).
    fn select_romance(&mut self) -> Selection {
        let married = self.character.as_ref().unwrap().married;
        let mut rng = rand::thread_rng();
        let chat_row = if married { 1 } else { data::FLIRT_RUNGS.len() };
        if self.cursor == chat_row {
            let line = inn::chat(self.character.as_ref().unwrap(), &mut rng);
            self.push_log(line);
            return Selection::Stay;
        }
        if married {
            let lines = inn::married_visit(self.character.as_mut().unwrap(), &mut rng);
            for line in lines {
                self.push_log(line);
            }
        } else {
            let out = inn::flirt(self.character.as_mut().unwrap(), self.cursor, &mut rng);
            for line in out.lines {
                self.push_log(line);
            }
            if let Some(item) = out.news {
                self.news(item);
            }
        }
        self.save();
        Selection::Stay
    }

    // --- the outhouse ---------------------------------------------------------

    /// Pick a stall (`modules/outhouse.php`): the 5-gold private one or the
    /// free trench. Either spends the day's visit.
    fn select_outhouse(&mut self) -> Selection {
        match self.cursor {
            0 => {
                let c = self.character.as_mut().unwrap();
                c.gold -= model::OUTHOUSE_COST;
                c.used_outhouse_today = true;
                self.push_log(format!(
                    "You pay {} gold for the private stall. It is, remarkably, almost clean.",
                    model::OUTHOUSE_COST
                ));
                self.save();
                self.goto(Mode::OuthouseWash(true));
            }
            1 => {
                self.character.as_mut().unwrap().used_outhouse_today = true;
                self.push_log("You brave the public trench. It is exactly as bad as feared.".into());
                self.save();
                self.goto(Mode::OuthouseWash(false));
            }
            _ => self.goto(Mode::Forest),
        }
        Selection::Stay
    }

    /// Wash up (the lucky-find rolls + sobering) or slip out unwashed (a coin
    /// in the muck and, likely, the morning paper).
    fn select_outhouse_wash(&mut self, paid: bool) -> Selection {
        let mut rng = rand::thread_rng();
        let mut lines = Vec::new();
        let mut news = None;
        if self.cursor == 0 {
            // The wash: 60% finds the private stall's dropped coins (then an
            // independent 25% gem); the trench needs a further 1-in-3.
            let c = self.character.as_mut().unwrap();
            let mut found = false;
            if rng.gen_range(1..=100) <= 60 {
                if paid {
                    c.gold += model::OUTHOUSE_GIVEBACK;
                    found = true;
                    lines.push(format!(
                        "Scrubbing up at the rain barrel, you find {} gold someone dropped in the mud.",
                        model::OUTHOUSE_GIVEBACK
                    ));
                    if rng.gen_range(1..=100) <= 25 {
                        c.gems += 1;
                        lines.push("Something else glitters down there: a GEM!".into());
                    }
                } else if rng.gen_range(1..=3) == 1 {
                    c.gold += model::OUTHOUSE_GIVEBACK;
                    found = true;
                    lines.push(format!(
                        "Scrubbing up at the rain barrel, you spot {} gold trodden into the mud.",
                        model::OUTHOUSE_GIVEBACK
                    ));
                }
            }
            if !found {
                lines.push("You scrub up at the rain barrel. Cleanliness is its own reward.".into());
            }
            // The wash sobers (`soberup` at 0.9), paid or free.
            if c.drunkenness > 0 {
                c.sober_up();
                lines.push("Leaving the outhouse, you feel a little more sober.".into());
            }
        } else {
            // Slipping out unwashed: a coin lost in the hurry, and the whole
            // village hears about the trailing paper either way.
            if rng.gen_range(1..=100) >= 50 {
                let c = self.character.as_mut().unwrap();
                if c.gold >= 1 {
                    c.gold -= 1;
                    lines.push("In your hurry you fumble a gold coin into the muck. It stays there.".into());
                }
                lines.push("You stride off. Somewhere behind you, someone starts laughing.".into());
                let who = self.character.as_ref().unwrap().titled_name();
                news = Some(format!(
                    "Ever graceful, {who} strode out of the forest privy trailing a banner of paper from one boot."
                ));
            } else {
                lines.push("You slip out. Nobody saw a thing.".into());
            }
        }
        for line in lines {
            self.push_log(line);
        }
        if let Some(item) = news {
            self.news(item);
        }
        self.save();
        self.goto(Mode::Forest);
        Selection::Stay
    }

    // --- the Dark Horse Tavern --------------------------------------------------

    /// Step into the Dark Horse (the accepted forest event): open the taproom
    /// and start the pot signboard loading.
    fn enter_tavern(&mut self) {
        self.tavern_view = TavernView::Hub;
        self.fivesix_pot_rx = Some(self.svc.load_fivesix_pot());
        self.push_log(
            "You push into the Dark Horse. Dice rattle somewhere back in the smoke.".into(),
        );
        self.goto(Mode::Tavern);
    }

    /// Drive whichever of the gambler's games is open.
    fn select_tavern(&mut self) -> Selection {
        let mut rng = rand::thread_rng();
        match self.tavern_view {
            TavernView::Hub => match self.cursor {
                0 => {
                    self.tavern_view = TavernView::DiceBet;
                    self.cursor = 0;
                }
                1 => self.play_fivesix(),
                2 => {
                    self.tavern_view = TavernView::StonesSide;
                    self.cursor = 0;
                }
                _ => self.goto(Mode::Forest),
            },
            TavernView::DiceBet => {
                let gold = self.character.as_ref().unwrap().gold;
                match bet_amount(self.cursor, gold) {
                    Some(bet) => {
                        let game = tavern::DiceGame::open(bet, &mut rng);
                        self.push_log(format!(
                            "You stake {bet} gold. The cup rattles, and you shake out a {}.",
                            game.roll
                        ));
                        self.tavern_view = TavernView::Dice(game);
                        self.cursor = 0;
                    }
                    None => {
                        self.tavern_view = TavernView::Hub;
                        self.cursor = 0;
                    }
                }
            }
            TavernView::Dice(mut game) => {
                if self.cursor == 1 && game.can_reroll() {
                    game.reroll(&mut rng);
                    self.push_log(format!(
                        "You shake again: a {} (roll {} of {}).",
                        game.roll, game.tries, tavern::DICE_MAX_ROLLS
                    ));
                    self.tavern_view = TavernView::Dice(game);
                    self.cursor = 0;
                } else {
                    // Standing: the old man rolls with his house rules.
                    let his = tavern::old_man_roll(game.roll, &mut rng);
                    let c = self.character.as_mut().unwrap();
                    let line = match his.cmp(&game.roll) {
                        std::cmp::Ordering::Greater => {
                            c.gold = c.gold.saturating_sub(game.bet);
                            format!(
                                "{} shows a {his} to your {}. He rakes in your {} gold.",
                                data::GAMBLER, game.roll, game.bet
                            )
                        }
                        std::cmp::Ordering::Less => {
                            c.gold += game.bet;
                            format!(
                                "{} shows a {his} to your {}. He pays out {} gold, scowling.",
                                data::GAMBLER, game.roll, game.bet
                            )
                        }
                        std::cmp::Ordering::Equal => format!(
                            "{} shows a {his} to your {}. A push; the stakes go home.",
                            data::GAMBLER, game.roll
                        ),
                    };
                    self.push_log(line);
                    self.save();
                    self.tavern_view = TavernView::Hub;
                    self.cursor = 0;
                }
            }
            TavernView::StonesSide => match self.cursor {
                0 | 1 => {
                    self.tavern_view = TavernView::StonesBet {
                        like_pair: self.cursor == 0,
                    };
                    self.cursor = 0;
                }
                _ => {
                    self.tavern_view = TavernView::Hub;
                    self.cursor = 0;
                }
            },
            TavernView::StonesBet { like_pair } => {
                let gold = self.character.as_ref().unwrap().gold;
                match bet_amount(self.cursor, gold) {
                    Some(bet) => {
                        self.push_log(format!(
                            "You stake {bet} gold on {} pairs. Six red stones and ten blue rattle into the bag.",
                            if like_pair { "like" } else { "unlike" }
                        ));
                        self.tavern_view =
                            TavernView::Stones(tavern::StonesGame::open(like_pair, bet));
                        self.cursor = 0;
                    }
                    None => {
                        self.tavern_view = TavernView::Hub;
                        self.cursor = 0;
                    }
                }
            }
            TavernView::Stones(mut game) => {
                let draw = game.draw(&mut rng);
                let color = |red: bool| if red { "red" } else { "blue" };
                self.push_log(format!(
                    "He draws {} and {}: the pair is {}.",
                    color(draw.first_red),
                    color(draw.second_red),
                    if draw.yours { "yours (+2 your pile)" } else { "his (+2 his pile)" }
                ));
                if game.finished() {
                    let payout = game.payout();
                    let c = self.character.as_mut().unwrap();
                    let line = match payout.cmp(&0) {
                        std::cmp::Ordering::Greater => {
                            c.gold += payout as u64;
                            format!(
                                "The bag runs dry at {} stones to {}. Your pile wins: +{} gold.",
                                game.player_pile, game.oldman_pile, game.bet
                            )
                        }
                        std::cmp::Ordering::Less => {
                            c.gold = c.gold.saturating_sub(game.bet);
                            format!(
                                "The bag runs dry at {} stones to {}. His pile wins: -{} gold.",
                                game.player_pile, game.oldman_pile, game.bet
                            )
                        }
                        std::cmp::Ordering::Equal => format!(
                            "Dead even at {} stones apiece. The stakes go home.",
                            game.player_pile
                        ),
                    };
                    self.push_log(line);
                    self.save();
                    self.tavern_view = TavernView::Hub;
                    self.cursor = 0;
                } else {
                    self.tavern_view = TavernView::Stones(game);
                }
            }
        }
        Selection::Stay
    }

    /// Throw the five dice (`game_fivesix.php`): the stake is paid at once and
    /// the shared pot settles asynchronously; [`State::tick`] lands the payout.
    fn play_fivesix(&mut self) {
        if self.fivesix_rx.is_some() {
            self.push_log("The gambler is still counting the pot.".into());
            return;
        }
        let c = self.character.as_mut().unwrap();
        if c.gold < model::FIVESIX_COST || c.fivesix_plays_today >= model::FIVESIX_PLAYS_PER_DAY {
            return;
        }
        c.gold -= model::FIVESIX_COST;
        c.fivesix_plays_today += 1;
        let mut rng = rand::thread_rng();
        let (dice, sixes) = tavern::fivesix_roll(&mut rng);
        let faces = dice
            .iter()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
            .join(" ");
        self.push_log(format!(
            "You pay {} gold and throw: {faces} - {sixes} six{}.",
            model::FIVESIX_COST,
            if sixes == 1 { "" } else { "es" }
        ));
        self.fivesix_rx = Some((
            sixes,
            self.svc
                .settle_fivesix(model::FIVESIX_COST, model::FIVESIX_MAX_POT, sixes),
        ));
        self.save();
    }

    /// Drain the tavern's async plumbing: the pot signboard read, and a
    /// pending Five Sixes settlement (paying the win, or refunding a failed
    /// round-trip).
    fn tick_tavern(&mut self) {
        if let Some(rx) = self.fivesix_pot_rx.as_mut() {
            let pot = *rx.borrow_and_update();
            if pot.is_some() {
                self.fivesix_pot = pot;
                self.fivesix_pot_rx = None;
            }
        }
        let Some((sixes, rx)) = self.fivesix_rx.as_mut() else {
            return;
        };
        let sixes = *sixes;
        let settled = match &*rx.borrow_and_update() {
            FiveSixLoad::Loading => return,
            FiveSixLoad::Ready { pot, left_over } => Some((*pot, *left_over)),
            FiveSixLoad::Failed => None,
        };
        self.fivesix_rx = None;
        let Some((pot, left_over)) = settled else {
            // The DB round-trip failed: the play never counted, so the stake
            // comes back.
            self.character.as_mut().unwrap().gold += model::FIVESIX_COST;
            self.push_log(
                "The gambler knocks the pot over mid-count and calls the throw off. Your stake is returned.".into(),
            );
            self.save();
            return;
        };
        self.fivesix_pot = Some(left_over);
        let win = if sixes >= 5 {
            pot
        } else if sixes == 4 || sixes == 3 {
            pot - left_over
        } else {
            0
        };
        if win == 0 {
            self.push_log("No luck. Your stake feeds the pot.".into());
            return;
        }
        let who = {
            let c = self.character.as_mut().unwrap();
            c.gold += win;
            c.titled_name()
        };
        if sixes >= 5 {
            self.push_log(format!("FIVE SIXES! The whole pot of {win} gold is yours!"));
            self.news(format!(
                "{who} rolled five sixes at the Dark Horse Tavern and swept the pot of {win} gold."
            ));
        } else if sixes == 4 {
            self.push_log(format!("Four sixes! A tenth of the pot: +{win} gold."));
            self.news(format!(
                "{who} rolled four sixes at the Dark Horse Tavern and won {win} gold."
            ));
        } else {
            self.push_log(format!("Three sixes pay a sliver of the pot: +{win} gold."));
            self.news(format!(
                "{who} rolled three sixes at the Dark Horse Tavern and won {win} gold."
            ));
        }
        self.save();
    }

    // --- dragon points --------------------------------------------------------

    /// Spend one dragon point on the highlighted upgrade; the gate lifts once
    /// the last point is allocated.
    fn select_dragon_point(&mut self) -> Selection {
        let kind = match self.cursor {
            0 => DragonPointKind::Hp,
            1 => DragonPointKind::ForestFights,
            2 => DragonPointKind::Attack,
            3 => DragonPointKind::Defense,
            _ => return Selection::Stay,
        };
        let c = self.character.as_mut().unwrap();
        if !c.spend_dragon_point(kind) {
            self.goto(Mode::Village);
            return Selection::Stay;
        }
        let left = c.dragon_points_unspent;
        let alive = c.alive;
        let race = c.race;
        let style = c.style;
        self.push_log(format!("Dragon point spent: {}.", kind.label()));
        if left == 0 {
            // The next gate in upstream's order: style, race, then play.
            self.goto(if style == model::AddressStyle::Unchosen {
                Mode::ChooseStyle
            } else if race == Race::None {
                Mode::ChooseRace
            } else if alive {
                Mode::Village
            } else {
                Mode::Graveyard
            });
        }
        self.save();
        Selection::Stay
    }

    // --- helpers ------------------------------------------------------------

    fn push_log(&mut self, line: String) {
        self.log.push_back(line);
        while self.log.len() > LOG_CAP {
            self.log.pop_front();
        }
    }

    /// Persist the current character, fire-and-forget.
    fn save(&mut self) {
        if let Some(c) = self.character.as_ref() {
            self.svc.save_character(self.user_id, c);
        }
    }

    /// Persist on the way out of the game (called from `leave`).
    pub fn save_on_leave(&self) {
        if let Some(c) = self.character.as_ref() {
            self.svc.save_character(self.user_id, c);
        }
    }
}

/// Apply signed combat damage to an HP pool. Positive damage subtracts;
/// negative damage (a glancing blow) heals the target. Heals cap at `max` —
/// but an *existing* overheal (a mending draught, the bard's boost) is never
/// clipped by taking damage, matching how upstream lets HP ride above max
/// until the healer's normalize.
fn apply_signed(hp: u32, dmg: i32, max: u32) -> u32 {
    let cap = max.max(hp) as i64;
    (hp as i64 - dmg as i64).clamp(0, cap) as u32
}

/// The result of activating a menu row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Selection {
    /// Stay in the game; the UI updates.
    Stay,
    /// Leave the door, returning to the Games hub.
    Leave,
}

// --- menu builders (pure, so they can be unit-tested) -----------------------

fn village_menu(c: &Character) -> Vec<(String, bool)> {
    let mut rows = vec![
        (format!("The Forest ({} turns left)", c.turns), c.turns > 0),
        (
            "The Proving Yard (warrior training)".into(),
            c.can_challenge_master(),
        ),
    ];
    if c.specialty == Specialty::None {
        rows.push(("Choose a Specialty".into(), true));
    }
    if c.can_seek_dragon() {
        rows.push(("Seek Out the Green Dragon".into(), true));
    }
    rows.push(("Ironroost Weapons".into(), true));
    rows.push(("Duskmail Armoury".into(), true));
    rows.push((
        "The Mendery (healer)".into(),
        c.hitpoints != c.max_hitpoints(),
    ));
    rows.push(("The Coinvault (bank)".into(), true));
    rows.push(("The Stables (mounts)".into(), true));
    rows.push(("The Mercenary Camp (allies)".into(), true));
    rows.push((format!("{} (the inn)", data::INN_NAME), true));
    rows.push(("The Daily News".into(), true));
    rows.push(("Leave the realm".into(), true));
    rows
}

/// The daily news pager: one day per page, like upstream's `news.php`.
fn news_menu(days_back: i64) -> Vec<(String, bool)> {
    vec![
        ("Earlier news (the day before)".into(), true),
        ("Later news (the day after)".into(), days_back > 0),
        ("Back to the village square".into(), true),
    ]
}

fn forest_menu(c: &Character) -> Vec<(String, bool)> {
    let has_turns = c.turns > 0;
    vec![
        ("Go Slumming (weaker prey)".into(), has_turns),
        ("Look for Something to Kill".into(), has_turns),
        ("Go Thrillseeking (deadlier prey)".into(), has_turns),
        // The outhouse (`modules/outhouse.php`): a forest amenity, once a day.
        (
            "The Outhouse (a smell among the trees)".into(),
            !c.used_outhouse_today,
        ),
    ]
}

/// The fight menu: Attack, then any unlocked specialty skills (shown with their
/// use-cost and disabled when the pool can't pay), then Flee. The skill rows sit
/// between Attack and Flee so those two keep stable positions.
fn fight_menu(c: &Character) -> Vec<(String, bool)> {
    let mut rows = vec![("Attack".into(), true)];
    // The dead fight with bare essence: no specialty skills beyond the grave
    // (upstream's graveyard calls `fightnav(false, ...)`).
    if c.alive {
        for skill in specialty::skills(c.specialty) {
            rows.push((
                format!(
                    "{} ({} use{})",
                    skill.name,
                    skill.cost,
                    if skill.cost == 1 { "" } else { "s" }
                ),
                c.specialty_uses >= skill.cost,
            ));
        }
    }
    rows.push(("Flee".into(), true));
    rows
}

/// The dead realm's hub (`graveyard.php` + the mausoleum): torment souls for
/// favor, restore the soul pool, buy a resurrection, or wait out the day.
fn graveyard_menu(c: &Character) -> Vec<(String, bool)> {
    let restore = c.soul_restore_cost();
    vec![
        (
            format!("Torment a lost soul ({} left today)", c.grave_fights),
            c.grave_fights > 0,
        ),
        (
            format!("The Mausoleum: restore your soul ({restore} favor)"),
            c.soulpoints < c.max_soulpoints() && c.favor >= restore,
        ),
        (
            format!(
                "Rise from the grave ({} favor)",
                model::RESURRECTION_FAVOR_COST
            ),
            c.favor >= model::RESURRECTION_FAVOR_COST,
        ),
        ("Wait for a new day (leave the realm)".into(), true),
    ]
}

/// The four ancestry choices for the forced race gate, in [`model::RACES`]
/// order. Perk numbers are upstream's; the names and framing are ours.
fn race_menu() -> Vec<(String, bool)> {
    model::RACES
        .iter()
        .map(|race| {
            let perk = match race {
                Race::Plainsborn => "tireless: +2 forest fights each day",
                Race::Wealdkin => "wary: bonus defense that grows with level",
                Race::Deepfolk => "gold-nosed: +20% creature gold, safe in mines",
                Race::Cragborn => "brutal: bonus attack that grows with level",
                Race::None => unreachable!("RACES holds only choosable races"),
            };
            (format!("The {} ({perk})", race.name()), true)
        })
        .collect()
}

/// The two address styles for the one-time chooser, with example titles off
/// the ladder so the choice is legible.
fn style_menu() -> Vec<(String, bool)> {
    vec![
        (
            "The first style of address (Ashlord, Dragonlord)".into(),
            true,
        ),
        (
            "The second style of address (Ashlady, Dragonlady)".into(),
            true,
        ),
    ]
}

/// The three specialty choices for the one-time chooser.
fn specialty_menu() -> Vec<(String, bool)> {
    vec![
        ("Mystical Powers (regeneration, life-siphon)".into(), true),
        ("Dark Arts (minions, curses)".into(), true),
        ("Thief Skills (poison, backstab)".into(), true),
    ]
}

/// The pending forest event's two choices, or empty if none is staged.
fn event_menu(c: &Character, event: Option<ForestEvent>) -> Vec<(String, bool)> {
    match event.and_then(|e| e.present(c).choice) {
        Some((accept, decline)) => vec![(accept.into(), true), (decline.into(), true)],
        None => Vec::new(),
    }
}

/// The healer's shelf: a complete heal, then the discount draughts at 90%
/// down to 10% of the damage (LoGD `healer.php` sells every step of ten).
fn healer_menu(c: &Character) -> Vec<(String, bool)> {
    let needs = c.hitpoints < c.max_hitpoints();
    let mut rows = vec![(
        format!("Complete healing ({} gold)", c.heal_cost(100)),
        needs && c.gold >= c.heal_cost(100),
    )];
    for pct in (10..=90).rev().step_by(10) {
        rows.push((
            format!("Heal {pct}% ({} gold)", c.heal_cost(pct)),
            needs && c.gold >= c.heal_cost(pct),
        ));
    }
    rows
}

fn bank_menu(c: &Character) -> Vec<(String, bool)> {
    let balance_row = if c.gold_in_bank < 0 {
        (
            format!("Pay down debt ({} owed) with all gold", -c.gold_in_bank),
            c.gold > 0,
        )
    } else {
        (format!("Deposit all ({} gold)", c.gold), c.gold > 0)
    };
    vec![
        balance_row,
        (
            format!("Withdraw all ({} gold)", c.gold_in_bank.max(0)),
            c.gold_in_bank > 0,
        ),
        (
            format!("Take a loan ({} gold available)", c.borrow_available()),
            c.borrow_available() > 0,
        ),
    ]
}

/// The forced dragon-point allocation gate (LoGD's new-day spend screen).
fn dragon_point_menu() -> Vec<(String, bool)> {
    [
        DragonPointKind::Hp,
        DragonPointKind::ForestFights,
        DragonPointKind::Attack,
        DragonPointKind::Defense,
    ]
    .into_iter()
    .map(|k| (k.label().to_string(), true))
    .collect()
}

/// The Sleeping Stag's common room (`inn.php`): the room, the barkeep, the
/// bard, the taps, and the corner table.
fn inn_menu(c: &Character) -> Vec<(String, bool)> {
    let room = c.inn_room_cost();
    let room_row = if c.lodged_today {
        ("Your room is paid (a warm bed waits upstairs)".into(), false)
    } else {
        (
            format!("A room for the night ({room} gold)"),
            c.gold >= room || c.gold_in_bank >= c.inn_room_bank_cost() as i64,
        )
    };
    vec![
        room_row,
        (format!("Speak with {} the barkeep", data::BARKEEP), true),
        (
            format!("Hear {} the bard sing (once a day)", data::BARD),
            !c.heard_bard_today,
        ),
        ("Order a drink".into(), true),
        (format!("Sit with {}", data::partner(c.style)), true),
    ]
}

/// The room's two purses (`inn_room.php`): gold at cost, the bank at +5%.
fn inn_room_menu(c: &Character) -> Vec<(String, bool)> {
    let open = !c.lodged_today;
    vec![
        (
            format!("Pay {} gold from your purse", c.inn_room_cost()),
            open && c.gold >= c.inn_room_cost(),
        ),
        (
            format!("Charge the bank {} gold (5% fee)", c.inn_room_bank_cost()),
            open && c.gold_in_bank >= c.inn_room_bank_cost() as i64,
        ),
        ("Think better of it".into(), true),
    ]
}

/// The barkeep's counter (`inn_bartender.php`): the six bribes (paid win or
/// lose), and the back shelf.
fn barkeep_menu(c: &Character) -> Vec<(String, bool)> {
    let mut rows: Vec<(String, bool)> = (1..=3u32)
        .map(|g| {
            (
                format!(
                    "Slip him {g} gem{} ({}% odds of a quiet word)",
                    if g == 1 { "" } else { "s" },
                    model::gem_bribe_chance(g)
                ),
                c.gems >= g as u64,
            )
        })
        .collect();
    for amount in c.bribe_gold_amounts() {
        let pct = model::gold_bribe_chance(amount, c.level);
        rows.push((
            format!("Slide him {amount} gold ({pct:.0}% odds of a quiet word)"),
            c.gold >= amount,
        ));
    }
    rows.push((
        format!(
            "Browse the back shelf ({} gems a dose)",
            model::POTION_COST_GEMS
        ),
        true,
    ));
    rows
}

/// The specialty paths the barkeep can switch you onto (everything but the
/// current one).
fn switchable_specialties(c: &Character) -> Vec<Specialty> {
    [Specialty::Mystical, Specialty::DarkArts, Specialty::Thief]
        .into_iter()
        .filter(|&s| s != c.specialty)
        .collect()
}

/// The bribed switch menu: each other path (benched progress shown), or keep.
fn switch_specialty_menu(c: &Character) -> Vec<(String, bool)> {
    let mut rows: Vec<(String, bool)> = switchable_specialties(c)
        .into_iter()
        .map(|s| {
            let benched = model::specialty_index(s)
                .map(|i| c.benched_specialties[i].0)
                .unwrap_or(0);
            let label = if benched > 0 {
                format!("Take up {} again (skill {benched} waiting)", s.name())
            } else {
                format!("Take up {} afresh", s.name())
            };
            (label, true)
        })
        .collect();
    rows.push(("Keep to your current path".into(), true));
    rows
}

/// The back shelf (`cedrikspotions.php`): five potions at a flat gem price.
fn potions_menu(c: &Character) -> Vec<(String, bool)> {
    model::POTIONS
        .iter()
        .map(|&p| {
            (
                format!(
                    "{} - {} ({} gems)",
                    p.name(),
                    p.blurb(),
                    model::POTION_COST_GEMS
                ),
                c.can_buy_potion(p),
            )
        })
        .collect()
}

/// The taps (`modules/drinks.php`): the three drinks, or a cut-off drunk.
fn drinks_menu(c: &Character) -> Vec<(String, bool)> {
    if c.drunkenness > model::MAX_DRUNKENNESS_SERVED {
        return vec![(
            format!(
                "{} eyes your sway and refuses to pour another drop today",
                data::BARKEEP
            ),
            false,
        )];
    }
    data::DRINKS
        .iter()
        .map(|d| {
            let cost = c.level as u64 * d.cost_per_level;
            let tag = if d.hard { ", hard liquor" } else { "" };
            (
                format!("{} ({cost} gold{tag})", d.name),
                c.can_be_served(d) && c.gold >= cost,
            )
        })
        .collect()
}

/// The corner table (`modules/lovers.php`): the flirt ladder (or the married
/// visit), and free talk.
fn romance_menu(c: &Character) -> Vec<(String, bool)> {
    let partner = data::partner(c.style);
    let mut rows = Vec::new();
    if c.married {
        rows.push((
            format!("Steal an hour with {partner}"),
            !c.flirted_today,
        ));
    } else {
        for (i, label) in data::FLIRT_RUNGS.iter().enumerate() {
            let hint = if i < model::FLIRT_LADDER.len() {
                format!("{label} (sure at {} charm)", model::FLIRT_LADDER[i].0)
            } else {
                format!("{label} (needs {} charm)", model::MARRY_CHARM_REQUIRED)
            };
            rows.push((hint, !c.flirted_today));
        }
    }
    rows.push(("Just talk a while".into(), true));
    rows
}

/// The outhouse's stalls (`modules/outhouse.php`).
fn outhouse_menu(c: &Character) -> Vec<(String, bool)> {
    vec![
        (
            format!("The private stall ({} gold)", model::OUTHOUSE_COST),
            c.gold >= model::OUTHOUSE_COST,
        ),
        ("The public trench (free)".into(), true),
        ("Hold your nose and move on".into(), true),
    ]
}

/// After the stall: the wash (and its lucky finds) or the shortcut.
fn outhouse_wash_menu() -> Vec<(String, bool)> {
    vec![
        ("Wash up at the rain barrel".into(), true),
        ("Slip out without washing".into(), true),
    ]
}

/// A stake for the gambler's even-money games (our menu stands in for
/// upstream's free-text bet box): a short ladder, or everything.
fn bet_menu(gold: u64) -> Vec<(String, bool)> {
    let mut rows: Vec<(String, bool)> = [10u64, 50, 100]
        .iter()
        .map(|&b| (format!("Stake {b} gold"), gold >= b))
        .collect();
    rows.push((format!("Stake everything ({gold} gold)"), gold > 0));
    rows.push(("Never mind".into(), true));
    rows
}

/// The stake the bet-menu row at `cursor` puts down.
fn bet_amount(cursor: usize, gold: u64) -> Option<u64> {
    match cursor {
        0 => Some(10),
        1 => Some(50),
        2 => Some(100),
        3 => Some(gold),
        _ => None,
    }
}

/// The Dark Horse (`darkhorse.php` + the three game modules), by view.
fn tavern_menu(
    c: &Character,
    view: TavernView,
    pot: Option<u64>,
    settling: bool,
) -> Vec<(String, bool)> {
    match view {
        TavernView::Hub => {
            let fivesix = match pot {
                Some(p) => format!(
                    "Five Sixes ({} gold a throw; {p} gold in the pot)",
                    model::FIVESIX_COST
                ),
                None => format!("Five Sixes ({} gold a throw)", model::FIVESIX_COST),
            };
            vec![
                (
                    "Dice with the one-eyed gambler (high die wins)".into(),
                    c.gold > 0,
                ),
                (
                    fivesix,
                    c.gold >= model::FIVESIX_COST
                        && c.fivesix_plays_today < model::FIVESIX_PLAYS_PER_DAY
                        && !settling,
                ),
                ("Stones (call the pairs)".into(), c.gold > 0),
                ("Back out into the forest".into(), true),
            ]
        }
        TavernView::DiceBet | TavernView::StonesBet { .. } => bet_menu(c.gold),
        TavernView::Dice(g) => {
            let mut rows = vec![(format!("Stand on your {}", g.roll), true)];
            if g.can_reroll() {
                rows.push((
                    format!("Shake again ({} left)", tavern::DICE_MAX_ROLLS - g.tries),
                    true,
                ));
            }
            rows
        }
        TavernView::StonesSide => vec![
            ("Call like pairs (matched colors pay you)".into(), true),
            ("Call unlike pairs (mixed colors pay you)".into(), true),
            ("Never mind".into(), true),
        ],
        TavernView::Stones(g) => vec![(
            format!(
                "Draw two stones (your pile {}, his {}, {} left in the bag)",
                g.player_pile,
                g.oldman_pile,
                g.red + g.blue
            ),
            true,
        )],
    }
}

/// The stable's stalls (`stables.php`): the three stock mounts (buying counts
/// the ⅔ trade-in refund toward the price), plus a sell row while mounted.
fn stables_menu(c: &Character) -> Vec<(String, bool)> {
    let refund = c.mount_refund();
    let mut rows: Vec<(String, bool)> = data::MOUNTS
        .iter()
        .enumerate()
        .map(|(i, m)| {
            if c.mount == i as u8 + 1 {
                (format!("{} (yours, saddled and ready)", m.name), false)
            } else {
                (
                    format!(
                        "Buy the {} ({} gems; +{} fights/day, {} mounted rounds)",
                        m.name, m.cost_gems, m.forest_fights, m.buff_rounds
                    ),
                    c.gems + refund >= m.cost_gems,
                )
            }
        })
        .collect();
    if let Some(m) = c.mount_data() {
        rows.push((
            format!("Sell the {} back ({refund} gem refund)", m.name),
            true,
        ));
    }
    rows
}

/// The camp's hire list: the two stock mercenaries, plus the Deepfolk-only
/// crag bear (`racedwarf.php`'s exclusive listing).
fn merc_listings(c: &Character) -> Vec<&'static data::Mercenary> {
    let mut list: Vec<&'static data::Mercenary> = data::MERCENARIES.iter().collect();
    if c.race == Race::Deepfolk {
        list.push(&data::DEEPFOLK_BEAR);
    }
    list
}

/// Indices of companions the camp sawbones can mend (wounded ones).
fn wounded_companions(c: &Character) -> Vec<usize> {
    (0..c.companions.len())
        .filter(|&i| c.companion_heal_cost(i).is_some())
        .collect()
}

/// The mercenary camp (`mercenarycamp.php`): hires (gated by the one-hire cap
/// and both currencies), then a mend row per wounded companion.
fn merc_camp_menu(c: &Character) -> Vec<(String, bool)> {
    let mut rows: Vec<(String, bool)> = merc_listings(c)
        .into_iter()
        .map(|merc| {
            (
                format!(
                    "Hire {} ({} gold + {} gems)",
                    merc.name, merc.cost_gold, merc.cost_gems
                ),
                c.can_hire(merc),
            )
        })
        .collect();
    for i in wounded_companions(c) {
        let cost = c.companion_heal_cost(i).unwrap();
        rows.push((
            format!("Mend {} ({cost} gold)", c.companions[i].name),
            c.gold >= cost,
        ));
    }
    rows
}

fn training_menu(c: &Character) -> Vec<(String, bool)> {
    match c.current_master() {
        Some((master, _, _)) => vec![(
            format!("Challenge {}", master.name),
            c.can_challenge_master(),
        )],
        None => vec![("You have mastered all training.".into(), false)],
    }
}

/// Up to the next five gear upgrade tiers with their trade-in-adjusted cost.
///
/// Level-gated, mirroring LoGD: a shop only stocks gear up to the character's
/// own level, so you can't grind gold to out-gear your rank and trivialize the
/// master fights. The cost ladder still gates affordability on top of this.
fn available_tiers(c: &Character, weapon: bool) -> Vec<(u8, u64)> {
    let current = if weapon { c.weapon_tier } else { c.armor_tier };
    let ceiling = c.level.min(data::COST_LADDER.len() as u8);
    (current + 1..=ceiling)
        .take(5)
        .filter_map(|tier| {
            let cost = if weapon {
                c.weapon_upgrade_cost(tier)
            } else {
                c.armor_upgrade_cost(tier)
            }?;
            Some((tier, cost))
        })
        .collect()
}

fn shop_menu(c: &Character, weapon: bool) -> Vec<(String, bool)> {
    let tiers = available_tiers(c, weapon);
    if tiers.is_empty() {
        let current = if weapon { c.weapon_tier } else { c.armor_tier };
        let msg = if current >= data::MAX_LEVEL {
            "You already wield the finest in the land. (nothing to buy)"
        } else {
            "Nothing here befits your rank yet. Advance a level for finer gear. (nothing to buy)"
        };
        return vec![(msg.into(), false)];
    }
    let name = if weapon {
        data::weapon_name
    } else {
        data::armor_name
    };
    tiers
        .into_iter()
        .map(|(tier, cost)| {
            (
                format!("{} (power {tier}) - {cost} gold", name(tier)),
                c.gold >= cost,
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lvl(level: u8) -> Character {
        let mut c = Character::new("t", 0);
        c.level = level;
        c.hitpoints = c.max_hitpoints();
        c
    }

    #[test]
    fn village_menu_gates_on_state() {
        let mut c = lvl(1);
        c.turns = 0;
        let rows = village_menu(&c);
        // Forest row disabled with no turns.
        assert!(!rows[0].1);
        // Healer disabled at full health.
        let healer = rows
            .iter()
            .find(|(l, _)| l.starts_with("The Mendery"))
            .unwrap();
        assert!(!healer.1);
        // Dragon not offered below level 15.
        assert!(!rows.iter().any(|(l, _)| l.starts_with("Seek Out")));
    }

    #[test]
    fn dragon_offered_at_max_level() {
        let c = lvl(15);
        let rows = village_menu(&c);
        assert!(rows.iter().any(|(l, _)| l.starts_with("Seek Out")));
    }

    #[test]
    fn shop_lists_affordable_upgrades() {
        let mut c = lvl(2); // level 2 stocks tiers 1 and 2
        c.gold = 100; // affords tier 1 (48) but not tier 2 (189 after trade-in)
        let tiers = available_tiers(&c, true);
        assert_eq!(tiers[0], (1, 48));
        let menu = shop_menu(&c, true);
        assert!(menu[0].1); // tier 1 affordable
        assert!(!menu[1].1); // tier 2 not
    }

    #[test]
    fn shop_is_level_gated() {
        // Even with limitless gold, a shop only stocks gear up to your level.
        let mut c = lvl(3);
        c.gold = 1_000_000;
        let tiers = available_tiers(&c, true);
        assert!(tiers.iter().all(|(t, _)| *t <= 3));
        assert_eq!(tiers.last().unwrap().0, 3);
        // Out of upgrades for your rank shows the level-gated nudge, not "finest".
        c.weapon_tier = 3;
        let menu = shop_menu(&c, true);
        assert!(menu[0].0.contains("Advance a level"));
    }

    #[test]
    fn bank_menu_reflects_balances() {
        let mut c = lvl(3);
        c.gold = 200;
        c.gold_in_bank = 0;
        let rows = bank_menu(&c);
        assert!(rows[0].1); // can deposit
        assert!(!rows[1].1); // nothing to withdraw
        // The loan row offers the full level-scaled credit line (3 * 20).
        assert!(rows[2].0.contains("60 gold available"));
        assert!(rows[2].1);

        // In debt: the deposit row becomes a pay-down and the credit shrinks.
        c.gold_in_bank = -40;
        let rows = bank_menu(&c);
        assert!(rows[0].0.starts_with("Pay down debt (40 owed)"));
        assert!(!rows[1].1); // nothing (positive) to withdraw
        assert!(rows[2].0.contains("20 gold available"));
    }

    #[test]
    fn healer_menu_stocks_the_full_percent_shelf() {
        let mut c = lvl(5);
        c.hitpoints = c.max_hitpoints() - 20; // full cost 48
        c.gold = 24;
        let rows = healer_menu(&c);
        // 100% plus 90..10 by tens.
        assert_eq!(rows.len(), 10);
        assert!(rows[0].0.starts_with("Complete healing (48 gold)"));
        assert!(!rows[0].1); // can't afford 48
        assert!(rows[1].0.starts_with("Heal 90%"));
        // 50% costs 24 — exactly affordable (row index 5: 100,90,80,70,60,50).
        assert!(rows[5].0.starts_with("Heal 50% (24 gold)"));
        assert!(rows[5].1);
        assert!(rows[9].0.starts_with("Heal 10% (5 gold)"));
    }

    #[test]
    fn graveyard_menu_gates_on_favor_and_fights() {
        let mut c = lvl(5); // max soulpoints 75
        c.die();
        c.grave_fights = 0;
        c.favor = 0;
        c.soulpoints = 55; // missing 20: restore costs round(200/75) = 3
        let rows = graveyard_menu(&c);
        assert!(rows[0].0.contains("0 left today"));
        assert!(!rows[0].1); // no torments left
        assert!(rows[1].0.contains("(3 favor)"));
        assert!(!rows[1].1); // can't afford restoration
        assert!(!rows[2].1); // resurrection needs 100 favor
        assert!(rows[3].1); // waiting always works

        c.grave_fights = 4;
        c.favor = 100;
        let rows = graveyard_menu(&c);
        assert!(rows[0].1);
        assert!(rows[1].1);
        assert!(rows[2].1);

        // A whole soul has nothing to restore, whatever the favor.
        c.soulpoints = c.max_soulpoints();
        assert!(!graveyard_menu(&c)[1].1);
    }

    #[test]
    fn fight_menu_hides_skills_from_the_dead() {
        let mut c = lvl(5);
        c.choose_specialty(Specialty::Thief);
        // Alive: Attack + 4 skills + Flee.
        assert_eq!(fight_menu(&c).len(), 6);
        // Dead (a torment fight): bare essence only.
        c.die();
        let rows = fight_menu(&c);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].0, "Attack");
        assert_eq!(rows[1].0, "Flee");
    }

    #[test]
    fn stables_menu_counts_the_trade_in() {
        let mut c = lvl(5);
        c.gems = 6;
        let rows = stables_menu(&c);
        assert_eq!(rows.len(), 3);
        assert!(rows[0].1); // pony affordable at 6 gems
        assert!(!rows[1].1); // courser (10 gems) not

        // Owning the pony adds its 4-gem refund to buying power and a sell row.
        c.mount = 1;
        let rows = stables_menu(&c);
        assert_eq!(rows.len(), 4);
        assert!(!rows[0].1); // your own stall is not for sale
        assert!(rows[1].1); // 6 gems + 4 refund covers the courser
        assert!(rows[3].0.contains("4 gem refund"));
    }

    #[test]
    fn merc_camp_lists_hires_and_mending() {
        let mut c = lvl(5);
        c.gold = 10_000;
        c.gems = 10;
        assert_eq!(merc_camp_menu(&c).len(), 2);

        // The crag bear is a Deepfolk-only listing.
        c.race = Race::Deepfolk;
        let rows = merc_camp_menu(&c);
        assert_eq!(rows.len(), 3);
        assert!(rows[2].0.contains("Crag Bear"));

        // One hire fills the cap: every hire row disables. Wounding the hire
        // adds a mend row priced by the sawbones formula.
        assert!(c.hire_mercenary(&data::MERCENARIES[0]));
        assert!(merc_camp_menu(&c).iter().all(|(_, enabled)| !enabled));
        c.companions[0].hitpoints = 1;
        let rows = merc_camp_menu(&c);
        assert!(rows.last().unwrap().0.starts_with("Mend Skarn"));
        assert!(rows.last().unwrap().1);
    }

    #[test]
    fn race_menu_offers_the_four_ancestries() {
        let rows = race_menu();
        assert_eq!(rows.len(), model::RACES.len());
        assert!(rows.iter().all(|(_, enabled)| *enabled));
        assert!(rows[0].0.contains("Plainsborn"));
        assert!(rows[2].0.contains("+20% creature gold"));
    }

    #[test]
    fn dragon_point_menu_offers_the_four_boons() {
        let rows = dragon_point_menu();
        assert_eq!(rows.len(), 4);
        assert!(rows.iter().all(|(_, enabled)| *enabled));
        assert!(rows[0].0.contains("max hitpoints"));
        assert!(rows[1].0.contains("forest fight"));
    }
}
