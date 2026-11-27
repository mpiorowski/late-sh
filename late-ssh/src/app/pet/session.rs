use std::time::{Duration, Instant};

use chrono::{NaiveDate, NaiveTime};
use late_core::models::pet::{
    Fed, Form, PetGuest, PetMood, PetSpecies, PetStay, PetVisitor, Pick, TreatKind, Trick,
    VisitKind,
};
use ratatui::layout::{Position, Rect};
use tokio::sync::{broadcast, mpsc};
use uuid::Uuid;

use super::state::{self, Growth, Need, Practiced};
use super::svc::{
    Did, PET_CHIP_BONUS, PetCommand, PetOutcome, PetService, PetSnapshot, Refusal, Visited,
};
use super::ui::Portrait;
use crate::app::activity::event::GameFamily;
use crate::app::common::primitives::{Banner, Screen};

/// How long each event mood lasts. A win outlasts a loss on purpose: the
/// pet is an optimist.
pub const PURR_FOR: Duration = Duration::from_secs(2 * 60);
pub const PROUD_FOR: Duration = Duration::from_secs(30 * 60);
pub const SULK_FOR: Duration = Duration::from_secs(10 * 60);
pub const CHATTY_FOR: Duration = Duration::from_secs(10 * 60);
/// No key from the owner for this long and the pet dozes off: well before
/// the account reads as idle to anyone else.
pub const ASLEEP_AFTER: Duration = Duration::from_secs(10 * 60);

/// Body width including the tail column, used to keep the pet inside its box.
pub const PET_WIDTH: usize = 8;
pub const PET_HEIGHT: usize = 3;

/// What the session has told the pet, as the moment each thing last
/// happened. Nothing here is persisted: a fresh session starts blank, so
/// the first tick reads idle (or asleep, for a session nobody touches).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MoodSignals {
    /// A click on the pet.
    pub petted: Option<Instant>,
    /// Any win: a game, a daily match, a boss.
    pub won: Option<Instant>,
    /// Any loss: a death in a door game, a lost daily match.
    pub lost: Option<Instant>,
    /// A chat message of the owner's landed, in any room or DM.
    pub spoke: Option<Instant>,
}

/// The session facts the tick hands over every time: they are read, not
/// pushed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ambient {
    /// The owner's last keystroke or click.
    pub last_input: Instant,
    /// A paired client is playing, unmuted.
    pub music_playing: bool,
}

/// The one reading: precedence top to bottom (`PetMood::ALL`). Event moods
/// hold for their window; sleep beats the music, since a pet dozing beside
/// a radio is the honest picture of an owner who walked off.
pub fn mood_at(signals: MoodSignals, ambient: Ambient, now: Instant) -> PetMood {
    let within = |at: Option<Instant>, window: Duration| {
        at.is_some_and(|at| now.saturating_duration_since(at) < window)
    };
    if within(signals.petted, PURR_FOR) {
        PetMood::Purring
    } else if within(signals.won, PROUD_FOR) {
        PetMood::Proud
    } else if within(signals.lost, SULK_FOR) {
        PetMood::Sulking
    } else if within(signals.spoke, CHATTY_FOR) {
        PetMood::Chatty
    } else if now.saturating_duration_since(ambient.last_input) >= ASLEEP_AFTER {
        PetMood::Asleep
    } else if ambient.music_playing {
        PetMood::Vibing
    } else {
        PetMood::Idle
    }
}

/// Where the pet's eyes point while it walks after the cursor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Look {
    Left,
    Ahead,
    Right,
}

/// The pet off the stroll formula: walking after the cursor, or sitting
/// under it. Cell offsets inside the roam zone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Perch {
    pub x: usize,
    pub y: usize,
    pub look: Look,
}

/// What the last draw of the box used, recorded for the tick: how far the
/// pet can travel, the zone it travels in (so the cursor can be placed in
/// it), what it has to watch beside it, and where it stood.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PetFrameInputs {
    pub travel: PetTravel,
    pub zone: Rect,
    pub neighbours: super::ui::Neighbours,
    pub position: (usize, usize),
    /// Where the pet would stand with no perch (the stroll, the sulk spot,
    /// the glass): where a pet the cursor let go of walks back to.
    pub home: (usize, usize),
    /// The tile's row over the box and the pouch row under it, when the
    /// tile drew them: a click on either opens the care box.
    pub rows: [Option<Rect>; 2],
    /// The treat drawn in the pouch row: a click on it gives it.
    pub treat: Option<Rect>,
}

/// How far the pet can travel inside its box, in cells, on each axis.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PetTravel {
    pub x: usize,
    pub y: usize,
}

/// Everything one tick reads.
#[derive(Clone, Copy, Debug)]
pub struct PetTick {
    /// The app's shared 66ms wall clock (marquee_tick): the adaptive loop
    /// ticks sparsely, so a per-call counter would slow the animation with
    /// the cadence; syncing to the wall clock keeps every speed true.
    pub wall_tick: usize,
    pub now: Instant,
    pub ambient: Ambient,
    /// The box drawn last frame, `None` when no box was drawn.
    pub frame: Option<PetFrameInputs>,
    /// The terminal cursor, 0-based cells, when the terminal reported it.
    pub cursor: Option<(u16, u16)>,
    /// The pet shows (`App::pet_out`: not put away). Only then does a key
    /// count a day, a win earn a treat, or a click reach it.
    pub out: bool,
    pub today: NaiveDate,
    /// How long ago this UTC day began: a key younger than this was
    /// pressed today.
    pub day_age: Duration,
}

/// What one tick came to.
#[derive(Debug, Default)]
pub struct PetTicked {
    /// A state edge that needs a frame even when the animation predicate
    /// is quiet: a mood change, a step of the walk, a toy moving, an answer
    /// from the service.
    pub changed: bool,
    /// What an answer has to say to the owner.
    pub banner: Option<Banner>,
}

/// Something the pet is in the middle of, for a few seconds. Session-only:
/// nothing here is stored.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Moment {
    Eating(TreatKind),
    /// Its owner is back after `days_away` whole days.
    Welcome {
        days_away: i64,
    },
    Trick(Trick),
    /// Just out of the egg.
    Hatched,
}

impl Moment {
    /// How long it lasts, in wall ticks (66ms).
    pub fn ticks(self) -> usize {
        match self {
            Moment::Eating(_) => 45,
            Moment::Trick(_) => 60,
            Moment::Hatched => 60,
            // The longer the silence, the bigger the welcome: six seconds,
            // and three more for each day away, up to a minute.
            Moment::Welcome { days_away } => {
                (90 + 45 * usize::try_from(days_away).unwrap_or(0)).min(900)
            }
        }
    }
}

/// What the owner threw for it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Toy {
    /// Lands and lies there until fetched.
    Ball,
    /// A toy mouse that runs along the floor.
    Mouse,
}

/// A toy in the pet's box, in cell offsets inside the box.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Plaything {
    pub toy: Toy,
    pub x: usize,
    pub y: usize,
    /// The wall tick it was thrown on.
    thrown: usize,
    /// The column it was thrown at: the mouse runs from here.
    from: usize,
}

/// A toy nobody fetched is picked up after this many wall ticks.
pub const TOY_TICKS: usize = 20_000 / 66;

/// What the art is drawn from besides the pose and the clock.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Body {
    pub species: PetSpecies,
    pub form: Form,
    pub mood: PetMood,
    /// An egg with its days: it rocks until it is clicked.
    pub ready: bool,
    /// The treat it ate today: the thing it carries until the day ends.
    pub carries: Option<TreatKind>,
    /// The moment it is in and the wall tick it began on.
    pub moment: Option<(Moment, usize)>,
}

/// What a click on the pet came to, for the caller that has to act on it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Clicked {
    /// The pet took it: a pat, a treat, a trick.
    Done,
    /// It has what its next stage asks for (days for an egg, treats
    /// after): the care box is where its owner picks what hatches, or
    /// which drawing it grows into.
    OpenCare,
}

/// The care box's own state: what this session is looking at.
#[derive(Clone, Debug, Default)]
pub struct Care {
    /// The trick the teach row points at, an index into `Trick::ALL`.
    pub trick: usize,
    /// Who came by, as of the box opening: the list stays while the box is
    /// up even though opening it marks them seen.
    pub came_by: Vec<PetVisitor>,
}

pub struct PetState {
    pub user_id: Uuid,
    pub(super) svc: PetService,

    /// User-set pet name. `None` until set via the `/petname` chat command.
    pub name: Option<String>,
    /// What it is, from the row: picked at the hatch, changed by `/pet`.
    /// An egg is no animal yet, whatever the row holds.
    pub species: PetSpecies,
    /// The last UTC day this account petted it, from the row and then from
    /// this session's clicks: the care row reads it, and a second click the
    /// same day does not ask the service again.
    pub last_petted: Option<NaiveDate>,
    /// The owner's switch, from the row and then from this session's
    /// `/pet away` and `/pet out`. Off, the pet keeps everything and is put
    /// away (`App::pet_out`).
    pub enabled: bool,

    pub signals: MoodSignals,
    mood: PetMood,
    perch: Option<Perch>,
    animation_ticks: usize,

    /// The mirror of the stored pet: drawn from, never decided from. It
    /// moves when the service answers or another session changes the row.
    growth: Growth,
    revision: i64,
    waiting: Option<TreatKind>,
    fed: Option<Fed>,
    visitors: Vec<PetVisitor>,
    /// The pets staying with this one, newest first. A stay ends on the
    /// clock: the tick lets a guest go when its hour is up.
    guests: Vec<PetGuest>,
    /// Where this pet went calling, while its hour there runs, and the
    /// minutes it has left as of the last tick.
    calling: Option<PetStay>,
    calling_left: Option<i64>,
    outcome_tx: mpsc::UnboundedSender<PetOutcome>,
    outcome_rx: mpsc::UnboundedReceiver<PetOutcome>,
    changes_rx: broadcast::Receiver<Uuid>,

    /// The owner's command that is out and not answered yet. One at a
    /// time: a held key must not queue a pooled connection per repeat.
    busy: Option<PetCommand>,
    /// A visit to someone else's pet is out and not answered yet: the
    /// same rule for the profile's keys.
    visiting: bool,
    /// `Ambient::last_input` as the first tick saw it: the session's start,
    /// not a key. A day counts on a key pressed after it.
    boot_input: Option<Instant>,
    /// The UTC day this session last asked to have counted. One ask a day:
    /// a refusal (the pet put away from another session) is not asked
    /// again on every key. A failure clears it, and so does bringing the
    /// pet out.
    day_asked: Option<NaiveDate>,
    /// The UTC day this session last asked for a treat on.
    earn_asked: Option<NaiveDate>,
    /// Active seconds in each door on `door_day`.
    door_day: Option<NaiveDate>,
    door_seconds: Vec<(Screen, f64)>,
    /// What the last tick was told: the glue's facts for the calls that
    /// arrive between ticks (a win, a click).
    out: bool,
    today: Option<NaiveDate>,

    moment: Option<(Moment, usize)>,
    plaything: Option<Plaything>,
    pub care: Care,
}

impl PetState {
    /// `pet` is the bootstrap load. `None` (the load failed) draws a
    /// sleeping egg until the first answer or change notice
    /// replaces it: revision -1, so anything stored is newer.
    pub fn new(user_id: Uuid, svc: PetService, pet: Option<PetSnapshot>) -> Self {
        let (outcome_tx, outcome_rx) = mpsc::unbounded_channel();
        let changes_rx = svc.subscribe_changes();
        let mut state = Self {
            user_id,
            svc,
            name: None,
            species: PetSpecies::Cat,
            last_petted: None,
            enabled: true,
            signals: MoodSignals::default(),
            mood: PetMood::Asleep,
            perch: None,
            animation_ticks: 0,
            growth: Growth {
                days_together: 0,
                last_day: None,
                form: Form::Egg,
                bond_days: 0,
                last_bond_day: None,
                tricks: Vec::new(),
                learning: None,
                learning_days: 0,
                last_practice: None,
                treats_eaten: 0,
            },
            revision: -1,
            waiting: None,
            fed: None,
            visitors: Vec::new(),
            guests: Vec::new(),
            calling: None,
            calling_left: None,
            outcome_tx,
            outcome_rx,
            changes_rx,
            busy: None,
            visiting: false,
            boot_input: None,
            day_asked: None,
            earn_asked: None,
            door_day: None,
            door_seconds: Vec::new(),
            out: false,
            today: None,
            moment: None,
            plaything: None,
            care: Care::default(),
        };
        if let Some(pet) = pet {
            // The mood the row holds (`asleep` after the last session
            // left): the first tick reads the real thing and, since it
            // differs, writes it, so the profile stops saying asleep the
            // moment the owner is back.
            state.mood = pet.row.mood();
            state.name = pet.row.name.clone();
            state.enabled = pet.row.enabled;
            state.install(pet);
        }
        state
    }

    /// Replace the mirror with a stored pet, unless the mirror already
    /// shows a newer one: a reload and an answer race each other and can
    /// arrive out of order. Every write a mirror shows bumps the row's
    /// revision, so it orders them. Returns the visitors that are new.
    fn install(&mut self, pet: PetSnapshot) -> Vec<PetVisitor> {
        if pet.row.revision < self.revision {
            return Vec::new();
        }
        let new_visitors: Vec<PetVisitor> = pet
            .visitors
            .iter()
            .filter(|visit| !self.visitors.contains(visit))
            .cloned()
            .collect();
        self.revision = pet.row.revision;
        self.growth = Growth::from_row(&pet.row);
        self.species = pet.row.species();
        // Another session of this owner may have petted it since.
        self.last_petted = self.last_petted.max(pet.row.last_petted);
        self.waiting = pet.waiting;
        self.fed = pet.fed;
        self.visitors = pet.visitors;
        self.guests = pet.guests;
        self.calling = pet.calling;
        new_visitors
    }

    /// Set (or clear with `None`) the user-set pet name and persist it.
    pub fn set_name(&mut self, name: Option<String>) {
        self.name = name.clone();
        self.svc.set_name_task(self.user_id, name);
    }

    /// Turn a hatched pet into another animal (`/pet cat`). The answer
    /// says what it became, or that it is still an egg.
    pub fn change_species(&mut self, species: PetSpecies) {
        self.request(PetCommand::Species(species));
    }

    /// Start over as an egg (`/pet reset yes`): the days, the stage and
    /// the tricks go; the name stays.
    pub fn start_over(&mut self) {
        self.request(PetCommand::Reset);
    }

    /// Bring the pet out or put it away, and persist it. The day is not
    /// asked for until the write is over (`PetOutcome::Switched`): asked
    /// sooner, it could find the pet still away and be refused for the day.
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        self.day_asked = self.today;
        self.svc
            .set_enabled_task(self.user_id, enabled, self.outcome_tx.clone());
    }

    /// The session is ending: an owner going offline leaves a sleeping pet
    /// on the profile.
    pub fn session_ended(&self) {
        self.svc.set_mood_task(self.user_id, PetMood::Asleep);
    }

    pub fn mood(&self) -> PetMood {
        self.mood
    }

    pub fn perch(&self) -> Option<Perch> {
        self.perch
    }

    pub fn animation_ticks(&self) -> usize {
        self.animation_ticks
    }

    pub fn note_petted(&mut self, now: Instant) {
        self.signals.petted = Some(now);
    }

    /// What the box draws this pet from.
    pub fn body(&self) -> Body {
        Body {
            species: self.species,
            form: self.growth.form,
            mood: self.mood,
            ready: self.growth.ready_to_hatch(),
            carries: self.fed_today(),
            moment: self.moment,
        }
    }

    pub fn growth(&self) -> &Growth {
        &self.growth
    }

    /// The pet as other surfaces draw it (`ui::portrait_lines`): its body
    /// at its stage in its mood, carrying what it ate today, with the
    /// tricks it knows. What the Arcade's corner and the tavern's heel
    /// read.
    pub fn portrait(&self) -> Portrait<'_> {
        Portrait {
            species: self.species,
            form: self.growth.form,
            mood: self.mood,
            carries: self.fed_today(),
            tricks: &self.growth.tricks,
        }
    }

    /// The treat waiting in the pouch, if one does.
    pub fn waiting(&self) -> Option<TreatKind> {
        self.waiting
    }

    /// Visits the owner has not looked at, newest first.
    pub fn visitors(&self) -> &[PetVisitor] {
        &self.visitors
    }

    /// The pets staying with this one right now, newest first.
    pub fn guests(&self) -> &[PetGuest] {
        &self.guests
    }

    /// Where this pet is staying right now: its host's username and the
    /// minutes it has left there.
    pub fn calling(&self) -> Option<(&str, i64)> {
        match (&self.calling, self.calling_left) {
            (Some(stay), Some(left)) => Some((stay.host.as_str(), left)),
            (Some(_), None) | (None, _) => None,
        }
    }

    /// Whether the egg has its days and waits on its owner to hatch it.
    pub fn ready_to_hatch(&self) -> bool {
        self.growth.ready_to_hatch()
    }

    /// The two forms it can grow into, while it has its treats and waits
    /// on its owner to pick one.
    pub fn offers(&self) -> Option<[Form; 2]> {
        self.growth.offers()
    }

    /// The treat a hatched pet ate today: its title and what it carries,
    /// until the UTC day ends.
    pub fn fed_today(&self) -> Option<TreatKind> {
        match (self.hatched(), self.today) {
            (true, Some(today)) => state::fed_today(self.fed, today),
            (true, None) | (false, _) => None,
        }
    }

    /// The title after its name today, from the treat it ate.
    pub fn title(&self) -> Option<&'static str> {
        self.fed_today().map(state::treat_title)
    }

    /// The moment it is in and the wall tick it began on.
    pub fn moment(&self) -> Option<(Moment, usize)> {
        self.moment
    }

    pub fn plaything(&self) -> Option<Plaything> {
        self.plaything
    }

    /// What the name goes by on a status line: the name, or what it is
    /// while it has none.
    pub fn display_name(&self) -> String {
        match (&self.name, self.hatched()) {
            (Some(name), _) => name.clone(),
            (None, true) => self.species.as_str().to_string(),
            (None, false) => "egg".to_string(),
        }
    }

    fn hatched(&self) -> bool {
        match self.growth.form {
            Form::Egg => false,
            Form::Baby | Form::Young { .. } | Form::Adult { .. } | Form::Elder { .. } => true,
        }
    }

    /// What it hatched as, `None` for an egg.
    pub fn animal(&self) -> Option<PetSpecies> {
        self.hatched().then_some(self.species)
    }

    /// Send one of the owner's own commands. Dropped while another is out:
    /// the mirror does not move until the answer comes back.
    fn request(&mut self, command: PetCommand) {
        if self.busy.is_some() {
            return;
        }
        self.busy = Some(command);
        self.svc
            .act_task(self.user_id, command, self.outcome_tx.clone());
    }

    /// Send a command the session raises by itself (the day, a treat
    /// earned, the visits seen): never gated on the owner's own command.
    fn raise(&self, command: PetCommand) {
        self.svc
            .act_task(self.user_id, command, self.outcome_tx.clone());
    }

    /// A click on the pet. An egg with its days asks for the care box,
    /// where its owner picks what hatches; the first click of the UTC day
    /// is the pat that pays; after that a waiting treat is given, and a
    /// click with nothing waiting is a pat it answers with a trick it knows.
    pub fn click(&mut self, now: Instant, today: NaiveDate, in_care: bool) -> Clicked {
        let waits_on_a_pick = self.ready_to_hatch() || self.offers().is_some();
        if waits_on_a_pick && !in_care {
            return Clicked::OpenCare;
        }
        if !self.petted_on(today) {
            self.pat(now, today);
            return Clicked::Done;
        }
        match (self.hatched(), self.waiting) {
            (true, Some(_)) => self.request(PetCommand::Give),
            (true, None) | (false, _) => {
                self.note_petted(now);
                self.perform_next();
            }
        }
        Clicked::Done
    }

    /// The pat: it always purrs, and the first of the UTC day asks the
    /// service for the daily chips. The service's claim decides the
    /// payout; this only saves asking twice from one session.
    pub fn pat(&mut self, now: Instant, today: NaiveDate) {
        self.note_petted(now);
        if self.petted_on(today) {
            return;
        }
        self.last_petted = Some(today);
        self.raise(PetCommand::Pat);
    }

    pub fn petted_on(&self, today: NaiveDate) -> bool {
        self.last_petted == Some(today)
    }

    /// What the day's paid pat says: the chips, and for an egg short of its
    /// days when it hatches, since a patted egg shows nothing else for it.
    pub fn petted_line(&self) -> String {
        match self.growth.days_to_hatch() {
            Some(days) => format!(
                "The egg is warm (+{PET_CHIP_BONUS} chips): it hatches {}",
                state::hatch_wait(days)
            ),
            None => format!("Petted (+{PET_CHIP_BONUS} chips)"),
        }
    }

    /// What giving a treat says: treats grow it, so how far this one took
    /// it toward the next stage.
    pub fn fed_line(&self, kind: TreatKind) -> String {
        let ate = format!("{} ate the {} treat", self.display_name(), kind.as_str());
        match self.growth.still_needs() {
            Some((_, Need::Treats(0))) => format!("{ate}: it is ready to grow"),
            Some((next, Need::Treats(treats))) => {
                format!("{ate}: {}", state::treat_wait(next, treats))
            }
            Some((_, Need::Days(_))) | None => ate,
        }
    }

    /// Give the treat that waits, if one does.
    pub fn give(&mut self) {
        if self.hatched() && self.waiting.is_some() {
            self.request(PetCommand::Give);
        }
    }

    /// Hatch a ready egg as `species`: the pick its owner makes in the
    /// care box.
    pub fn hatch(&mut self, species: PetSpecies) {
        if self.ready_to_hatch() {
            self.request(PetCommand::Hatch(species));
        }
    }

    /// Grow into the next stage drawn with `pick`: the pick its owner
    /// makes in the care box once the pet has its treats.
    pub fn grow(&mut self, pick: Pick) {
        if self.offers().is_some() {
            self.request(PetCommand::Grow(pick));
        }
    }

    /// Throw `toy` into the box at the cell (`x`, `y`). It drops to the
    /// floor (`floor`, the box's last row); the pet goes after it on the
    /// next ticks. An egg plays with nothing.
    pub fn throw(&mut self, toy: Toy, x: usize, floor: usize) {
        if !self.hatched() {
            return;
        }
        self.plaything = Some(Plaything {
            toy,
            x,
            y: floor,
            thrown: self.animation_ticks,
            from: x,
        });
    }

    /// Move the teach row to the next trick it does not know yet.
    pub fn teach_next(&mut self) {
        let count = Trick::ALL.len();
        for step in 1..=count {
            let next = (self.care.trick + step) % count;
            if !self.growth.tricks.contains(&Trick::ALL[next]) {
                self.care.trick = next;
                return;
            }
        }
    }

    /// The trick the teach row points at, `None` once it knows them all.
    pub fn teaching(&self) -> Option<Trick> {
        let trick = Trick::ALL[self.care.trick];
        match self.growth.tricks.contains(&trick) {
            true => Trick::ALL
                .into_iter()
                .find(|trick| !self.growth.tricks.contains(trick)),
            false => Some(trick),
        }
    }

    /// A day's practice of the trick the teach row points at.
    pub fn practise(&mut self) {
        if let Some(trick) = self.teaching() {
            self.request(PetCommand::Practice(trick));
        }
    }

    /// Perform the next trick it knows, in the order it learned them.
    pub fn perform_next(&mut self) {
        let known = &self.growth.tricks;
        if known.is_empty() {
            return;
        }
        let next = match self.moment {
            Some((Moment::Trick(last), _)) => known
                .iter()
                .position(|trick| *trick == last)
                .map_or(0, |at| (at + 1) % known.len()),
            Some((Moment::Eating(_) | Moment::Welcome { .. } | Moment::Hatched, _)) | None => {
                self.animation_ticks % known.len()
            }
        };
        self.moment = Some((Moment::Trick(known[next]), self.animation_ticks));
    }

    /// The care box opened: keep who came by on show, and mark them seen.
    pub fn open_care(&mut self) {
        self.care.came_by = self.visitors.clone();
        if let Some(trick) = self.teaching() {
            self.care.trick = Trick::ALL
                .iter()
                .position(|known| *known == trick)
                .expect("every trick is in Trick::ALL");
        }
        if !self.visitors.is_empty() {
            self.raise(PetCommand::SeenVisits);
        }
    }

    /// Pay `owner_id`'s pet a visit from this session's user: a pat from
    /// its profile, or this user's own pet calling. The answer comes back
    /// as a banner on a later tick.
    pub fn visit(&mut self, owner_id: Uuid, kind: VisitKind) {
        if self.visiting {
            return;
        }
        self.visiting = true;
        self.svc
            .visit_task(self.user_id, owner_id, kind, self.outcome_tx.clone());
    }

    /// A win of the owner's in `family`: the pet is proud, and the day's
    /// treat is asked for, of the family's kind.
    pub fn note_win(&mut self, now: Instant, family: GameFamily) {
        self.signals.won = Some(now);
        if let Some(kind) = state::treat_for_win(family) {
            self.earn(kind);
        }
    }

    pub fn note_loss(&mut self, now: Instant) {
        self.signals.lost = Some(now);
    }

    pub fn note_spoke(&mut self, now: Instant) {
        self.signals.spoke = Some(now);
    }

    /// `seconds` the owner just spent, keys landing, on `screen`. Ten
    /// minutes in one door in a UTC day earn the day's treat.
    pub fn note_active_seconds(&mut self, screen: Screen, seconds: f64) {
        if !state::is_door(screen) {
            return;
        }
        let Some(today) = self.today else {
            return;
        };
        if self.door_day != Some(today) {
            self.door_day = Some(today);
            self.door_seconds.clear();
        }
        let spent = match self
            .door_seconds
            .iter_mut()
            .find(|(door, _)| *door == screen)
        {
            Some((_, spent)) => {
                *spent += seconds;
                *spent
            }
            None => {
                self.door_seconds.push((screen, seconds));
                seconds
            }
        };
        if spent >= state::DOOR_TREAT_SECONDS as f64 {
            self.earn(TreatKind::Door);
        }
    }

    /// Ask for the day's treat, of `kind`: once a day from this session
    /// and not while one waits. The service's claim decides; this only
    /// saves asking on every win.
    fn earn(&mut self, kind: TreatKind) {
        let Some(today) = self.today else {
            return;
        };
        if !self.out || self.waiting.is_some() || self.earn_asked == Some(today) {
            return;
        }
        self.earn_asked = Some(today);
        self.raise(PetCommand::Earn(kind));
    }

    /// Whether any drained change notice names this session's user. A
    /// lagged receiver may have dropped one that did, so it counts as yes.
    fn own_pet_changed(&mut self) -> bool {
        let mut own = false;
        loop {
            match self.changes_rx.try_recv() {
                Ok(user_id) => own |= user_id == self.user_id,
                Err(broadcast::error::TryRecvError::Lagged(_)) => own = true,
                Err(broadcast::error::TryRecvError::Empty)
                | Err(broadcast::error::TryRecvError::Closed) => return own,
            }
        }
    }

    /// Drain answers and change notices into the mirror.
    fn drain(&mut self, now: Instant) -> PetTicked {
        if self.own_pet_changed() {
            self.svc.reload_task(self.user_id, self.outcome_tx.clone());
        }
        let mut ticked = PetTicked::default();
        while let Ok(outcome) = self.outcome_rx.try_recv() {
            ticked.changed = true;
            let banner = match outcome {
                PetOutcome::Acted { pet, command, did } => {
                    if self.busy == Some(command) {
                        self.busy = None;
                    }
                    self.install(*pet);
                    self.acted(did, now)
                }
                PetOutcome::Reloaded { pet } => {
                    let came = self.install(*pet);
                    match came.first() {
                        Some(visit) => {
                            self.note_petted(now);
                            Some(Banner::info(&visit_line(visit, &self.display_name())))
                        }
                        None => None,
                    }
                }
                PetOutcome::Failed { command } => {
                    if self.busy == Some(command) {
                        self.busy = None;
                    }
                    self.failed(command)
                }
                PetOutcome::Visited(visited) => {
                    self.visiting = false;
                    visited_banner(visited)
                }
                PetOutcome::Switched => {
                    self.day_asked = None;
                    None
                }
            };
            if banner.is_some() {
                ticked.banner = banner;
            }
        }
        ticked
    }

    /// What an answer does to the session, and what it says.
    fn acted(&mut self, did: Did, now: Instant) -> Option<Banner> {
        let name = self.display_name();
        match did {
            Did::DayCounted(counted) => {
                // An egg missed nobody.
                let missed = counted.days_away >= 1 && self.hatched();
                if missed {
                    self.moment = Some((
                        Moment::Welcome {
                            days_away: counted.days_away,
                        },
                        self.animation_ticks,
                    ));
                    self.note_petted(now);
                }
                match missed {
                    true => Some(Banner::info(&format!(
                        "{name} is glad you are back ({})",
                        days_text(counted.days_away)
                    ))),
                    false => None,
                }
            }
            Did::Grew(stage) => {
                self.note_petted(now);
                Some(Banner::success(&format!(
                    "{name} grew up: {}",
                    stage.as_str()
                )))
            }
            Did::Hatched(species) => {
                self.moment = Some((Moment::Hatched, self.animation_ticks));
                self.note_petted(now);
                Some(Banner::success(&format!(
                    "The egg hatched: a baby {}",
                    species.as_str()
                )))
            }
            Did::Became(species) => {
                self.note_petted(now);
                Some(Banner::success(&format!(
                    "{name} is a {} now",
                    species.as_str()
                )))
            }
            Did::StartedOver => {
                // The first key after it counts the egg's first day.
                self.day_asked = None;
                Some(Banner::info("Your pet is an egg again"))
            }
            Did::Earned(kind) => Some(Banner::info(&format!(
                "A {} treat for {name}: click it to give it",
                kind.as_str()
            ))),
            Did::Gave { kind, .. } => {
                self.moment = Some((Moment::Eating(kind), self.animation_ticks));
                self.note_petted(now);
                Some(Banner::info(&self.fed_line(kind)))
            }
            Did::Practiced { trick, practiced } => match practiced {
                Practiced::Progress { days } => Some(Banner::info(&format!(
                    "{name} practised its {} (day {days} of {})",
                    trick.as_str(),
                    state::TRICK_DAYS
                ))),
                Practiced::Learned => {
                    self.moment = Some((Moment::Trick(trick), self.animation_ticks));
                    Some(Banner::success(&format!(
                        "{name} learned to {}",
                        trick.as_str()
                    )))
                }
                Practiced::AlreadyToday => Some(Banner::info("One practice a day: it is tired")),
                Practiced::Known | Practiced::Egg => None,
            },
            // The first pat's chips are told by the activity feed, to
            // every session of the owner.
            Did::Patted
            | Did::AlreadyPatted
            | Did::DayAlreadyCounted
            | Did::NotEarned(_)
            | Did::NothingToGive
            | Did::Played { .. }
            | Did::VisitsSeen => None,
            Did::Refused(refusal) => match refusal {
                Refusal::Away | Refusal::NotReady => None,
                Refusal::Egg => Some(Banner::info("It is still an egg")),
            },
        }
    }

    /// A command that never reached the row. What the session raised by
    /// itself is asked again on the next key or win; what the owner asked
    /// for says so.
    fn failed(&mut self, command: PetCommand) -> Option<Banner> {
        match command {
            PetCommand::CountDay => {
                self.day_asked = None;
                None
            }
            PetCommand::Earn(_) => {
                self.earn_asked = None;
                None
            }
            PetCommand::Pat => {
                self.last_petted = None;
                Some(Banner::error("The pet is unreachable; try again"))
            }
            PetCommand::SeenVisits => None,
            PetCommand::Hatch(_)
            | PetCommand::Grow(_)
            | PetCommand::Give
            | PetCommand::Play
            | PetCommand::Practice(_)
            | PetCommand::Species(_)
            | PetCommand::Reset => Some(Banner::error("The pet is unreachable; try again")),
        }
    }

    /// Advance the pet: drain the service's answers, count the day, read
    /// the mood, walk after the toy or the cursor. Pure animation cadence
    /// is the box's business (`ui::frame_changed`).
    pub fn tick(&mut self, input: PetTick) -> PetTicked {
        let elapsed = input.wall_tick.saturating_sub(self.animation_ticks);
        let last_tick = self.animation_ticks;
        self.animation_ticks = input.wall_tick;
        self.out = input.out;
        self.today = Some(input.today);
        let mut ticked = self.drain(input.now);
        let mut changed = ticked.changed;

        // A day together: the first key of the UTC day with the pet out.
        // The service's claim on the row decides; the session asks once.
        let key = input.ambient.last_input;
        let boot = *self.boot_input.get_or_insert(key);
        if input.out
            && key != boot
            && self.growth.last_day != Some(input.today)
            && input.now.saturating_duration_since(key) < input.day_age
            && self.day_asked != Some(input.today)
        {
            self.day_asked = Some(input.today);
            self.raise(PetCommand::CountDay);
        }

        // A stay ends on the clock, with no answer from anyone: a guest
        // whose hour is up leaves, and this pet's own stay counts down.
        let utc = input.today.and_time(NaiveTime::MIN).and_utc() + input.day_age;
        let staying = self.guests.len();
        self.guests
            .retain(|guest| state::stay_left(guest.since, utc).is_some());
        let left = match &self.calling {
            Some(stay) => state::stay_left(stay.since, utc),
            None => None,
        };
        if left.is_none() {
            self.calling = None;
        }
        if self.guests.len() != staying || left != self.calling_left {
            self.calling_left = left;
            changed = true;
        }
        // A guest blinks and flicks its tail on the wall clock, in a box
        // that is on screen.
        if input.frame.is_some()
            && self
                .guests
                .iter()
                .any(|guest| super::ui::guest_moved(guest, last_tick, input.wall_tick))
        {
            changed = true;
        }

        if let Some((moment, began)) = self.moment
            && input.wall_tick.saturating_sub(began) >= moment.ticks()
        {
            self.moment = None;
            changed = true;
        }

        let mood = mood_at(self.signals, input.ambient, input.now);
        if mood != self.mood {
            self.mood = mood;
            changed = true;
            self.svc.set_mood_task(self.user_id, mood);
        }

        // The toy: the mouse runs, a toy nobody fetched is picked up.
        let follows = mood_follows(mood) && self.hatched();
        let toy_target = match (self.plaything, input.frame) {
            (Some(toy), Some(frame)) => {
                let age = input.wall_tick.saturating_sub(toy.thrown);
                match age >= TOY_TICKS {
                    true => {
                        self.plaything = None;
                        changed = true;
                        None
                    }
                    false => {
                        let x = toy_column(toy, age, frame.zone.width as usize);
                        if x != toy.x {
                            self.plaything = Some(Plaything { x, ..toy });
                            changed = true;
                        }
                        follows.then(|| Perch {
                            x: x.saturating_sub(3).min(frame.travel.x),
                            y: frame.travel.y,
                            look: Look::Ahead,
                        })
                    }
                }
            }
            (Some(_), None) | (None, _) => None,
        };

        let target = match (toy_target, follows, input.frame, input.cursor) {
            (Some(toy), _, _, _) => Some(toy),
            (None, true, Some(frame), Some(cursor)) => cursor_target(frame, cursor),
            (None, true, _, _) | (None, false, _, _) => None,
        };
        // Off the stroll the pet only ever walks: one cell per animation
        // edge on each axis, from wherever it stood. After the toy or the
        // cursor while there is one in the box; back to where the stroll is
        // once there is not, purring or not (a pet that holds still where
        // it was petted reads as stuck, and one that jumps back reads as a
        // glitch). The stroll takes over when the pet is right beside it.
        // The stroll paints on every second wall tick; the walk keeps that
        // pace whatever the loop's cadence.
        let cells = elapsed.div_ceil(2);
        let perch = match (target, self.perch, input.frame) {
            (Some(target), from, Some(frame)) => {
                let (x, y) = from.map_or(frame.position, |perch| (perch.x, perch.y));
                Some(Perch {
                    x: step_toward(x, target.x, cells),
                    y: step_toward(y, target.y, cells),
                    look: match toy_target {
                        Some(_) => look_toward(x, target.x),
                        None => target.look,
                    },
                })
            }
            (None, Some(from), Some(frame)) => {
                let (home_x, home_y) = frame.home;
                match from.x.abs_diff(home_x) <= 1 && from.y.abs_diff(home_y) <= 1 {
                    true => None,
                    false => Some(Perch {
                        x: step_toward(from.x, home_x, cells),
                        y: step_toward(from.y, home_y, cells),
                        look: look_toward(from.x, home_x),
                    }),
                }
            }
            (None, None, Some(_)) | (Some(_), _, None) | (None, _, None) => None,
        };
        if perch != self.perch {
            self.perch = perch;
            changed = true;
        }
        // Fetched: the pet stands on the toy. The first play of the UTC
        // day is a bond day, claimed by the service.
        if let (Some(toy), Some(at)) = (toy_target, self.perch)
            && at.x == toy.x
            && at.y == toy.y
        {
            self.plaything = None;
            self.note_petted(input.now);
            changed = true;
            if self.growth.last_bond_day != Some(input.today) {
                self.raise(PetCommand::Play);
            }
        }
        ticked.changed = changed;
        ticked
    }
}

/// The column a toy is at `age` wall ticks after it was thrown, in a box
/// `width` cells wide. A ball lies where it landed; the mouse runs the
/// floor end to end at half the pet's pace, so the pet catches it on a
/// turn.
fn toy_column(toy: Plaything, age: usize, width: usize) -> usize {
    match toy.toy {
        Toy::Ball => toy.x,
        Toy::Mouse => {
            let span = width.saturating_sub(1);
            if span == 0 {
                return 0;
            }
            let run = (toy.from + age / 4) % (2 * span);
            match run <= span {
                true => run,
                false => 2 * span - run,
            }
        }
    }
}

/// How a visit reads to the pet's owner.
pub fn visit_line(visit: &PetVisitor, pet: &str) -> String {
    match visit.kind {
        VisitKind::Pat => format!("{} came by to pet {pet}", visit.username),
        VisitKind::Call => format!("{}'s pet came to stay with {pet}", visit.username),
    }
}

fn days_text(days: i64) -> String {
    match days {
        1 => "a day away".to_string(),
        days => format!("{days} days away"),
    }
}

/// What a visit this session's user paid someone else's pet says.
fn visited_banner(visited: Visited) -> Option<Banner> {
    match visited {
        Visited::Patted { pet } => Some(Banner::success(&format!("You petted {pet}"))),
        Visited::Called { caller, host, line } => Some(Banner::success(&format!(
            "{caller} went to stay with {host} for an hour: {line}"
        ))),
        Visited::AlreadyToday(VisitKind::Pat) => Some(Banner::info("You already petted it today")),
        Visited::AlreadyToday(VisitKind::Call) => {
            Some(Banner::info("Your pet already went calling today"))
        }
        Visited::NobodyHome => Some(Banner::info("No pet is out here")),
        Visited::NobodyToSend => Some(Banner::info("You have no hatched pet out to send")),
        Visited::OwnPet => None,
        Visited::Failed => Some(Banner::error("The pet is unreachable; try again")),
    }
}

/// A sulking or sleeping pet does not come when called.
fn mood_follows(mood: PetMood) -> bool {
    match mood {
        PetMood::Purring | PetMood::Proud | PetMood::Chatty | PetMood::Vibing | PetMood::Idle => {
            true
        }
        PetMood::Sulking | PetMood::Asleep => false,
    }
}

/// Where the pet should stand to sit under the cursor, when the cursor is
/// inside the box the pet was last drawn in; the face lands on the cursor
/// column and the eyes point at it on the way.
fn cursor_target(frame: PetFrameInputs, cursor: (u16, u16)) -> Option<Perch> {
    let zone = frame.zone;
    let (cx, cy) = cursor;
    if !zone.contains(Position { x: cx, y: cy }) {
        return None;
    }
    // The face is three cells in from the pet's left edge.
    let x = (usize::from(cx - zone.x))
        .saturating_sub(3)
        .min(frame.travel.x);
    let y = (usize::from(cy - zone.y))
        .saturating_sub(1)
        .min(frame.travel.y);
    let face = frame.position.0 + 3;
    let look = if usize::from(cx - zone.x) < face {
        Look::Left
    } else if usize::from(cx - zone.x) > face + 1 {
        Look::Right
    } else {
        Look::Ahead
    };
    Some(Perch { x, y, look })
}

/// Which way the pet faces while walking from `from` to `to`.
fn look_toward(from: usize, to: usize) -> Look {
    match from.cmp(&to) {
        std::cmp::Ordering::Less => Look::Right,
        std::cmp::Ordering::Equal => Look::Ahead,
        std::cmp::Ordering::Greater => Look::Left,
    }
}

fn step_toward(from: usize, to: usize, cells: usize) -> usize {
    if from < to {
        (from + cells).min(to)
    } else {
        from.saturating_sub(cells).max(to)
    }
}

#[cfg(test)]
#[path = "session_test.rs"]
mod session_test;
