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
use super::commentary::{self, CommentLine, CommentRoom};
use super::data;
use super::events::{self, ForestEvent};
use super::inn;
use super::model::{self, Character, DragonPointKind, ForestHunt, Race, SlainFoe, Specialty};
use super::specialty::{self, SkillEffect};
use super::svc::{
    BountyBoardLoad, BountyPlace, CharacterLoad, CommentaryLoad, FiveSixLoad, GreenDragonService,
    HauntLoad, NewsLoad, PvpEngage, PvpSettle, PvpTarget, RosterEntry, RosterLoad,
};
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
    /// A commentary room (the shared chat primitive, `lib/commentary.php`):
    /// which room decides the section, verb, window, and the way back.
    Commentary(CommentRoom),
    /// The warrior list (`list.php`): who's online, the full roll of the
    /// realm, and the name search. The slice shown lives in [`RosterView`].
    WarriorList,
    /// The Hall of Fame (`hof.php`): seven stat rankings, each pageable and
    /// flippable best/worst.
    HallOfFame,
    /// The prize of a successful barkeep bribe (`inn_bartender.php`'s
    /// unlocked navs): the who's-upstairs list and the specialty switch.
    BarkeepEar,
    /// The PvP target list (`pvp.php` / the barkeep's keys): sleeping
    /// warriors at one of the two venues, each row an attack.
    PvpList(PvpVenue),
    /// The bounty broker's booth at the inn (`modules/dag.php`): the price
    /// on your own head, the wanted list, and placing contracts.
    DagTable,
    /// The wanted list: matured open bounties aggregated per head.
    BountyList,
    /// Picking a contract's target: a name typed on the talk line, then the
    /// matches as rows.
    BountyTarget,
    /// Naming a contract's price: the amount typed on the talk line.
    BountyAmount,
    /// The haunt (`case_haunt*.php`), reached from the graveyard at 25
    /// favor: a name typed on the talk line, then the matches as rows.
    Haunt,
}

/// A landed bounty-board read: the matured price on the player's own head
/// and the wanted aggregates (per-target matured gold), joined against the
/// roster snapshot at render time.
struct BountyBoard {
    on_my_head: u64,
    wanted: std::sync::Arc<Vec<(Uuid, u64)>>,
}

/// Where sleeping warriors are hunted (`pvplist`'s location split): the
/// fields off the village square, or the inn's rooms through the barkeep.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PvpVenue {
    Fields,
    Inn,
}

/// Which slice of the warrior list is showing (`list.php`'s three queries).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RosterView {
    /// Warriors currently online (the default landing).
    Online,
    /// Every warrior of the realm, paged.
    All,
    /// Name-search results (the query lives in `State::roster_query`).
    Search,
}

/// A Hall of Fame ranking (`hof.php`'s `op` values).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HofRanking {
    Kills,
    Wealth,
    Gems,
    Charm,
    Toughness,
    Resurrections,
    /// Fastest dragon kill (ascending `bestdragonage`; "least" = slowest).
    Speed,
}

/// Every ranking, in `hof.php`'s nav order.
pub const HOF_RANKINGS: [HofRanking; 7] = [
    HofRanking::Kills,
    HofRanking::Wealth,
    HofRanking::Gems,
    HofRanking::Charm,
    HofRanking::Toughness,
    HofRanking::Resurrections,
    HofRanking::Speed,
];

impl HofRanking {
    /// The nav label (upstream's addnav names, lightly ours).
    fn label(self) -> &'static str {
        match self {
            HofRanking::Kills => "Dragon kills",
            HofRanking::Wealth => "Gold",
            HofRanking::Gems => "Gems",
            HofRanking::Charm => "Charm",
            HofRanking::Toughness => "Toughness",
            HofRanking::Resurrections => "Resurrections",
            HofRanking::Speed => "Dragon kill speed",
        }
    }
}

/// A built page of the warrior list or a Hall of Fame ranking: the heading,
/// the formatted rows, and any footer lines (the gold-fuzz note, your
/// percentile). Built once per action — the richest ranking re-fuzzes each
/// build, so it must not be recomputed per frame.
#[derive(Clone, Debug, Default)]
pub struct ListPage {
    pub heading: String,
    /// A column-header line, when the view has data columns.
    pub header: Option<String>,
    pub rows: Vec<String>,
    pub foot: Vec<String>,
    pub page: usize,
    pub pages: usize,
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
    StonesBet {
        like_pair: bool,
    },
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
    /// Another player's sleeping character (`pvp.php`): no fleeing, no
    /// specialty skills, no buffs or companions on either side (nothing
    /// stock sets `allowinpvp`) — only the inn's bodyguard. Settlement runs
    /// through `svc` against the victim's stored blob.
    Pvp,
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
    /// A larcenous type (`data::BANDIT_CREATURES`): may cut a heavy purse
    /// once per fight while it stands.
    pub bandit: bool,
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
    /// Gold a bandit foe cut from the purse this fight: recovered in full on
    /// victory (its corpse still holds it), gone on a flee. Also flags the
    /// once-per-fight cut as spent.
    pub stolen_gold: Option<u64>,
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
            stolen_gold: None,
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

/// Idle seconds between presence-heartbeat saves: comfortably under the
/// roster's 15-minute online window (`svc::ONLINE_WINDOW_SECS`), so a live
/// but idle session never drifts into looking asleep and PvP-attackable.
const HEARTBEAT_SECS: u64 = 240;

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
    /// The in-flight commentary page load or post, drained by [`State::tick`].
    commentary_rx: Option<tokio::sync::watch::Receiver<CommentaryLoad>>,
    /// The loaded commentary window for the open room, newest first.
    commentary_lines: Option<std::sync::Arc<Vec<CommentLine>>>,
    /// The talk line being typed, while composing a commentary post (or a
    /// warrior-list name search). `Some` routes all key bytes into the
    /// buffer instead of the menu.
    talk_input: Option<String>,
    /// The in-flight roster load (the warrior list / Hall of Fame source),
    /// drained by [`State::tick`].
    roster_rx: Option<tokio::sync::watch::Receiver<RosterLoad>>,
    /// The loaded roster snapshot, shared by both views.
    roster: Option<std::sync::Arc<Vec<RosterEntry>>>,
    /// Which slice of the warrior list is showing.
    roster_view: RosterView,
    /// The warrior list's current page (0-based).
    roster_page: usize,
    /// The last submitted name search.
    roster_query: String,
    /// The built warrior-list page (`None` while the roster loads).
    roster_page_view: Option<ListPage>,
    /// The PvP target rows built off the roster snapshot for the open
    /// [`Mode::PvpList`] venue: `(target, label, attackable)`.
    pvp_rows: Vec<(Uuid, String, bool)>,
    /// Sleepers at the *other* venue (the fields list hears "N sleeping at
    /// the inn", and vice versa — upstream's location counts).
    pvp_elsewhere: usize,
    /// An engage round-trip in flight (`setup_target`), drained by
    /// [`State::tick`]; the venue decides the fight's bodyguard and exits.
    pvp_engage_rx: Option<(PvpVenue, tokio::sync::watch::Receiver<PvpEngage>)>,
    /// The engaged sleeping defender while a PvP fight runs (and its venue):
    /// the settlement formulas read these engage-time snapshots.
    pvp_ctx: Option<(PvpVenue, PvpTarget)>,
    /// A won fight's settlement round-trip (the victim's gold re-read),
    /// drained by [`State::tick`] into the attacker's purse.
    pvp_settle_rx: Option<tokio::sync::watch::Receiver<PvpSettle>>,
    /// The in-flight bounty-board read (your head + the wanted list),
    /// drained by [`State::tick`].
    bounty_board_rx: Option<tokio::sync::watch::Receiver<BountyBoardLoad>>,
    /// The loaded bounty board: the matured price on your own head and the
    /// wanted aggregates, joined against the roster at render.
    bounty_board: Option<BountyBoard>,
    /// Wanted-list ordering: gold desc when true, level desc otherwise
    /// (upstream's two sort links; level is the default).
    bounty_by_gold: bool,
    /// The wanted list's current page (0-based).
    bounty_page: usize,
    /// The built wanted-list page (`None` until board + roster both land).
    bounty_page_view: Option<ListPage>,
    /// Search matches while picking a contract's target:
    /// `(target, label, contractable)`.
    bounty_matches: Vec<(Uuid, String, bool)>,
    /// The picked contract target while naming the price:
    /// `(target, level, name)`.
    bounty_target: Option<(Uuid, u8, String)>,
    /// An in-flight bounty placement: the quoted fee'd cost already taken
    /// off the purse (refunded on refusal), and the round-trip.
    bounty_place_rx: Option<(u64, tokio::sync::watch::Receiver<BountyPlace>)>,
    /// Search matches on the haunt screen: `(target, label)`.
    haunt_matches: Vec<(Uuid, String)>,
    /// An in-flight haunt attempt, drained by [`State::tick`].
    haunt_rx: Option<tokio::sync::watch::Receiver<HauntLoad>>,
    /// Last persisted moment, for the idle presence heartbeat: a live
    /// session must never fall out of the roster's 15-minute online window,
    /// or it would look attackable (upstream's `laston` refreshes every
    /// page load; ours refreshes on action, so idling needs the heartbeat).
    last_save: std::time::Instant,
    /// The Hall of Fame's current ranking, order flip, and page.
    hof_ranking: HofRanking,
    hof_least: bool,
    hof_page: usize,
    /// The built Hall of Fame page (`None` while the roster loads).
    hof_page_view: Option<ListPage>,
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
            commentary_rx: None,
            commentary_lines: None,
            talk_input: None,
            roster_rx: None,
            roster: None,
            roster_view: RosterView::Online,
            roster_page: 0,
            roster_query: String::new(),
            roster_page_view: None,
            pvp_rows: Vec::new(),
            pvp_elsewhere: 0,
            pvp_engage_rx: None,
            pvp_ctx: None,
            pvp_settle_rx: None,
            bounty_board_rx: None,
            bounty_board: None,
            bounty_by_gold: false,
            bounty_page: 0,
            bounty_page_view: None,
            bounty_matches: Vec::new(),
            bounty_target: None,
            bounty_place_rx: None,
            haunt_matches: Vec::new(),
            haunt_rx: None,
            last_save: std::time::Instant::now(),
            hof_ranking: HofRanking::Kills,
            hof_least: false,
            hof_page: 0,
            hof_page_view: None,
        }
    }

    /// Drain pending async loads (the initial character, a news page). Called
    /// every app tick.
    pub fn tick(&mut self) {
        self.tick_news();
        self.tick_tavern();
        self.tick_commentary();
        self.tick_roster();
        self.tick_pvp();
        self.tick_bounty();
        self.tick_haunt();
        if self.character.is_some() {
            // The presence heartbeat: an idle session re-stamps its save
            // well inside the roster's 15-minute online window, so a live
            // player can never be mistaken for a sleeping PvP target
            // (upstream's `laston` refreshes with every page load).
            if self.last_save.elapsed().as_secs() > HEARTBEAT_SECS {
                self.save();
            }
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
            // What happened while you slept (upstream's system mail):
            // settlement reports left by other players' fights, drained into
            // the log once and cleared.
            let reports = std::mem::take(&mut character.pvp_reports);
            self.character = Some(character);
            self.cursor = 0;
            if !reports.is_empty() {
                for report in reports {
                    self.push_log(report);
                }
                // Persist the drain so the reports don't replay next entry.
                self.save();
            }
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
            Mode::Fight => fight_menu(
                c,
                self.encounter
                    .as_ref()
                    .map(|e| e.kind)
                    .unwrap_or(FoeKind::Creature),
            ),
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
            Mode::Tavern => tavern_menu(
                c,
                self.tavern_view,
                self.fivesix_pot,
                self.fivesix_rx.is_some(),
            ),
            Mode::Commentary(room) => commentary_menu(room, self.commentary_posts_left()),
            Mode::WarriorList => warrior_list_menu(self.roster_page_view.as_ref()),
            Mode::HallOfFame => hall_of_fame_menu(
                self.hof_ranking,
                self.hof_least,
                self.hof_page_view.as_ref(),
            ),
            Mode::BarkeepEar => barkeep_ear_menu(),
            Mode::PvpList(_) => self.pvp_list_menu(c),
            Mode::DagTable => self.dag_table_menu(c),
            Mode::BountyList => self.bounty_list_menu(),
            Mode::BountyTarget => self.bounty_target_menu(),
            Mode::BountyAmount => self.bounty_amount_menu(),
            Mode::Haunt => self.haunt_menu(c),
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
            Mode::Commentary(room) => self.select_commentary(room),
            Mode::WarriorList => self.select_warrior_list(),
            Mode::HallOfFame => self.select_hall_of_fame(),
            Mode::BarkeepEar => self.select_barkeep_ear(),
            Mode::PvpList(venue) => self.select_pvp_list(venue),
            Mode::DagTable => self.select_dag_table(),
            Mode::BountyList => self.select_bounty_list(),
            Mode::BountyTarget => self.select_bounty_target(),
            Mode::BountyAmount => self.select_bounty_amount(),
            Mode::Haunt => self.select_haunt(),
            Mode::Loading => Selection::Stay,
        }
    }

    /// Back out one level: leaf screens return to the village; the village
    /// leaves the game.
    pub fn back(&mut self) -> Selection {
        match self.mode {
            Mode::Village | Mode::Loading => Selection::Leave,
            // Esc during a fight attempts to flee (a 1-in-3 roll, like the
            // Flee row). Leaving mid-fight is never free — and there is no
            // running from a warrior you chose to attack (`pvp.php` turns
            // `run` into a fought round: "your pride prevents you").
            Mode::Fight => {
                if self
                    .encounter
                    .as_ref()
                    .is_some_and(|e| e.kind == FoeKind::Pvp)
                {
                    self.push_log("Your pride will not let you run from this.".into());
                    self.attack_round();
                } else {
                    self.attempt_flee();
                }
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
            // Esc while composing a talk line drops the line; otherwise the
            // room lets out wherever it was entered from.
            Mode::Commentary(room) => {
                if self.talk_input.is_some() {
                    self.talk_input = None;
                } else {
                    self.leave_commentary(room);
                }
                Selection::Stay
            }
            // The lists let out into the village square, dropping the
            // roster snapshot on the way.
            Mode::WarriorList | Mode::HallOfFame => {
                self.close_roster();
                Selection::Stay
            }
            // The barkeep's quiet word lets out into the common room; the
            // target lists let out where they were opened.
            Mode::BarkeepEar => {
                self.goto(Mode::Inn);
                Selection::Stay
            }
            Mode::PvpList(venue) => {
                self.goto(match venue {
                    PvpVenue::Fields => Mode::Village,
                    PvpVenue::Inn => Mode::BarkeepEar,
                });
                Selection::Stay
            }
            // The broker's booth lets out into the common room; its inner
            // screens back out to the booth (Esc while typing is handled
            // upstream of this, dropping the line first).
            Mode::DagTable => {
                self.leave_dag_table();
                Selection::Stay
            }
            Mode::BountyList | Mode::BountyTarget | Mode::BountyAmount => {
                self.goto(Mode::DagTable);
                Selection::Stay
            }
            // The haunt search lets out among the graves.
            Mode::Haunt => {
                self.goto(Mode::Graveyard);
                Selection::Stay
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
            s if s.starts_with("The Town Square") => self.open_commentary(CommentRoom::Village),
            s if s.starts_with("The Gardens") => self.open_commentary(CommentRoom::Gardens),
            s if s.starts_with("A weathered standing stone") => {
                // The veterans' rock (`rock.php`): dragon-killers see the
                // door; everyone else sees a rock and gets bored of it.
                if self.character.as_ref().unwrap().dragon_kills > 0 {
                    self.open_commentary(CommentRoom::Veterans);
                } else {
                    self.push_log(
                        "You circle the old stone. It stays a stone. Bored, you walk on.".into(),
                    );
                }
            }
            s if s.starts_with("The Gypsy's Tent") => {
                // The seance is pay-per-visit (`gypsy.php`, `level * 20`).
                let c = self.character.as_mut().unwrap();
                let cost = c.gypsy_cost();
                if c.gold >= cost {
                    c.gold -= cost;
                    self.save();
                    self.push_log(format!(
                        "The seer pockets your {cost} gold and the crystal clouds over."
                    ));
                    self.open_commentary(CommentRoom::ShadeGypsy);
                } else {
                    self.push_log(
                        "The seer counts your coin and sniffs. The dead don't rise for so little."
                            .into(),
                    );
                }
            }
            s if s.starts_with("The Daily News") => self.open_news(0),
            s if s.starts_with("List Warriors") => self.open_warrior_list(),
            s if s.starts_with("The Hall of Fame") => self.open_hall_of_fame(),
            s if s.starts_with("Slay Other Warriors") => {
                // The immunity warning greets the still-protected at the door
                // (`pvpwarning()` without the kill).
                if self.character.as_ref().unwrap().pvp_immune() {
                    self.push_log(
                        "You are yet under the realm's protection from other warriors - attack one and it ends forever."
                            .into(),
                    );
                }
                self.open_pvp_list(PvpVenue::Fields);
            }
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
                bandit: data::BANDIT_CREATURES.contains(&name),
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
            stolen_gold: None,
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
        // Buffs can't follow beyond the grave (Torment) or into PvP —
        // nothing stock sets `allowinpvp`, so `suspend_buffs` shelves them
        // all; the mount stays stabled too.
        if matches!(enc.kind, FoeKind::Torment | FoeKind::Pvp) {
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
        // Torment and PvP fights never injected them (see above): writing
        // back would wrongly expire every shelved buff.
        if matches!(enc.kind, FoeKind::Torment | FoeKind::Pvp) {
            return;
        }
        let c = self.character.as_mut().unwrap();
        c.persistent_buffs
            .retain_mut(|pb| match enc.buffs.iter().find(|b| b.name == pb.name) {
                Some(b) if b.rounds_left > 0 => {
                    pb.rounds_left = b.rounds_left;
                    true
                }
                _ => false,
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

    // --- commentary (the shared chat rooms) ----------------------------------

    /// Open a commentary room and kick off its page load.
    fn open_commentary(&mut self, room: CommentRoom) {
        self.commentary_lines = None;
        self.talk_input = None;
        self.commentary_rx = Some(
            self.svc
                .load_commentary(room.section(), room.display_limit()),
        );
        self.goto(Mode::Commentary(room));
    }

    /// Drain a finished commentary load (or post round-trip) into the view.
    fn tick_commentary(&mut self) {
        let Some(rx) = self.commentary_rx.as_mut() else {
            return;
        };
        let ready = match &*rx.borrow_and_update() {
            CommentaryLoad::Ready { lines, double_post } => Some((lines.clone(), *double_post)),
            CommentaryLoad::Loading => None,
        };
        if let Some((lines, double_post)) = ready {
            self.commentary_lines = Some(lines);
            self.commentary_rx = None;
            if double_post {
                self.push_log("You have already said exactly that. The room lets it pass.".into());
            }
        }
    }

    /// Put the room away and step back to wherever it was entered from.
    fn leave_commentary(&mut self, room: CommentRoom) {
        self.commentary_rx = None;
        self.commentary_lines = None;
        self.talk_input = None;
        match room {
            CommentRoom::Inn => self.goto(Mode::Inn),
            CommentRoom::DarkHorse => {
                self.tavern_view = TavernView::Hub;
                self.goto(Mode::Tavern);
            }
            CommentRoom::ShadeGrave => self.goto(Mode::Graveyard),
            _ => self.goto(Mode::Village),
        }
    }

    /// Posts the player may still make in the open room; `None` while the
    /// page (which the count is read off) is still loading.
    fn commentary_posts_left(&self) -> Option<usize> {
        let Mode::Commentary(room) = self.mode else {
            return None;
        };
        let lines = self.commentary_lines.as_ref()?;
        Some(commentary::posts_left(lines, self.user_id, room))
    }

    /// Handle an open room's rows: speak, listen afresh, leave.
    fn select_commentary(&mut self, room: CommentRoom) -> Selection {
        match self.cursor {
            0 => {
                if self.commentary_posts_left().unwrap_or(0) > 0 {
                    self.talk_input = Some(String::new());
                }
            }
            1 => self.open_commentary(room),
            _ => self.leave_commentary(room),
        }
        Selection::Stay
    }

    /// The loaded commentary window, newest first (`None` while loading).
    pub fn commentary_page(&self) -> Option<&[CommentLine]> {
        self.commentary_lines.as_deref().map(Vec::as_slice)
    }

    /// Whether a talk line is being composed: all key bytes go to the buffer.
    pub fn is_typing(&self) -> bool {
        self.talk_input.is_some()
    }

    /// The talk line as typed so far, while composing.
    pub fn talk_line(&self) -> Option<&str> {
        self.talk_input.as_deref()
    }

    /// Feed one printable character into the talk line, up to the venue's
    /// budget: a commentary post gets 200 less the room verb's baked emote
    /// overhead (as upstream), a warrior-name search a name's worth.
    pub fn talk_push(&mut self, ch: char) {
        let budget = match self.mode {
            Mode::Commentary(room) => commentary::max_post_len(room.verb()),
            Mode::WarriorList | Mode::BountyTarget | Mode::Haunt => SEARCH_QUERY_BUDGET,
            // Gold amounts only: digits, capped well under any purse.
            Mode::BountyAmount => {
                if !ch.is_ascii_digit() {
                    return;
                }
                AMOUNT_QUERY_BUDGET
            }
            _ => return,
        };
        if let Some(buf) = self.talk_input.as_mut()
            && buf.chars().count() < budget
        {
            buf.push(ch);
        }
    }

    /// Erase the last typed character.
    pub fn talk_backspace(&mut self) {
        if let Some(buf) = self.talk_input.as_mut() {
            buf.pop();
        }
    }

    /// Drop the talk line without posting.
    pub fn talk_cancel(&mut self) {
        self.talk_input = None;
    }

    /// Submit the talk line. In a commentary room: prepare the post (trim,
    /// run breaks, verb baking, the silence rejection) and send it off; the
    /// refreshed page comes back through the same channel as a plain load.
    /// On the warrior list: run the name search.
    pub fn talk_submit(&mut self) {
        if self.mode == Mode::WarriorList {
            let query = self.talk_input.take().unwrap_or_default();
            let query = query.trim().to_string();
            if query.is_empty() {
                self.push_log("You ask after no one in particular.".into());
            } else {
                self.roster_query = query;
                self.roster_view = RosterView::Search;
                self.roster_page = 0;
                self.rebuild_roster_views();
            }
            return;
        }
        if self.mode == Mode::BountyTarget {
            self.submit_bounty_search();
            return;
        }
        if self.mode == Mode::BountyAmount {
            self.submit_bounty_amount();
            return;
        }
        if self.mode == Mode::Haunt {
            self.submit_haunt_search();
            return;
        }
        let Mode::Commentary(room) = self.mode else {
            self.talk_input = None;
            return;
        };
        let Some(raw) = self.talk_input.take() else {
            return;
        };
        match commentary::prepare_post(&raw, room.verb()) {
            Some(body) => {
                let name = self
                    .character
                    .as_ref()
                    .map(|c| c.name.clone())
                    .unwrap_or_default();
                self.commentary_rx = Some(self.svc.post_commentary(
                    room.section(),
                    room.display_limit(),
                    self.user_id,
                    name,
                    body,
                ));
            }
            None => self.push_log("You open your mouth, then think better of it.".into()),
        }
    }

    // --- the warrior list + Hall of Fame --------------------------------------

    /// Open the warrior list on the online slice (`list.php`'s default view),
    /// kicking off a fresh roster load.
    fn open_warrior_list(&mut self) {
        self.roster_view = RosterView::Online;
        self.roster_page = 0;
        self.kick_roster_load();
        self.goto(Mode::WarriorList);
    }

    /// Open the Hall of Fame on dragon kills (`hof.php`'s default `op`).
    fn open_hall_of_fame(&mut self) {
        self.hof_ranking = HofRanking::Kills;
        self.hof_least = false;
        self.hof_page = 0;
        self.kick_roster_load();
        self.goto(Mode::HallOfFame);
    }

    /// Start (or restart) the roster load; [`State::tick`] lands it and
    /// builds the open view's page.
    fn kick_roster_load(&mut self) {
        self.roster = None;
        self.roster_page_view = None;
        self.hof_page_view = None;
        self.roster_rx = Some(self.svc.load_roster());
    }

    /// Drain a finished roster load and build the open view's page.
    fn tick_roster(&mut self) {
        let Some(rx) = self.roster_rx.as_mut() else {
            return;
        };
        let ready = match &*rx.borrow_and_update() {
            RosterLoad::Ready(entries) => Some(entries.clone()),
            RosterLoad::Loading => None,
        };
        if let Some(entries) = ready {
            self.roster = Some(entries);
            self.roster_rx = None;
            self.rebuild_roster_views();
        }
    }

    /// Rebuild the open view's page off the roster snapshot. Called on every
    /// view/page/ranking change — never per frame: the richest ranking rolls
    /// a fresh ±5% fuzz per build (upstream re-fuzzes per page load).
    fn rebuild_roster_views(&mut self) {
        let Some(roster) = self.roster.as_ref() else {
            return;
        };
        match self.mode {
            Mode::WarriorList => {
                self.roster_page_view = Some(build_warrior_page(
                    roster,
                    self.roster_view,
                    &self.roster_query,
                    self.roster_page,
                ));
            }
            Mode::HallOfFame => {
                if let Some(c) = self.character.as_ref() {
                    self.hof_page_view = Some(build_hof_page(
                        roster,
                        c,
                        self.user_id,
                        self.hof_ranking,
                        self.hof_least,
                        self.hof_page,
                        &mut rand::thread_rng(),
                    ));
                }
            }
            Mode::PvpList(venue) => self.rebuild_pvp_rows(venue),
            // The wanted list joins the roster; the booth and target picker
            // just need it present (their menus re-read `self.roster`).
            Mode::DagTable | Mode::BountyList => self.rebuild_bounty_page(),
            _ => {}
        }
    }

    // --- PvP ("slay other warriors", pvp.php + lib/pvplist.php) -------------

    /// Open a target list: kick a fresh roster read (the presence and
    /// dogpile columns go stale fast) and show the venue's sleepers.
    fn open_pvp_list(&mut self, venue: PvpVenue) {
        self.pvp_rows.clear();
        self.pvp_elsewhere = 0;
        self.kick_roster_load();
        self.goto(Mode::PvpList(venue));
    }

    /// Build the open venue's target rows off the roster snapshot (see
    /// [`build_pvp_rows`]).
    fn rebuild_pvp_rows(&mut self, venue: PvpVenue) {
        let Some(roster) = self.roster.as_ref() else {
            return;
        };
        let Some(c) = self.character.as_ref() else {
            return;
        };
        let (rows, elsewhere) = build_pvp_rows(
            roster,
            self.user_id,
            c.level,
            venue,
            chrono::Utc::now().timestamp(),
        );
        self.pvp_rows = rows;
        self.pvp_elsewhere = elsewhere;
    }

    /// Sleepers at the other venue, for the list panel's rumor line.
    pub fn pvp_elsewhere(&self) -> usize {
        self.pvp_elsewhere
    }

    /// The target list's rows: one per sleeper (disabled while the roster
    /// loads, an engage is in flight, or the day's attacks are spent), then
    /// the refresh and the way out.
    fn pvp_list_menu(&self, c: &Character) -> Vec<(String, bool)> {
        let can_attack = c.player_fights > 0 && self.pvp_engage_rx.is_none();
        let mut rows: Vec<(String, bool)> = self
            .pvp_rows
            .iter()
            .map(|(_, label, attackable)| (format!("Attack {label}"), *attackable && can_attack))
            .collect();
        if rows.is_empty() {
            let note = if self.roster.is_none() {
                "You listen for snoring..."
            } else {
                "No one worth attacking sleeps here tonight."
            };
            rows.push((note.into(), false));
        }
        rows.push(("Listen again for sleepers".into(), self.roster.is_some()));
        rows.push(("Think better of it".into(), true));
        rows
    }

    /// Handle a target-list row: an attack engages through `svc` (the fresh
    /// re-check + dogpile stamp); the tail rows refresh and leave.
    fn select_pvp_list(&mut self, venue: PvpVenue) -> Selection {
        let targets = self.pvp_rows.len().max(1);
        if self.cursor >= targets {
            match self.cursor - targets {
                0 => {
                    self.pvp_rows.clear();
                    self.kick_roster_load();
                }
                _ => {
                    return self.back();
                }
            }
            return Selection::Stay;
        }
        let Some(&(target_id, _, _)) = self.pvp_rows.get(self.cursor) else {
            return Selection::Stay;
        };
        let c = self.character.as_ref().unwrap();
        if c.player_fights == 0 {
            self.push_log("You are too weary to stalk anyone else today.".into());
            return Selection::Stay;
        }
        let rx = self.svc.pvp_engage(c.level, target_id);
        self.pvp_engage_rx = Some((venue, rx));
        self.push_log("You creep closer through the dark...".into());
        Selection::Stay
    }

    /// Drain the PvP round-trips: a finished engage starts the fight; a
    /// finished victory settlement pays the spoils.
    fn tick_pvp(&mut self) {
        if let Some((venue, rx)) = self.pvp_engage_rx.as_mut() {
            let venue = *venue;
            let engaged = match &*rx.borrow_and_update() {
                PvpEngage::Loading => None,
                PvpEngage::Ready(target) => Some(Ok((*target).clone())),
                PvpEngage::Refused(msg) => Some(Err(msg.clone())),
            };
            match engaged {
                None => {}
                Some(Err(msg)) => {
                    self.pvp_engage_rx = None;
                    self.push_log(msg);
                    // The list is stale (someone moved); re-read it.
                    if matches!(self.mode, Mode::PvpList(_)) {
                        self.pvp_rows.clear();
                        self.kick_roster_load();
                    }
                }
                Some(Ok(target)) => {
                    self.pvp_engage_rx = None;
                    // Only draw steel if the player is still at the list: if
                    // they wandered off mid-engage, the fight never starts
                    // (the target keeps the 10-minute flag; no attack is
                    // spent — like closing the browser on upstream's setup).
                    if matches!(self.mode, Mode::PvpList(_)) && self.encounter.is_none() {
                        self.start_pvp_fight(venue, *target);
                    }
                }
            }
        }
        if let Some(rx) = self.pvp_settle_rx.as_mut() {
            let settled = match &*rx.borrow_and_update() {
                PvpSettle::Loading => None,
                PvpSettle::Ready {
                    win_gold,
                    taken_gold: _,
                    bounty_gold,
                    forfeited,
                    victim,
                } => Some(Some((*win_gold, *bounty_gold, *forfeited, victim.clone()))),
                PvpSettle::Failed => Some(None),
            };
            match settled {
                None => {}
                Some(None) => {
                    self.pvp_settle_rx = None;
                    self.push_log("Their purse slips through your fingers in the dark.".into());
                }
                Some(Some((win_gold, bounty_gold, forfeited, victim))) => {
                    self.pvp_settle_rx = None;
                    let c = self.character.as_mut().unwrap();
                    // The level-15 "no prowess" rule zeroes the attacker's
                    // spoils (the victim's losses stand regardless).
                    if c.level as u32 >= data::MAX_LEVEL as u32 {
                        self.push_log(
                            "At your prowess, the victory itself is the only prize worth having."
                                .into(),
                        );
                    } else {
                        c.gold = c.gold.saturating_add(win_gold);
                        self.push_log(format!("You rifle their purse: +{win_gold} gold."));
                    }
                    // The bounty sweep pays on top, and — unlike the purse —
                    // even at level 15 (`dag`'s hook runs after the zeroing).
                    if bounty_gold > 0 {
                        let c = self.character.as_mut().unwrap();
                        c.gold = c.gold.saturating_add(bounty_gold);
                        self.push_log(format!(
                            "{} appears at your shoulder with a clinking purse: \
                             the {bounty_gold} gold bounty on {victim}'s head.",
                            data::BOUNTY_BROKER
                        ));
                        let who = self.character.as_ref().unwrap().titled_name();
                        self.news(format!(
                            "{who} collected the {bounty_gold} gold bounty on {victim}'s head!"
                        ));
                    }
                    if forfeited > 0 {
                        self.push_log(format!(
                            "\"The {forfeited} gold you posted yourself, I'll be \
                             keeping,\" {} adds, and is gone.",
                            data::BOUNTY_BROKER
                        ));
                    }
                    self.save();
                }
            }
        }
    }

    /// Begin the fight against an engaged sleeper: spend the day's attack,
    /// forfeit newbie immunity if this is the betrayal that ends it
    /// (`pvpwarning(true)`), then face their stored stats at full health.
    /// No buffs or companions follow either side; an inn target's bodyguard
    /// (they always bought the room) tilts the odds their way; a coin flip
    /// can hand the waking sleeper the first blow (`battle.php`'s surprise).
    fn start_pvp_fight(&mut self, venue: PvpVenue, target: PvpTarget) {
        let c = self.character.as_mut().unwrap();
        if c.player_fights == 0 {
            return;
        }
        c.player_fights -= 1;
        if c.pvp_immune() {
            c.pk = true;
            self.push_log(
                "You were still under the realm's protection - attacking ends it forever.".into(),
            );
        }
        let foe = Foe {
            name: target.name.clone(),
            weapon: target.weapon.to_string(),
            combatant: Combatant {
                attack: target.attack,
                defense: target.defense,
            },
            hp: target.max_hp,
            max_hp: target.max_hp,
            reward_gold: 0,
            reward_exp: 0,
            level: target.level,
            bandit: false,
        };
        let mut enc = Encounter::single(foe, FoeKind::Pvp);
        if venue == PvpVenue::Inn {
            // The room they bought comes with a light sleeper in the hall
            // (`apply_bodyguard(1)`): their arm swings harder, yours guards
            // worse, for the whole fight.
            let mut buff = Buff::new("Bodyguard", u32::MAX);
            buff.enemy_atk_mod = 1.05;
            buff.player_def_mod = 0.95;
            enc.buffs.push(buff);
        }
        let name = target.name.clone();
        self.pvp_ctx = Some((venue, target));
        self.push_log(format!(
            "You find {name} asleep and draw your blade over them."
        ));
        self.encounter = Some(enc);
        self.goto(Mode::Fight);
        self.save();
        // The sleeper may wake swinging: a coin flip for the first round
        // (`battle.php` rolls surprise 50/50 for single-foe fights).
        if rand::thread_rng().gen_range(0..2) == 0 {
            self.push_log(format!("{name}'s eyes snap open - they strike first!"));
            let Some(mut enc) = self.encounter.take() else {
                return;
            };
            self.foes_strike(&mut enc, None);
            if self.character.as_ref().unwrap().hitpoints == 0 {
                self.defeat(&enc);
                return;
            }
            self.encounter = Some(enc);
        }
    }

    // --- the bounty broker's booth (modules/dag.php) -------------------------

    /// Approach the broker: kick the board read (your head + the wanted
    /// aggregates) and a roster read (the list's columns and the target
    /// search both come off it).
    fn open_dag_table(&mut self) {
        self.bounty_board = None;
        self.bounty_page_view = None;
        self.bounty_board_rx = Some(self.svc.load_bounty_board(self.user_id));
        self.kick_roster_load();
        self.goto(Mode::DagTable);
    }

    /// Leave the booth for the common room, dropping the board and roster.
    fn leave_dag_table(&mut self) {
        self.bounty_board = None;
        self.bounty_board_rx = None;
        self.bounty_page_view = None;
        self.bounty_matches.clear();
        self.bounty_target = None;
        self.talk_input = None;
        self.roster_rx = None;
        self.roster = None;
        self.goto(Mode::Inn);
    }

    /// The price on your own head, once the board has landed (the broker's
    /// greeting; `None` renders as him still sizing you up).
    pub fn bounty_on_my_head(&self) -> Option<u64> {
        self.bounty_board.as_ref().map(|b| b.on_my_head)
    }

    /// The built wanted-list page (`None` until the board and roster land).
    pub fn bounty_page_view(&self) -> Option<&ListPage> {
        self.bounty_page_view.as_ref()
    }

    fn dag_table_menu(&self, c: &Character) -> Vec<(String, bool)> {
        let ready = self.bounty_board.is_some() && self.roster.is_some();
        let left = model::BOUNTIES_PER_DAY.saturating_sub(c.bounties_set_today);
        vec![
            ("Study the wanted list".into(), ready),
            (
                format!(
                    "Put a price on a head ({left} contract{} left today)",
                    if left == 1 { "" } else { "s" }
                ),
                ready && left > 0 && self.bounty_place_rx.is_none(),
            ),
            (format!("Leave {} to his pipe", data::BOUNTY_BROKER), true),
        ]
    }

    fn select_dag_table(&mut self) -> Selection {
        match self.cursor {
            0 => {
                self.bounty_page = 0;
                self.rebuild_bounty_page();
                self.goto(Mode::BountyList);
            }
            1 => {
                self.bounty_matches.clear();
                self.goto(Mode::BountyTarget);
                self.talk_input = Some(String::new());
            }
            _ => self.leave_dag_table(),
        }
        Selection::Stay
    }

    fn bounty_list_menu(&self) -> Vec<(String, bool)> {
        let pages = self.bounty_page_view.as_ref().map(|p| p.pages).unwrap_or(1);
        vec![
            (
                if self.bounty_by_gold {
                    "Order the list by level"
                } else {
                    "Order the list by gold"
                }
                .into(),
                self.bounty_page_view.is_some(),
            ),
            ("Turn the page".into(), pages > 1),
            ("Back to the booth".into(), true),
        ]
    }

    fn select_bounty_list(&mut self) -> Selection {
        match self.cursor {
            0 => {
                self.bounty_by_gold = !self.bounty_by_gold;
                self.bounty_page = 0;
                self.rebuild_bounty_page();
            }
            1 => {
                if let Some(p) = self.bounty_page_view.as_ref() {
                    self.bounty_page = (p.page + 1) % p.pages;
                    self.rebuild_bounty_page();
                }
            }
            _ => self.goto(Mode::DagTable),
        }
        Selection::Stay
    }

    /// (Re)build the wanted list once both the board aggregates and the
    /// roster snapshot are in; also called on sort/page changes.
    fn rebuild_bounty_page(&mut self) {
        let (Some(board), Some(roster)) = (self.bounty_board.as_ref(), self.roster.as_ref())
        else {
            return;
        };
        let page = build_bounty_page(&board.wanted, roster, self.bounty_by_gold, self.bounty_page);
        self.bounty_page = page.page;
        self.bounty_page_view = Some(page);
    }

    /// Run the typed name against the roster for a contract target
    /// (`dag.php`'s finalize search: subsequence match, >100 = narrow it
    /// down). Matches render as rows; the broker's refusals — yourself, the
    /// level floor, his one-notch-lenient immunity test — disable theirs.
    fn submit_bounty_search(&mut self) {
        let query = self.talk_input.take().unwrap_or_default();
        let query = query.trim().to_string();
        let Some(roster) = self.roster.as_ref() else {
            self.push_log(format!(
                "{} is still thumbing through his book; give him a moment.",
                data::BOUNTY_BROKER
            ));
            return;
        };
        if query.is_empty() {
            self.push_log(format!(
                "{} doesn't look up. \"A name first.\"",
                data::BOUNTY_BROKER
            ));
            return;
        }
        let mut matches: Vec<&RosterEntry> = roster
            .iter()
            .filter(|e| name_matches(&e.name, &query))
            .collect();
        if matches.is_empty() {
            self.push_log(format!(
                "{} shakes his head. \"Nobody I know answers to that name.\"",
                data::BOUNTY_BROKER
            ));
            return;
        }
        if matches.len() > MAX_SEARCH_MATCHES {
            self.push_log(format!(
                "{} snorts. \"That could be half the town. Narrow it down.\"",
                data::BOUNTY_BROKER
            ));
            return;
        }
        matches.sort_by(|a, b| {
            a.level
                .cmp(&b.level)
                .then_with(|| a.handle.to_lowercase().cmp(&b.handle.to_lowercase()))
        });
        let me = self.user_id;
        self.bounty_matches = matches
            .iter()
            .map(|e| {
                if e.user_id == me {
                    (
                        e.user_id,
                        format!("{} (level {}) - no contracts on yourself", e.name, e.level),
                        false,
                    )
                } else if e.level < model::BOUNTY_MIN_TARGET_LEVEL || e.bounty_immune {
                    (
                        e.user_id,
                        format!("{} (level {}) - not worth a contract", e.name, e.level),
                        false,
                    )
                } else {
                    (e.user_id, format!("{} (level {})", e.name, e.level), true)
                }
            })
            .collect();
    }

    fn bounty_target_menu(&self) -> Vec<(String, bool)> {
        let mut rows: Vec<(String, bool)> = self
            .bounty_matches
            .iter()
            .map(|(_, label, ok)| (label.clone(), *ok))
            .collect();
        if rows.is_empty() {
            let note = if self.roster.is_none() {
                "He waits while you gather the name..."
            } else {
                "Name a head and he'll check his book."
            };
            rows.push((note.into(), false));
        }
        rows.push(("Ask after another name".into(), self.roster.is_some()));
        rows.push(("Back to the booth".into(), true));
        rows
    }

    fn select_bounty_target(&mut self) -> Selection {
        let targets = self.bounty_matches.len().max(1);
        if self.cursor >= targets {
            match self.cursor - targets {
                0 => self.talk_input = Some(String::new()),
                _ => self.goto(Mode::DagTable),
            }
            return Selection::Stay;
        }
        let Some((target_id, _, _)) = self.bounty_matches.get(self.cursor) else {
            return Selection::Stay;
        };
        let target_id = *target_id;
        let Some(entry) = self
            .roster
            .as_ref()
            .and_then(|r| r.iter().find(|e| e.user_id == target_id).cloned())
        else {
            return Selection::Stay;
        };
        self.bounty_target = Some((entry.user_id, entry.level, entry.name.clone()));
        self.goto(Mode::BountyAmount);
        self.talk_input = Some(String::new());
        Selection::Stay
    }

    /// The picked contract target while naming the price, for the panel:
    /// `(name, level)`.
    pub fn bounty_target_info(&self) -> Option<(&str, u8)> {
        self.bounty_target
            .as_ref()
            .map(|(_, level, name)| (name.as_str(), *level))
    }

    fn bounty_amount_menu(&self) -> Vec<(String, bool)> {
        vec![
            (
                "Name your price".into(),
                self.bounty_place_rx.is_none() && self.bounty_target.is_some(),
            ),
            ("Think better of it".into(), true),
        ]
    }

    fn select_bounty_amount(&mut self) -> Selection {
        match self.cursor {
            0 => self.talk_input = Some(String::new()),
            _ => {
                self.bounty_target = None;
                self.goto(Mode::DagTable);
            }
        }
        Selection::Stay
    }

    /// Check and place the typed amount (`dag.php`'s finalize, upstream's
    /// order): the per-level minimum, the fee'd cost against the purse, then
    /// the open-total cap inside the placement transaction. The cost is
    /// taken up front and refunded on a refusal (upstream leaves the coins
    /// on the table; the net effect is identical).
    fn submit_bounty_amount(&mut self) {
        let raw = self.talk_input.take().unwrap_or_default();
        let Some((target_id, level, name)) = self.bounty_target.clone() else {
            self.goto(Mode::DagTable);
            return;
        };
        let amount: u64 = raw.trim().parse().unwrap_or(0);
        let min = model::BOUNTY_MIN_PER_LEVEL * level as u64;
        let cap = model::BOUNTY_MAX_PER_LEVEL * level as u64;
        if amount < min {
            self.push_log(format!(
                "{} scowls. \"{name}'s head is worth {min} gold to me at the least. \
                 Come back with real coin.\"",
                data::BOUNTY_BROKER
            ));
            return;
        }
        let cost = model::bounty_cost(amount);
        let c = self.character.as_mut().unwrap();
        if c.gold < cost {
            self.push_log(format!(
                "{} eyes your purse. \"That's {cost} gold with my listing fee, \
                 and you don't have it.\"",
                data::BOUNTY_BROKER
            ));
            return;
        }
        c.gold -= cost;
        self.save();
        self.bounty_place_rx = Some((
            cost,
            self.svc.place_bounty(self.user_id, target_id, amount, cap),
        ));
        self.push_log(format!(
            "{} counts your coins twice and thumbs through his book...",
            data::BOUNTY_BROKER
        ));
    }

    /// Drain the bounty round-trips: a landed board read builds the list; a
    /// landed placement charges (already-taken) or refunds.
    fn tick_bounty(&mut self) {
        if let Some(rx) = self.bounty_board_rx.as_mut() {
            let ready = match &*rx.borrow_and_update() {
                BountyBoardLoad::Loading => None,
                BountyBoardLoad::Ready { on_my_head, wanted } => Some(BountyBoard {
                    on_my_head: *on_my_head,
                    wanted: wanted.clone(),
                }),
            };
            if let Some(board) = ready {
                self.bounty_board = Some(board);
                self.bounty_board_rx = None;
                self.rebuild_bounty_page();
            }
        }
        if let Some((cost, rx)) = self.bounty_place_rx.as_mut() {
            let cost = *cost;
            let placed = match &*rx.borrow_and_update() {
                BountyPlace::Loading => None,
                BountyPlace::Placed => Some(Ok(())),
                BountyPlace::OverCap(current) => Some(Err(Some(*current))),
                BountyPlace::Failed => Some(Err(None)),
            };
            match placed {
                None => {}
                Some(Ok(())) => {
                    self.bounty_place_rx = None;
                    let target = self.bounty_target.take();
                    let c = self.character.as_mut().unwrap();
                    c.bounties_set_today += 1;
                    let name = target.map(|(_, _, n)| n).unwrap_or_default();
                    // No placement news: contracts are anonymous until
                    // collected (upstream's only tell is the collection item).
                    self.push_log(format!(
                        "{} palms the coins off the table. \"The word goes out \
                         on {name}. Be patient, and watch the news.\"",
                        data::BOUNTY_BROKER
                    ));
                    self.save();
                    // The totals moved; re-read the board for the booth.
                    self.bounty_board_rx = Some(self.svc.load_bounty_board(self.user_id));
                    if self.mode == Mode::BountyAmount {
                        self.goto(Mode::DagTable);
                    }
                }
                Some(Err(refusal)) => {
                    self.bounty_place_rx = None;
                    let c = self.character.as_mut().unwrap();
                    c.gold = c.gold.saturating_add(cost);
                    self.save();
                    match refusal {
                        Some(current) => {
                            let cap = self
                                .bounty_target
                                .as_ref()
                                .map(|(_, level, _)| model::BOUNTY_MAX_PER_LEVEL * *level as u64)
                                .unwrap_or(0);
                            self.push_log(format!(
                                "{} slides the coins back. \"That head already \
                                 carries {current} gold in contracts, and {cap} \
                                 is my ceiling. I'll not be called an assassin.\"",
                                data::BOUNTY_BROKER
                            ));
                        }
                        None => {
                            self.push_log(format!(
                                "{} slides the coins back. \"My book's gone \
                                 missing. Another time.\"",
                                data::BOUNTY_BROKER
                            ));
                        }
                    }
                }
            }
        }
    }

    // --- the haunt (lib/graveyard/case_haunt*.php) ----------------------------

    /// Open the haunt search off the favor menu: a fresh roster read and the
    /// talk line for the name.
    fn open_haunt(&mut self) {
        self.haunt_matches.clear();
        self.kick_roster_load();
        self.goto(Mode::Haunt);
        self.talk_input = Some(String::new());
    }

    /// Run the typed name against the roster (`case_haunt2.php`): the plain
    /// subsequence search, capped at 100 ("narrow down the number of people
    /// you wish to haunt"), sorted level then name (upstream `ORDER BY
    /// level,login`). No other filter — the dead, the brand-new, the
    /// PvP-immune, and even yourself all match, exactly as upstream; only
    /// "already haunted" refuses, at attempt time.
    fn submit_haunt_search(&mut self) {
        let query = self.talk_input.take().unwrap_or_default();
        let query = query.trim().to_string();
        let Some(roster) = self.roster.as_ref() else {
            self.push_log("The veil is still parting; whisper again in a moment.".into());
            return;
        };
        if query.is_empty() {
            self.push_log("The veil swallows your empty whisper.".into());
            return;
        }
        let mut matches: Vec<&RosterEntry> = roster
            .iter()
            .filter(|e| name_matches(&e.name, &query))
            .collect();
        if matches.is_empty() {
            self.push_log(format!(
                "{} could find no one who answers to that name.",
                data::DEATH_OVERLORD
            ));
            return;
        }
        if matches.len() > MAX_SEARCH_MATCHES {
            self.push_log(format!(
                "{} thinks you should narrow down whom you wish to haunt.",
                data::DEATH_OVERLORD
            ));
            return;
        }
        matches.sort_by(|a, b| {
            a.level
                .cmp(&b.level)
                .then_with(|| a.handle.to_lowercase().cmp(&b.handle.to_lowercase()))
        });
        self.haunt_matches = matches
            .iter()
            .map(|e| (e.user_id, format!("{} (level {})", e.name, e.level)))
            .collect();
    }

    fn haunt_menu(&self, c: &Character) -> Vec<(String, bool)> {
        let can = c.favor >= model::HAUNT_FAVOR_THRESHOLD && self.haunt_rx.is_none();
        let mut rows: Vec<(String, bool)> = self
            .haunt_matches
            .iter()
            .map(|(_, label)| (format!("Haunt {label}"), can))
            .collect();
        if rows.is_empty() {
            let note = if self.roster.is_none() {
                "You peer across the veil..."
            } else {
                "Whisper a name to seek them out."
            };
            rows.push((note.into(), false));
        }
        rows.push(("Whisper another name".into(), self.roster.is_some()));
        rows.push(("Back to the graves".into(), true));
        rows
    }

    fn select_haunt(&mut self) -> Selection {
        let targets = self.haunt_matches.len().max(1);
        if self.cursor >= targets {
            match self.cursor - targets {
                0 => self.talk_input = Some(String::new()),
                _ => self.goto(Mode::Graveyard),
            }
            return Selection::Stay;
        }
        let Some((target_id, _)) = self.haunt_matches.get(self.cursor) else {
            return Selection::Stay;
        };
        let target_id = *target_id;
        let c = self.character.as_ref().unwrap();
        self.haunt_rx = Some(self.svc.haunt(c.level, c.titled_name(), target_id));
        self.push_log("You gather your grave-chill and slip toward the mortal world...".into());
        Selection::Stay
    }

    /// Drain a finished haunt attempt: the rolled outcomes charge the 25
    /// favor (success and fumble alike — `case_haunt3.php` deducts before
    /// the roll) and make the news; refusals cost nothing.
    fn tick_haunt(&mut self) {
        let Some(rx) = self.haunt_rx.as_mut() else {
            return;
        };
        let outcome = match &*rx.borrow_and_update() {
            HauntLoad::Loading => None,
            outcome => Some(outcome.clone()),
        };
        let Some(outcome) = outcome else {
            return;
        };
        self.haunt_rx = None;
        let me = self.character.as_ref().unwrap().titled_name();
        match outcome {
            HauntLoad::Loading => {}
            HauntLoad::Success { target } => {
                let c = self.character.as_mut().unwrap();
                c.favor = c.favor.saturating_sub(model::HAUNT_FAVOR_THRESHOLD);
                self.push_log(format!(
                    "You pour through the veil and rake cold fingers through \
                     {target}'s dreams. They will wake the poorer for sleep."
                ));
                self.news(format!(
                    "{me} slipped through the veil and haunted {target}!"
                ));
                self.save();
            }
            HauntLoad::Fumble { target } => {
                let c = self.character.as_mut().unwrap();
                c.favor = c.favor.saturating_sub(model::HAUNT_FAVOR_THRESHOLD);
                let line = data::haunt_fumble(&mut rand::thread_rng(), &target);
                self.push_log(line);
                self.news(format!("{me} tried to haunt {target}, and botched it!"));
                self.save();
            }
            HauntLoad::AlreadyHaunted { target } => {
                self.push_log(format!(
                    "{} stays your hand: another shade already rides {target}'s dreams.",
                    data::DEATH_OVERLORD
                ));
            }
            HauntLoad::Gone => {
                self.push_log(format!(
                    "{} has lost his grip on that soul; you cannot haunt them now.",
                    data::DEATH_OVERLORD
                ));
            }
        }
    }

    /// The built warrior-list page (`None` while the roster loads).
    pub fn warrior_page(&self) -> Option<&ListPage> {
        self.roster_page_view.as_ref()
    }

    /// The built Hall of Fame page (`None` while the roster loads).
    pub fn hall_of_fame_page(&self) -> Option<&ListPage> {
        self.hof_page_view.as_ref()
    }

    /// Put the roster views away when stepping back to the village.
    fn close_roster(&mut self) {
        self.roster_rx = None;
        self.roster = None;
        self.roster_page_view = None;
        self.hof_page_view = None;
        self.talk_input = None;
        self.goto(Mode::Village);
    }

    /// Handle the warrior list's rows: search, the two slices, the pager.
    fn select_warrior_list(&mut self) -> Selection {
        match self.cursor {
            0 => self.talk_input = Some(String::new()),
            // "Here right now" re-reads the roster: presence is the one
            // column that goes stale while you stare at it.
            1 => {
                self.roster_view = RosterView::Online;
                self.roster_page = 0;
                self.kick_roster_load();
            }
            2 => {
                self.roster_view = RosterView::All;
                self.roster_page = 0;
                self.rebuild_roster_views();
            }
            3 => {
                self.roster_page += 1;
                self.rebuild_roster_views();
            }
            4 => {
                self.roster_page = self.roster_page.saturating_sub(1);
                self.rebuild_roster_views();
            }
            _ => self.close_roster(),
        }
        Selection::Stay
    }

    /// Handle the Hall of Fame's rows: rankings, the best/worst flip, the
    /// pager. A ranking switch resets the page and keeps the flip; the flip
    /// keeps the page — upstream's links do the same.
    fn select_hall_of_fame(&mut self) -> Selection {
        match self.cursor {
            i if i < HOF_RANKINGS.len() => {
                self.hof_ranking = HOF_RANKINGS[i];
                self.hof_page = 0;
                self.rebuild_roster_views();
            }
            i if i == HOF_RANKINGS.len() => {
                self.hof_least = !self.hof_least;
                self.rebuild_roster_views();
            }
            i if i == HOF_RANKINGS.len() + 1 => {
                self.hof_page += 1;
                self.rebuild_roster_views();
            }
            i if i == HOF_RANKINGS.len() + 2 => {
                self.hof_page = self.hof_page.saturating_sub(1);
                self.rebuild_roster_views();
            }
            _ => self.close_roster(),
        }
        Selection::Stay
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
        self.goto(if alive {
            Mode::Village
        } else {
            Mode::Graveyard
        });
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
                    // A haunt collects even on a bought dawn (`newday.php`'s
                    // block is unconditional).
                    if let Some(haunter) = fx.haunted_by.as_ref() {
                        self.push_log(format!(
                            "{haunter} haunted your brief death; the fright costs you a turn."
                        ));
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
            3 => self.open_haunt(),
            4 => self.open_commentary(CommentRoom::ShadeGrave),
            5 => return Selection::Leave,
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
                bandit: false,
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
        if c.seen_master_today {
            // One challenge per day (`train.php`'s `seenmaster` gate); only a
            // win reopens the yard before dawn.
            self.push_log(
                "You have tested your master's patience once today. Come back tomorrow.".into(),
            );
            return Selection::Stay;
        }
        if !c.can_challenge_master() {
            self.push_log("Your master shakes their head. Gain more experience first.".into());
            return Selection::Stay;
        }
        let Some((master, foe, hp)) = c.scaled_master(&mut rand::thread_rng()) else {
            return Selection::Stay;
        };
        // The challenge itself spends the day's audience, win or lose —
        // persisted now so a disconnect mid-fight can't refund it.
        self.character.as_mut().unwrap().seen_master_today = true;
        self.save();
        let c = self.character.as_ref().unwrap();
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
                bandit: false,
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
                bandit: false,
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
        // Against a sleeping warrior the menu is one row: Attack (no skills,
        // no flee — `pvp.php` strips both).
        if self
            .encounter
            .as_ref()
            .is_some_and(|e| e.kind == FoeKind::Pvp)
        {
            self.attack_round();
            return Selection::Stay;
        }
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
        self.bandit_purse_cut(&mut enc);
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

    /// A living bandit-type foe tries to cut a heavy purse: once per fight,
    /// only above the gold threshold, a 1-in-8 roll each round. The cut
    /// (20% of carried gold) rides the encounter — killing every foe wins it
    /// back off the corpse; fleeing forfeits it. An original late.sh
    /// mechanic (stock LoGD has no mid-fight steal); a death would zero the
    /// purse anyway.
    fn bandit_purse_cut(&mut self, enc: &mut Encounter) {
        if enc.stolen_gold.is_some() {
            return;
        }
        let Some(bandit) = enc
            .foes
            .iter()
            .find(|f| f.bandit && f.hp > 0)
            .map(|f| f.name.clone())
        else {
            return;
        };
        let c = self.character.as_ref().unwrap();
        if c.gold <= model::BANDIT_GOLD_THRESHOLD {
            return;
        }
        let mut rng = rand::thread_rng();
        if rng.gen_range(0..8) != 0 {
            return;
        }
        let cut = (self.character.as_ref().unwrap().gold as f64 * 0.20).round() as u64;
        let c = self.character.as_mut().unwrap();
        c.gold -= cut;
        enc.stolen_gold = Some(cut);
        self.push_log(format!(
            "{bandit} cuts your purse in the scuffle: {cut} gold gone! Kill it to take it back."
        ));
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
        // Companions sit PvP out entirely (`suspend_companions`: nothing
        // stock is `allowinpvp`) — no bandaging, no swings, no getting hit.
        let pvp = enc.kind == FoeKind::Pvp;
        if !pvp {
            // Field-medics bandage before the blades cross (upstream
            // activates `heal` first each round): the player first, then the
            // most wounded companion, then themselves. They still swing in
            // the resolver below.
            self.companion_heals(player_max);
        }
        // Companions live on the character and fight each round; the resolver
        // mutates their HP and removes any that fall. The player and their
        // companions all strike the current target.
        let mut benched = Vec::new();
        let outcome = {
            let c = self.character.as_mut().unwrap();
            resolve_round_buffed(
                &mut rng,
                player,
                enc.foes[target].combatant,
                &mut enc.buffs,
                if pvp { &mut benched } else { &mut c.companions },
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
        self.bandit_purse_cut(&mut enc);
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
                self.push_log(data::foe_dying_line(&mut rng).into());
                self.push_log(format!("Victory! +{} gold, +{} experience.", v.gold, v.exp));
                // A cut purse comes back off the bandit's corpse, whole.
                if let Some(cut) = enc.stolen_gold {
                    self.character.as_mut().unwrap().gold += cut;
                    self.push_log(format!(
                        "You pry your stolen {cut} gold back out of the bandit's coat."
                    ));
                }
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
            FoeKind::Pvp => {
                let Some((venue, target)) = self.pvp_ctx.take() else {
                    self.encounter = None;
                    self.goto(Mode::Village);
                    self.save();
                    return;
                };
                // Winning on your last breath: staunch the wounds at 1 HP
                // (`pvp.php`'s "bit of cloth", the mushroom save's cousin).
                if self.character.as_ref().unwrap().hitpoints == 0 {
                    self.character.as_mut().unwrap().hitpoints = 1;
                    self.push_log(
                        "You tear a strip from their bedding and staunch your own wounds just in time."
                            .into(),
                    );
                }
                let my_level = self.character.as_ref().unwrap().level;
                self.push_log(format!("You have slain {}!", target.name));
                // The experience settles locally off the engage snapshot; the
                // gold waits on the victim's purse re-read (`tick_pvp`). A
                // level-15 attacker takes nothing ("no prowess" rule).
                if my_level >= data::MAX_LEVEL {
                    self.push_log(
                        "At your prowess, the victory itself is the only prize worth having."
                            .into(),
                    );
                } else {
                    let (exp, bonus) =
                        model::pvp_attacker_exp(target.experience, target.level, my_level);
                    let c = self.character.as_mut().unwrap();
                    c.experience = c.experience.saturating_add(exp);
                    if bonus > 0 {
                        self.push_log(format!(
                            "A hard-won fight earns you {bonus} extra experience."
                        ));
                    } else if bonus < 0 {
                        self.push_log(format!(
                            "So easy a mark costs you {} experience in respect.",
                            -bonus
                        ));
                    }
                    self.push_log(format!("+{exp} experience."));
                }
                let who = self.character.as_ref().unwrap().titled_name();
                self.pvp_settle_rx = Some(self.svc.pvp_settle_victory(
                    target.user_id,
                    target.clone(),
                    self.user_id,
                    who.clone(),
                ));
                // Both outcomes make the paper (`pvp.php`'s two variants).
                if venue == PvpVenue::Inn {
                    self.news(format!(
                        "{who} crept into {}'s room at the inn and bested them in their sleep.",
                        target.name
                    ));
                } else {
                    self.news(format!(
                        "{who} bested {} in single combat in the fields.",
                        target.name
                    ));
                }
                self.encounter = None;
                self.goto(match venue {
                    PvpVenue::Inn => Mode::Inn,
                    PvpVenue::Fields => Mode::Village,
                });
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
                // Any price on the slayer's head dies with the old life:
                // open bounties close to the house (`dag`'s dragonkill hook).
                self.svc.close_bounties_on(self.user_id);
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
            FoeKind::Pvp => {
                // The sleeper wins (`pvpdefeat`): their spoils come off your
                // corpse — gold by the log formula read *before* the wipe,
                // experience at 10% (zeroed against a level-15 defender;
                // upstream's `$wonamount` typo leaves the gold flowing even
                // then, kept 1=1) — and you go to the graveyard poorer by
                // every coin and 15% of your experience.
                let Some((venue, target)) = self.pvp_ctx.take() else {
                    self.encounter = None;
                    self.goto(Mode::Graveyard);
                    self.save();
                    return;
                };
                let win_gold = model::pvp_win_gold(c.level, c.gold);
                let won_exp = if target.level >= data::MAX_LEVEL {
                    0
                } else {
                    (model::PVP_DEFENDER_GAIN_PCT as f64 * c.experience as f64 / 100.0).round()
                        as u64
                };
                c.pvp_die();
                self.svc.pvp_settle_defeat(
                    target.user_id,
                    target.level,
                    win_gold,
                    won_exp,
                    who.clone(),
                );
                self.push_log(format!(
                    "{} wakes at the last instant and cuts you down!",
                    target.name
                ));
                self.push_log(
                    "Every coin you carried is gone, and 15% of your experience with it.".into(),
                );
                if venue == PvpVenue::Inn {
                    self.news(format!(
                        "{who} was slain breaking into {}'s room at the inn. {taunt}",
                        target.name
                    ));
                } else {
                    self.news(format!(
                        "{who} was slain attacking {} as they slept in the fields. {taunt}",
                        target.name
                    ));
                }
                self.encounter = None;
                self.goto(Mode::Graveyard);
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
                if enc.kind == FoeKind::Creature {
                    self.push_log(data::foe_gloating_line(&mut rand::thread_rng()).into());
                }
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
            let traded_in = self
                .character
                .as_ref()
                .unwrap()
                .mount_data()
                .map(|m| m.name);
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
            5 => self.open_dag_table(),
            6 => self.open_commentary(CommentRoom::Inn),
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
                let source = if from_bank {
                    "the bank's ledger"
                } else {
                    "your purse"
                };
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
            self.goto(Mode::BarkeepEar);
        } else {
            self.push_log(format!(
                "{} makes the bribe disappear and suddenly remembers other customers.",
                data::BARKEEP
            ));
        }
        Selection::Stay
    }

    /// The bribed barkeep's quiet word (`inn_bartender.php`'s unlocked navs):
    /// the keys to the rooms upstairs, or the specialty switch.
    fn select_barkeep_ear(&mut self) -> Selection {
        match self.cursor {
            0 => {
                if self.character.as_ref().unwrap().pvp_immune() {
                    self.push_log(
                        "You are yet under the realm's protection from other warriors - attack one and it ends forever."
                            .into(),
                    );
                }
                self.open_pvp_list(PvpVenue::Inn);
            }
            1 => self.goto(Mode::SwitchSpecialty),
            _ => self.goto(Mode::Inn),
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
                self.push_log(
                    "You brave the public trench. It is exactly as bad as feared.".into(),
                );
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
                lines
                    .push("You scrub up at the rain barrel. Cleanliness is its own reward.".into());
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
                    lines.push(
                        "In your hurry you fumble a gold coin into the muck. It stays there."
                            .into(),
                    );
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
                3 => self.open_commentary(CommentRoom::DarkHorse),
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
                        game.roll,
                        game.tries,
                        tavern::DICE_MAX_ROLLS
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
                                data::GAMBLER,
                                game.roll,
                                game.bet
                            )
                        }
                        std::cmp::Ordering::Less => {
                            c.gold += game.bet;
                            format!(
                                "{} shows a {his} to your {}. He pays out {} gold, scowling.",
                                data::GAMBLER,
                                game.roll,
                                game.bet
                            )
                        }
                        std::cmp::Ordering::Equal => format!(
                            "{} shows a {his} to your {}. A push; the stakes go home.",
                            data::GAMBLER,
                            game.roll
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
                    if draw.yours {
                        "yours (+2 your pile)"
                    } else {
                        "his (+2 his pile)"
                    }
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
            self.last_save = std::time::Instant::now();
        }
    }

    /// Persist on the way out of the game (called from `leave`).
    pub fn save_on_leave(&self) {
        if let Some(c) = self.character.as_ref() {
            // The leave save drops the presence flag (upstream's logout
            // clearing `loggedin`), so the roster stops listing you as here.
            let mut c = c.clone();
            c.online = false;
            self.svc.save_character(self.user_id, &c);
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
            // Enter on banked experience alone; the yard itself delivers the
            // "seen enough of you today" refusal, like upstream's nav.
            c.level < data::MAX_LEVEL && c.experience >= c.exp_for_next_level(),
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
    rows.push(("The Town Square (gossip)".into(), true));
    rows.push(("The Gardens (a quiet walk)".into(), true));
    rows.push(("A weathered standing stone".into(), true));
    rows.push((
        format!("The Gypsy's Tent ({} gold)", c.gypsy_cost()),
        c.gold >= c.gypsy_cost(),
    ));
    rows.push(("The Daily News".into(), true));
    rows.push(("List Warriors".into(), true));
    rows.push(("The Hall of Fame".into(), true));
    rows.push((
        format!("Slay Other Warriors ({} left today)", c.player_fights),
        true,
    ));
    rows.push(("Leave the realm".into(), true));
    rows
}

/// The warrior list's rows (`list.php`'s navs): the name search, the two
/// slices, and the pager. Everything but the way back waits on the roster.
fn warrior_list_menu(page: Option<&ListPage>) -> Vec<(String, bool)> {
    let loaded = page.is_some();
    let (cur, pages) = page.map(|p| (p.page, p.pages)).unwrap_or((0, 0));
    vec![
        ("Ask after a warrior by name".into(), loaded),
        ("Warriors here right now".into(), true),
        ("All the warriors of the realm".into(), loaded),
        ("Next page".into(), loaded && cur + 1 < pages),
        ("Previous page".into(), loaded && cur > 0),
        ("Back to the village square".into(), true),
    ]
}

/// The Hall of Fame's rows (`hof.php`'s navs): every ranking, the best/worst
/// flip, and the pager. Each ranking keeps the flip; the flip keeps the page,
/// exactly like upstream's links.
fn hall_of_fame_menu(
    ranking: HofRanking,
    least: bool,
    page: Option<&ListPage>,
) -> Vec<(String, bool)> {
    let loaded = page.is_some();
    let (cur, pages) = page.map(|p| (p.page, p.pages)).unwrap_or((0, 0));
    let mut rows: Vec<(String, bool)> = HOF_RANKINGS
        .iter()
        .map(|r| {
            let label = if *r == ranking {
                format!("{} (shown)", r.label())
            } else {
                r.label().to_string()
            };
            (label, loaded)
        })
        .collect();
    rows.push((
        if least {
            "Show the best instead".into()
        } else {
            "Show the worst instead".into()
        },
        loaded,
    ));
    rows.push(("Next page".into(), loaded && cur + 1 < pages));
    rows.push(("Previous page".into(), loaded && cur > 0));
    rows.push(("Back to the village square".into(), true));
    rows
}

// --- the warrior list + Hall of Fame page builders (pure) --------------------

/// Rows per page in both lists. Upstream pages 50 at a time (and leaves the
/// online/search views unpaged, capping search at `maxlistsize` 100); a TUI
/// panel holds far fewer, so every view pages at this size instead.
const ROSTER_PAGE_SIZE: usize = 15;

/// Typing budget for the warrior name search: a name's worth.
const SEARCH_QUERY_BUDGET: usize = 30;

/// Typing budget for a gold amount (the bounty stake): digits only.
const AMOUNT_QUERY_BUDGET: usize = 9;

/// Search-hit ceiling shared by the haunt and contract pickers (upstream's
/// "narrow it down" check at 100, `maxlistsize`).
const MAX_SEARCH_MATCHES: usize = 100;

/// Upstream's name search interleaves `%` between every typed character
/// (`list.php` builds `%j%o%e%`): a case-insensitive subsequence match.
fn name_matches(name: &str, query: &str) -> bool {
    let name = name.to_lowercase();
    let mut name_chars = name.chars();
    query
        .to_lowercase()
        .chars()
        .all(|q| name_chars.any(|c| c == q))
}

/// The "last seen" column, off seconds since the character's last save.
fn humanize_idle(secs: i64) -> String {
    match secs {
        s if s < 3600 => format!("{}m", (s / 60).max(1)),
        s if s < 86_400 => format!("{}h", s / 3600),
        s => format!("{}d", s / 86_400),
    }
}

/// Build one warrior-list page: filter the view's slice, order it by
/// `list.php`'s total order (level desc, dragon kills desc, name asc — total
/// so no one straddles a page break), and format the window's rows.
fn build_warrior_page(
    entries: &[RosterEntry],
    view: RosterView,
    query: &str,
    page: usize,
) -> ListPage {
    let mut picked: Vec<&RosterEntry> = entries
        .iter()
        .filter(|e| match view {
            RosterView::Online => e.online,
            RosterView::All => true,
            RosterView::Search => name_matches(&e.name, query),
        })
        .collect();
    picked.sort_by(|a, b| {
        b.level
            .cmp(&a.level)
            .then(b.dragon_kills.cmp(&a.dragon_kills))
            .then_with(|| a.handle.to_lowercase().cmp(&b.handle.to_lowercase()))
    });
    let total = picked.len();
    let pages = total.div_ceil(ROSTER_PAGE_SIZE).max(1);
    let page = page.min(pages - 1);
    let heading = match view {
        RosterView::Online => format!("Warriors in the realm right now ({total})"),
        RosterView::All => format!("The warriors of the realm ({total})"),
        RosterView::Search => format!("Warriors answering to \"{query}\" ({total})"),
    };
    let heading = if pages > 1 {
        format!("{heading} - page {} of {pages}", page + 1)
    } else {
        heading
    };
    let rows = picked
        .iter()
        .skip(page * ROSTER_PAGE_SIZE)
        .take(ROSTER_PAGE_SIZE)
        .map(|e| {
            let seen = if e.online {
                "here".to_string()
            } else {
                humanize_idle(e.idle_secs)
            };
            let whereabouts = if e.alive { "village" } else { "graveyard" };
            format!(
                "{:>2}  {:<24.24}  {:<10.10}  {:<9}  {:>5}",
                e.level, e.name, e.race, whereabouts, seen
            )
        })
        .collect();
    ListPage {
        heading,
        header: Some(format!(
            "{:>2}  {:<24}  {:<10}  {:<9}  {:>5}",
            "Lv", "Name", "Race", "Where", "Seen"
        )),
        rows,
        foot: Vec::new(),
        page,
        pages,
    }
}

/// Build one wanted-list page (`dag.php` op=list): the matured open
/// aggregates joined against the roster snapshot, ordered level desc with
/// gold-desc ties by default or gold desc on the toggle (upstream's two sort
/// links; ours breaks pure-gold ties by level where upstream leaves them to
/// chance). Targets with no roster row are dropped — the board read already
/// closed their contracts to the house.
fn build_bounty_page(
    wanted: &[(Uuid, u64)],
    roster: &[RosterEntry],
    by_gold: bool,
    page: usize,
) -> ListPage {
    let mut picked: Vec<(&RosterEntry, u64)> = wanted
        .iter()
        .filter_map(|&(target, gold)| {
            roster
                .iter()
                .find(|e| e.user_id == target)
                .map(|e| (e, gold))
        })
        .collect();
    if by_gold {
        picked.sort_by(|a, b| b.1.cmp(&a.1).then(b.0.level.cmp(&a.0.level)));
    } else {
        picked.sort_by(|a, b| b.0.level.cmp(&a.0.level).then(b.1.cmp(&a.1)));
    }
    let total = picked.len();
    let pages = total.div_ceil(ROSTER_PAGE_SIZE).max(1);
    let page = page.min(pages - 1);
    let mut heading = format!(
        "The wanted list ({total} head{})",
        if total == 1 { "" } else { "s" }
    );
    if pages > 1 {
        heading = format!("{heading} - page {} of {pages}", page + 1);
    }
    let rows = picked
        .iter()
        .skip(page * ROSTER_PAGE_SIZE)
        .take(ROSTER_PAGE_SIZE)
        .map(|(e, gold)| {
            let seen = if e.online {
                "here".to_string()
            } else {
                humanize_idle(e.idle_secs)
            };
            let whereabouts = if e.alive { "village" } else { "graveyard" };
            format!(
                "{:>7}  {:>2}  {:<24.24}  {:<9}  {:>5}",
                gold, e.level, e.name, whereabouts, seen
            )
        })
        .collect();
    ListPage {
        heading,
        header: Some(format!(
            "{:>7}  {:>2}  {:<24}  {:<9}  {:>5}",
            "Gold", "Lv", "Name", "Where", "Seen"
        )),
        rows,
        foot: Vec::new(),
        page,
        pages,
    }
}

/// A ranking's sort/display key. The richest ranking's key is the **fuzzed**
/// wealth — upstream orders by the rand()-perturbed column, so neighbors can
/// swap between reloads and exact fortunes never leak.
fn hof_key(e: &RosterEntry, ranking: HofRanking, rng: &mut impl Rng) -> i64 {
    match ranking {
        HofRanking::Kills => e.dragon_kills as i64,
        HofRanking::Wealth => fuzz_wealth(e.wealth, rng),
        HofRanking::Gems => e.gems.min(i64::MAX as u64) as i64,
        HofRanking::Charm => e.charm as i64,
        HofRanking::Toughness => e.max_hp as i64,
        HofRanking::Resurrections => e.resurrections as i64,
        HofRanking::Speed => e.best_dragon_age as i64,
    }
}

/// `hof.php`'s wealth blur: `total + round(((rand()*10)-5)/100 * total)`,
/// a fresh ±5% every render.
fn fuzz_wealth(wealth: i64, rng: &mut impl Rng) -> i64 {
    wealth + (rng.gen_range(-0.05..0.05) * wealth as f64).round() as i64
}

/// A run-day count for display: 0 renders as unknown (upstream's
/// `IF(dragonage,dragonage,'Unknown')`).
fn days_or_unknown(days: u32) -> String {
    if days == 0 {
        "?".to_string()
    } else {
        days.to_string()
    }
}

/// Build one Hall of Fame page (`hof.php`): filter the ranking's pool, sort
/// by its key with the level/experience/id tie-break *in the same direction*
/// (upstream reuses `$order` for every column, so "worst" flips the
/// tie-break too; the speed ranking's best is ascending), then format the
/// window and the "your rank" percentile.
fn build_hof_page(
    entries: &[RosterEntry],
    me: &Character,
    my_id: Uuid,
    ranking: HofRanking,
    least: bool,
    page: usize,
    rng: &mut impl Rng,
) -> ListPage {
    // The kills ranking lists dragon-slayers only; the speed ranking also
    // needs a recorded pace. Everything else ranks the whole realm.
    let filtered = entries.iter().filter(|e| match ranking {
        HofRanking::Kills => e.dragon_kills > 0,
        HofRanking::Speed => e.dragon_kills > 0 && e.best_dragon_age > 0,
        _ => true,
    });
    let mut keyed: Vec<(&RosterEntry, i64)> = filtered
        .map(|e| {
            let key = hof_key(e, ranking, rng);
            (e, key)
        })
        .collect();
    let asc = if ranking == HofRanking::Speed {
        !least
    } else {
        least
    };
    keyed.sort_by(|(a, ka), (b, kb)| {
        let ord = ka
            .cmp(kb)
            .then(a.level.cmp(&b.level))
            .then(a.experience.cmp(&b.experience))
            .then(a.user_id.cmp(&b.user_id));
        if asc { ord } else { ord.reverse() }
    });

    let total = keyed.len();
    let pages = total.div_ceil(ROSTER_PAGE_SIZE).max(1);
    let page = page.min(pages - 1);
    let heading = match (ranking, least) {
        (HofRanking::Kills, false) => "Heroes with the most dragon kills",
        (HofRanking::Kills, true) => "Heroes with the fewest dragon kills",
        (HofRanking::Wealth, false) => "The richest warriors of the realm",
        (HofRanking::Wealth, true) => "The poorest warriors of the realm",
        (HofRanking::Gems, false) => "The warriors with the most gems",
        (HofRanking::Gems, true) => "The warriors with the fewest gems",
        (HofRanking::Charm, false) => "The most charming warriors of the realm",
        (HofRanking::Charm, true) => "The least charming warriors of the realm",
        (HofRanking::Toughness, false) => "The toughest warriors of the realm",
        (HofRanking::Toughness, true) => "The frailest warriors of the realm",
        (HofRanking::Resurrections, false) => "Warriors best acquainted with death",
        (HofRanking::Resurrections, true) => "Warriors least acquainted with death",
        (HofRanking::Speed, false) => "Heroes with the fastest dragon kills",
        (HofRanking::Speed, true) => "Heroes with the slowest dragon kills",
    }
    .to_string();
    let heading = if pages > 1 {
        format!("{heading} - page {} of {pages}", page + 1)
    } else {
        heading
    };

    let name_head = format!("{:>4}  {:<24}", "", "Name");
    let header = match ranking {
        HofRanking::Kills => Some(format!(
            "{name_head}  {:>5}  {:>3}  {:>4}  {:>4}",
            "Kills", "Lv", "Days", "Best"
        )),
        HofRanking::Wealth => Some(format!("{name_head}  {:>14}", "Estimated gold")),
        // Upstream's gems ranking shows rank and name only: exact counts
        // stay private.
        HofRanking::Gems => Some(name_head.clone()),
        HofRanking::Charm => Some(format!("{name_head}  {:<10}", "Race")),
        HofRanking::Toughness => Some(format!("{name_head}  {:<10}  {:>3}", "Race", "Lv")),
        HofRanking::Resurrections => Some(format!("{name_head}  {:>3}", "Lv")),
        HofRanking::Speed => Some(format!("{name_head}  {:>4}", "Days")),
    };
    let rows = keyed
        .iter()
        .enumerate()
        .skip(page * ROSTER_PAGE_SIZE)
        .take(ROSTER_PAGE_SIZE)
        .map(|(i, (e, key))| {
            // Your own row gets a marker (upstream hilights it).
            let mark = if e.user_id == my_id { '*' } else { ' ' };
            let head = format!("{mark}{:>2}. {:<24.24}", i + 1, e.name);
            match ranking {
                HofRanking::Kills => format!(
                    "{head}  {:>5}  {:>3}  {:>4}  {:>4}",
                    e.dragon_kills,
                    e.level,
                    days_or_unknown(e.dragon_age),
                    days_or_unknown(e.best_dragon_age)
                ),
                HofRanking::Wealth => format!("{head}  {key:>9} gold"),
                HofRanking::Gems => head,
                HofRanking::Charm => format!("{head}  {:<10.10}", e.race),
                HofRanking::Toughness => format!("{head}  {:<10.10}  {:>3}", e.race, e.level),
                HofRanking::Resurrections => format!("{head}  {:>3}", e.level),
                HofRanking::Speed => format!("{head}  {:>4}", e.best_dragon_age),
            }
        })
        .collect();

    let mut foot = Vec::new();
    if ranking == HofRanking::Wealth {
        foot.push("(gold amounts are estimated to within 5% or so)".into());
    }
    if total == 0 {
        foot.push("No heroes stand in this list yet.".into());
    }
    // "Your rank": how many of the listed sort at-or-before your *exact*
    // stat, as a percentile floored at 1 (upstream's `$me` count query).
    // The kills ranking only shows it to dragon-slayers.
    let show_me = ranking != HofRanking::Kills || me.dragon_kills > 0;
    if show_me && total > 0 {
        let mine: i64 = match ranking {
            HofRanking::Kills => me.dragon_kills as i64,
            HofRanking::Wealth => me.gold as i64 + me.gold_in_bank,
            HofRanking::Gems => me.gems.min(i64::MAX as u64) as i64,
            HofRanking::Charm => me.charm as i64,
            HofRanking::Toughness => me.max_hitpoints() as i64,
            HofRanking::Resurrections => me.resurrections as i64,
            HofRanking::Speed => me.best_dragon_age as i64,
        };
        let count = keyed
            .iter()
            .filter(|(_, k)| if asc { *k <= mine } else { *k >= mine })
            .count();
        let pct = ((100.0 * count as f64 / total as f64).round() as u32).max(1);
        foot.push(format!(
            "You stand within about the top {pct}% of this list."
        ));
    }
    ListPage {
        heading,
        header,
        rows,
        foot,
        page,
        pages,
    }
}

/// A commentary room's rows: speak, listen afresh, leave. The speak row
/// carries the venue flavor and, when close to the limit, the posts left
/// (upstream surfaces the count under 3); it disables while the page (which
/// the allowance is counted off) is still loading.
fn commentary_menu(room: CommentRoom, posts_left: Option<usize>) -> Vec<(String, bool)> {
    let prompt = match room {
        CommentRoom::Village => "Speak up",
        CommentRoom::Inn => "Join the table talk",
        CommentRoom::DarkHorse => "Scratch an etching of your own",
        CommentRoom::Gardens => "Whisper something",
        CommentRoom::Veterans => "Boast of your deeds",
        CommentRoom::ShadeGypsy => "Project your voice to the dead",
        CommentRoom::ShadeGrave => "Add your lament",
    };
    let speak = match posts_left {
        Some(0) => ("You have said enough here today".to_string(), false),
        Some(n) if n < 3 => (format!("{prompt} ({n} left today)"), true),
        Some(_) => (prompt.to_string(), true),
        None => (prompt.to_string(), false),
    };
    let back = match room {
        CommentRoom::Inn => "Back to the common room",
        CommentRoom::DarkHorse => "Back to the taproom",
        CommentRoom::ShadeGrave => "Back among the graves",
        CommentRoom::ShadeGypsy => "Snap out of the trance",
        _ => "Back to the village square",
    };
    vec![
        speak,
        ("Listen for new voices".into(), true),
        (back.into(), true),
    ]
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
fn fight_menu(c: &Character, kind: FoeKind) -> Vec<(String, bool)> {
    // A fight you picked with a sleeping warrior offers no skills ("your
    // honor prevents it") and no way out ("your pride prevents it") —
    // `pvp.php` strips both.
    if kind == FoeKind::Pvp {
        return vec![("Attack".into(), true)];
    }
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
        (
            format!("Haunt a foe ({} favor)", model::HAUNT_FAVOR_THRESHOLD),
            c.favor >= model::HAUNT_FAVOR_THRESHOLD,
        ),
        ("Lament with the lost souls".into(), true),
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
        (
            "Your room is paid (a warm bed waits upstairs)".into(),
            false,
        )
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
        (
            format!("Approach {} in his shadowed booth", data::BOUNTY_BROKER),
            true,
        ),
        ("Listen in at the long table".into(), true),
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
/// The PvP target rows for one venue, plus the count of sleepers at the
/// other (`pvplist.php`'s filter and location split made local): a target is
/// listed when they're someone else, alive, asleep (offline by the presence
/// window), past newbie immunity, within `[mine-1, mine+2]` levels, and at
/// this venue — the inn's rooms hold the lodged, the fields everyone else.
/// Rows engaged within the 10-minute dogpile window show but can't be
/// picked. Ordered level, then experience, then dragon kills, descending
/// (upstream's within-location order).
fn build_pvp_rows(
    roster: &[RosterEntry],
    me: Uuid,
    my_level: u8,
    venue: PvpVenue,
    now: i64,
) -> (Vec<(Uuid, String, bool)>, usize) {
    let (lo, hi) = (my_level as i16 - 1, my_level as i16 + 2);
    let mut eligible: Vec<&RosterEntry> = roster
        .iter()
        .filter(|e| {
            e.user_id != me
                && e.alive
                && !e.online
                && !e.pvp_immune
                && (lo..=hi).contains(&(e.level as i16))
        })
        .collect();
    eligible.sort_by(|a, b| {
        (b.level, b.experience, b.dragon_kills).cmp(&(a.level, a.experience, a.dragon_kills))
    });
    let here = |e: &RosterEntry| e.lodged == (venue == PvpVenue::Inn);
    let elsewhere = eligible.iter().filter(|e| !here(e)).count();
    let rows = eligible
        .iter()
        .filter(|e| here(e))
        .map(|e| {
            let hunted = now - e.pvp_engaged_at < model::PVP_TIMEOUT_SECS;
            let label = if hunted {
                format!("{} (level {}) - hunted too recently", e.name, e.level)
            } else {
                format!("{} (level {})", e.name, e.level)
            };
            (e.user_id, label, !hunted)
        })
        .collect();
    (rows, elsewhere)
}

/// The quiet word a successful bribe buys (`inn_bartender.php`'s unlocked
/// navs): the rooms upstairs and the specialty switch.
fn barkeep_ear_menu() -> Vec<(String, bool)> {
    vec![
        ("Ask who's sleeping upstairs".into(), true),
        ("Ask about switching your path".into(), true),
        ("Slide back down the bar".into(), true),
    ]
}

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
        rows.push((format!("Steal an hour with {partner}"), !c.flirted_today));
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
                ("Read the etchings in the table".into(), true),
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
        Some((master, _, _)) if c.seen_master_today => vec![(
            format!("{} has seen enough of you today", master.name),
            false,
        )],
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
        assert!(!rows[3].1); // haunting needs 25 favor
        assert!(rows[4].1); // the lost souls always listen
        assert!(rows[5].1); // waiting always works

        c.grave_fights = 4;
        c.favor = 100;
        let rows = graveyard_menu(&c);
        assert!(rows[0].1);
        assert!(rows[1].1);
        assert!(rows[2].1);
        assert!(rows[3].1); // 100 favor covers the haunt too

        // A whole soul has nothing to restore, whatever the favor.
        c.soulpoints = c.max_soulpoints();
        assert!(!graveyard_menu(&c)[1].1);
    }

    #[test]
    fn fight_menu_hides_skills_from_the_dead() {
        let mut c = lvl(5);
        c.choose_specialty(Specialty::Thief);
        // Alive: Attack + 4 skills + Flee.
        assert_eq!(fight_menu(&c, FoeKind::Creature).len(), 6);
        // PvP strips skills AND the way out ("honor" and "pride").
        let rows = fight_menu(&c, FoeKind::Pvp);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].0, "Attack");
        // Dead (a torment fight): bare essence only.
        c.die();
        let rows = fight_menu(&c, FoeKind::Torment);
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

    #[test]
    fn training_gate_blocks_a_second_daily_challenge() {
        let mut c = lvl(1);
        c.experience = c.exp_for_next_level();
        assert!(c.can_challenge_master());
        assert!(training_menu(&c)[0].0.starts_with("Challenge"));

        // The challenge spends the day's audience; only a win reopens it.
        c.seen_master_today = true;
        assert!(!c.can_challenge_master());
        let rows = training_menu(&c);
        assert!(rows[0].0.contains("seen enough of you"));
        assert!(!rows[0].1);
        c.advance_level();
        assert!(!c.seen_master_today);
    }

    #[test]
    fn commentary_menu_gates_the_speak_row_on_the_allowance() {
        // Loading: nothing to count against, so speaking waits.
        let rows = commentary_menu(CommentRoom::Village, None);
        assert_eq!(rows.len(), 3);
        assert!(!rows[0].1);
        assert!(rows[1].1); // refresh
        assert!(rows[2].1); // leave

        // Plenty left: a plain prompt.
        let rows = commentary_menu(CommentRoom::Village, Some(13));
        assert!(rows[0].1);
        assert!(!rows[0].0.contains("left today"));

        // Running low surfaces the count (upstream shows it under 3).
        let rows = commentary_menu(CommentRoom::Village, Some(2));
        assert!(rows[0].0.contains("2 left today"));
        assert!(rows[0].1);

        // Exhausted: the row closes.
        let rows = commentary_menu(CommentRoom::DarkHorse, Some(0));
        assert!(!rows[0].1);
    }

    #[test]
    fn village_menu_lists_the_talk_rooms() {
        let mut c = lvl(3);
        c.gold = 0;
        let rows = village_menu(&c);
        assert!(rows.iter().any(|(l, _)| l.starts_with("The Town Square")));
        assert!(rows.iter().any(|(l, _)| l.starts_with("The Gardens")));
        assert!(
            rows.iter()
                .any(|(l, _)| l.starts_with("A weathered standing stone"))
        );
        // The seance is pay-per-visit: level 3 wants 60 gold.
        let gypsy = rows
            .iter()
            .find(|(l, _)| l.starts_with("The Gypsy's Tent"))
            .unwrap();
        assert!(gypsy.0.contains("60 gold"));
        assert!(!gypsy.1);
        c.gold = 60;
        let rows = village_menu(&c);
        assert!(
            rows.iter()
                .find(|(l, _)| l.starts_with("The Gypsy's Tent"))
                .unwrap()
                .1
        );
    }

    // --- the warrior list + Hall of Fame ---------------------------------

    fn entry(handle: &str, level: u8) -> RosterEntry {
        RosterEntry {
            user_id: Uuid::from_u128(handle.bytes().fold(0u128, |a, b| a * 31 + b as u128)),
            name: format!("Seedling {handle}"),
            handle: handle.to_string(),
            level,
            alive: true,
            race: "Plainsborn",
            dragon_kills: 0,
            dragon_age: 0,
            best_dragon_age: 0,
            resurrections: 0,
            gems: 0,
            charm: 0,
            max_hp: level as u32 * 10,
            experience: 0,
            wealth: 0,
            online: false,
            idle_secs: 0,
            lodged: false,
            pvp_immune: false,
            bounty_immune: false,
            pvp_engaged_at: 0,
        }
    }

    // --- the bounty board (modules/dag.php) --------------------------------

    #[test]
    fn bounty_page_orders_by_level_then_gold_and_flips() {
        let low = entry("low", 3);
        let high = entry("high", 9);
        let rich_low = entry("richlow", 3);
        let wanted = vec![
            (low.user_id, 200u64),
            (high.user_id, 150u64),
            (rich_low.user_id, 500u64),
        ];
        let roster = vec![low, high, rich_low];
        // Default: level desc, gold desc within a level (dag's default sort).
        let page = build_bounty_page(&wanted, &roster, false, 0);
        assert!(page.rows[0].contains("high"));
        assert!(page.rows[1].contains("richlow"));
        assert!(page.rows[2].contains("low"));
        // The gold toggle re-orders by the price alone.
        let page = build_bounty_page(&wanted, &roster, true, 0);
        assert!(page.rows[0].contains("richlow"));
        assert!(page.rows[1].contains("low"));
        assert!(page.rows[2].contains("high"));
    }

    #[test]
    fn bounty_page_drops_targets_without_a_roster_row() {
        // A vanished character's contracts were closed by the board read;
        // whatever aggregate still arrives has no row to hang on.
        let known = entry("known", 5);
        let wanted = vec![(known.user_id, 100u64), (Uuid::from_u128(424242), 999u64)];
        let roster = vec![known];
        let page = build_bounty_page(&wanted, &roster, false, 0);
        assert_eq!(page.rows.len(), 1);
        assert!(page.heading.contains("1 head"));
    }

    // --- PvP target lists (pvp.php + lib/pvplist.php) ---------------------

    #[test]
    fn pvp_rows_filter_the_ineligible() {
        let me = Uuid::from_u128(999);
        let mut sleeper = entry("prey", 5);
        let awake = {
            let mut e = entry("awake", 5);
            e.online = true;
            e
        };
        let shielded = {
            let mut e = entry("green", 5);
            e.pvp_immune = true;
            e
        };
        let dead = {
            let mut e = entry("ghost", 5);
            e.alive = false;
            e
        };
        let low = entry("low", 3); // below my-1
        let high = entry("high", 8); // above my+2
        let roster = vec![sleeper.clone(), awake, shielded, dead, low, high];
        // My level 5: the band is [4, 7]; only the plain sleeper qualifies.
        let (rows, elsewhere) = build_pvp_rows(&roster, me, 5, PvpVenue::Fields, 100_000);
        assert_eq!(rows.len(), 1);
        assert!(rows[0].1.contains("prey"));
        assert!(rows[0].2); // attackable
        assert_eq!(elsewhere, 0);

        // A fresh engage flags them off for ten minutes, but they still show.
        sleeper.pvp_engaged_at = 100_000 - 60;
        let (rows, _) = build_pvp_rows(&[sleeper.clone()], me, 5, PvpVenue::Fields, 100_000);
        assert!(!rows[0].2);
        assert!(rows[0].1.contains("hunted too recently"));
        sleeper.pvp_engaged_at = 100_000 - model::PVP_TIMEOUT_SECS;
        let (rows, _) = build_pvp_rows(&[sleeper.clone()], me, 5, PvpVenue::Fields, 100_000);
        assert!(rows[0].2);
    }

    #[test]
    fn pvp_venues_split_on_the_inn_room() {
        let me = Uuid::from_u128(999);
        let fields = entry("fields", 5);
        let mut lodged = entry("lodged", 5);
        lodged.lodged = true;
        let roster = vec![fields, lodged];
        // The fields list holds the unlodged and rumors the other.
        let (rows, elsewhere) = build_pvp_rows(&roster, me, 5, PvpVenue::Fields, 0);
        assert_eq!(rows.len(), 1);
        assert!(rows[0].1.contains("fields"));
        assert_eq!(elsewhere, 1);
        // The inn's keys open only the lodged rooms.
        let (rows, elsewhere) = build_pvp_rows(&roster, me, 5, PvpVenue::Inn, 0);
        assert_eq!(rows.len(), 1);
        assert!(rows[0].1.contains("lodged"));
        assert_eq!(elsewhere, 1);
    }

    #[test]
    fn pvp_rows_never_list_yourself() {
        let me = Uuid::from_u128(999);
        let mut myself = entry("me", 5);
        myself.user_id = me;
        let (rows, elsewhere) = build_pvp_rows(&[myself], me, 5, PvpVenue::Fields, 0);
        assert!(rows.is_empty());
        assert_eq!(elsewhere, 0);
    }

    #[test]
    fn village_menu_offers_the_hunt() {
        let c = lvl(3);
        let row = village_menu(&c)
            .into_iter()
            .find(|(l, _)| l.starts_with("Slay Other Warriors"))
            .unwrap();
        assert!(row.0.contains("3 left today"));
        assert!(row.1);
    }

    #[test]
    fn warrior_list_orders_by_level_kills_then_name() {
        // list.php: level DESC, dragonkills DESC, login ASC — a total order.
        let mut a = entry("zed", 5);
        let mut b = entry("abe", 5);
        b.dragon_kills = 2;
        let c = entry("moe", 9);
        let page = build_warrior_page(&[a.clone(), b.clone(), c.clone()], RosterView::All, "", 0);
        assert!(page.rows[0].contains("moe"));
        assert!(page.rows[1].contains("abe")); // kills break the level tie
        assert!(page.rows[2].contains("zed"));
        // Same level and kills: the bare name decides.
        a.dragon_kills = 2;
        b.name = "Zzz abe".into(); // the *display* name must not re-order
        let page = build_warrior_page(&[a, b, c], RosterView::All, "", 0);
        assert!(page.rows[1].contains("abe"));
    }

    #[test]
    fn warrior_search_is_a_subsequence_match() {
        // Upstream interleaves % between typed characters: `%j%o%e%`.
        assert!(name_matches("Farmboy Joe", "joe"));
        assert!(name_matches("Journeyman Orc Expert", "joe")); // subsequence
        assert!(!name_matches("Joe", "joex"));
        assert!(name_matches("Anything", ""));
    }

    #[test]
    fn warrior_online_view_filters_and_pages_clamp() {
        let mut on = entry("here", 3);
        on.online = true;
        let off = entry("gone", 7);
        let entries = [on, off];
        let page = build_warrior_page(&entries, RosterView::Online, "", 0);
        assert_eq!(page.rows.len(), 1);
        assert!(page.rows[0].contains("here"));
        // A page past the end clamps to the last page instead of blanking.
        let page = build_warrior_page(&entries, RosterView::All, "", 99);
        assert_eq!(page.page, 0);
        assert_eq!(page.rows.len(), 2);
    }

    #[test]
    fn hof_kills_lists_slayers_only_and_gates_your_rank() {
        let mut vet = entry("vet", 10);
        vet.dragon_kills = 3;
        vet.dragon_age = 9;
        vet.best_dragon_age = 7;
        let fresh = entry("fresh", 2);
        let me = lvl(5); // no kills
        let page = build_hof_page(
            &[vet, fresh],
            &me,
            Uuid::nil(),
            HofRanking::Kills,
            false,
            0,
            &mut rand::thread_rng(),
        );
        assert_eq!(page.rows.len(), 1);
        assert!(page.rows[0].contains("vet"));
        // No kills: no "your rank" line (upstream only sets $me when
        // dragonkills > 0 on this ranking).
        assert!(!page.foot.iter().any(|f| f.contains("top")));
    }

    #[test]
    fn hof_gems_ranking_shows_names_only() {
        let mut rich = entry("rich", 5);
        rich.gems = 40;
        let page = build_hof_page(
            &[rich],
            &lvl(1),
            Uuid::nil(),
            HofRanking::Gems,
            false,
            0,
            &mut rand::thread_rng(),
        );
        // Exact gem counts never render (upstream lists rank + name only).
        assert!(!page.rows[0].contains("40"));
    }

    #[test]
    fn hof_wealth_is_fuzzed_within_five_percent() {
        let mut rich = entry("rich", 5);
        rich.wealth = 10_000;
        let mut rng = rand::thread_rng();
        for _ in 0..200 {
            let key = hof_key(&rich, HofRanking::Wealth, &mut rng);
            assert!((9_500..=10_500).contains(&key), "fuzz out of range: {key}");
        }
        // Debt fuzzes too (the total is signed).
        rich.wealth = -1_000;
        let key = hof_key(&rich, HofRanking::Wealth, &mut rng);
        assert!((-1_050..=-950).contains(&key));
    }

    #[test]
    fn hof_speed_ranks_ascending_and_least_flips_it() {
        let mut quick = entry("quick", 5);
        quick.dragon_kills = 1;
        quick.best_dragon_age = 3;
        let mut slow = entry("slow", 5);
        slow.dragon_kills = 1;
        slow.best_dragon_age = 20;
        let mut unranked = entry("never", 15); // no kill: filtered out
        unranked.best_dragon_age = 0;
        let entries = [quick, slow, unranked];
        let page = build_hof_page(
            &entries,
            &lvl(1),
            Uuid::nil(),
            HofRanking::Speed,
            false,
            0,
            &mut rand::thread_rng(),
        );
        assert_eq!(page.rows.len(), 2);
        assert!(page.rows[0].contains("quick")); // fastest first
        let page = build_hof_page(
            &entries,
            &lvl(1),
            Uuid::nil(),
            HofRanking::Speed,
            true,
            0,
            &mut rand::thread_rng(),
        );
        assert!(page.rows[0].contains("slow")); // "worst" = slowest first
    }

    #[test]
    fn hof_percentile_counts_at_or_better_and_floors_at_one() {
        let mut me = lvl(5);
        me.charm = 10;
        let mut best = entry("best", 5);
        best.charm = 50;
        let mut mid = entry("mid", 5);
        mid.charm = 10;
        let mut worst = entry("worst", 5);
        worst.charm = 1;
        let page = build_hof_page(
            &[best, mid, worst],
            &me,
            Uuid::nil(),
            HofRanking::Charm,
            false,
            0,
            &mut rand::thread_rng(),
        );
        // Two of three have charm >= mine: round(200/3) = 67.
        assert!(
            page.foot.iter().any(|f| f.contains("top 67%")),
            "{:?}",
            page.foot
        );
    }

    #[test]
    fn hof_marks_your_own_row() {
        let mut mine = entry("me", 5);
        mine.charm = 9;
        let my_id = mine.user_id;
        let page = build_hof_page(
            &[mine, entry("other", 5)],
            &lvl(5),
            my_id,
            HofRanking::Charm,
            false,
            0,
            &mut rand::thread_rng(),
        );
        assert!(page.rows[0].starts_with('*'));
        assert!(!page.rows[1].starts_with('*'));
    }

    #[test]
    fn warrior_list_menu_gates_the_pager() {
        // Loading: only the presence row and the way back are live.
        let rows = warrior_list_menu(None);
        assert!(!rows[0].1);
        assert!(rows[1].1);
        assert!(!rows[3].1);
        assert!(rows[5].1);
        // One page of results: no pager either way.
        let page = ListPage {
            pages: 3,
            page: 1,
            ..ListPage::default()
        };
        let rows = warrior_list_menu(Some(&page));
        assert!(rows[3].1); // next
        assert!(rows[4].1); // previous
    }

    #[test]
    fn hall_of_fame_menu_marks_the_shown_ranking() {
        let page = ListPage::default();
        let rows = hall_of_fame_menu(HofRanking::Wealth, false, Some(&page));
        assert!(rows[1].0.contains("(shown)"));
        assert!(rows[7].0.contains("worst"));
        let rows = hall_of_fame_menu(HofRanking::Wealth, true, Some(&page));
        assert!(rows[7].0.contains("best"));
    }

    #[test]
    fn village_menu_lists_the_rosters() {
        let rows = village_menu(&lvl(1));
        assert!(rows.iter().any(|(l, _)| l == "List Warriors"));
        assert!(rows.iter().any(|(l, _)| l == "The Hall of Fame"));
    }
}
