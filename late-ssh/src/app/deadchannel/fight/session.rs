//! A session's side of the fight: a mirror of the sheet to draw from, the
//! road and the scene open over the street, and the requests that ask
//! `FightService` to change the row. Nothing here decides anything: a key
//! press becomes a command, and the bars move when the service's answer
//! arrives. The road's threat words are the sim's odds over the mirror
//! (`sim::odds`), read once when it opens and again when the mirror moves,
//! never per frame.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use tokio::sync::mpsc;
use uuid::Uuid;

use super::cards::Draft;
use super::road::Node;
use super::sim::{ODDS_FIGHTS, Threat, odds};
use super::state::{Applied, Call, Command, Pick, Quarry, Refusal, Sheet};
use super::svc::{FightOutcome, FightService};

/// What every command answers on a process draining for a deploy.
const DRAINING_LINE: &str = "the city is moving under you. reconnect to keep going.";

/// Lines of the exchange the scene shows.
const SCENE_KEEP: usize = 10;

/// The fight panel over the street: what it says, and whether the fight
/// is over (Enter closes it) or waiting on a key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scene {
    pub lines: Vec<String>,
    /// How many of the last `lines` came with the latest answer: the
    /// renderer brightens those and dims the rest.
    pub latest: usize,
    pub over: bool,
    /// A command is out and its answer has not landed.
    pub waiting: bool,
    /// The last answer was the service failing (`ActionFailed`): the
    /// static is not answering. Esc closes the scene then instead of
    /// running, so an outage never traps the runner in retries; the fight
    /// stays on the row. Cleared by the next request.
    pub failed: bool,
    /// The fight on the row is the Old Signal's: the scene is drawn its
    /// way (`ui.rs`, the whole screen, the frame torn). Set when the
    /// fight starts or resumes, kept until the scene closes, so the
    /// aftermath is drawn on the same screen the fight was.
    pub old_signal: bool,
}

/// The road over the street, before a step: the runner's sheet, the
/// day's road with the run on it, and what waits on the lane under the
/// cursor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Picker {
    /// The lane of the next step under the cursor.
    pub lane: u8,
    /// The threat of each pick from the mirror as it stands (its signal,
    /// its kit, the static in its deck); `None` when no fight would start
    /// (the signal down, the rations spent, nothing below the flicker),
    /// when no lane the next step reaches holds its node (a rest or a
    /// cache shows no threat, and a draft owed is shown ahead of any
    /// step), or before the mirror landed.
    pub fair: Option<Threat>,
    pub lower: Option<Threat>,
    pub bright: Option<Threat>,
}

pub(crate) struct FightSession {
    /// The mirror: `None` until the first reload answers, or when there is
    /// no standing runner.
    pub sheet: Option<Sheet>,
    pub picker: Option<Picker>,
    pub scene: Option<Scene>,
    /// The counter's last word: the answer to a command sent with no
    /// scene open (a purchase or a patch, or its refusal), shown in the
    /// panel that asked.
    pub till: Option<String>,
    user_id: Uuid,
    username: String,
    svc: FightService,
    action_in_flight: bool,
    /// The process is shutting down for a deploy (`State::is_draining`):
    /// a newer one owns the rows, and this one stops changing them.
    is_draining: Arc<AtomicBool>,
    /// Crate-visible so a test can put an answer on the wire the way the
    /// service would.
    pub(crate) outcome_tx: mpsc::UnboundedSender<FightOutcome>,
    outcome_rx: mpsc::UnboundedReceiver<FightOutcome>,
}

impl FightSession {
    pub(crate) fn new(
        user_id: Uuid,
        username: String,
        svc: FightService,
        is_draining: Arc<AtomicBool>,
    ) -> Self {
        let (outcome_tx, outcome_rx) = mpsc::unbounded_channel();
        Self {
            sheet: None,
            picker: None,
            scene: None,
            till: None,
            user_id,
            username,
            svc,
            action_in_flight: false,
            is_draining,
            outcome_tx,
            outcome_rx,
        }
    }

    /// Re-read the sheet: at connect and on every directory edge for a
    /// standing runner, and on the descent into the city.
    pub(crate) fn reload(&mut self) {
        self.svc.reload_task(self.user_id, self.outcome_tx.clone());
    }

    /// The runner left the row: nothing to mirror until they are back.
    pub(crate) fn drop_sheet(&mut self) {
        self.sheet = None;
    }

    /// Walk up to the static (`f`, or Enter at the screen). A fight the
    /// mirror shows waiting goes straight back in: a dropped session never
    /// opens a map over a live fight. Otherwise the road opens, and the
    /// sheet re-reads so it shows today's bars and today's road.
    pub(crate) fn step_up(&mut self) {
        let waiting = self
            .sheet
            .as_ref()
            .is_some_and(|sheet| sheet.fight.is_some());
        if waiting {
            self.resume();
            return;
        }
        self.open_road();
        self.till = None;
        self.reload();
    }

    /// The road over the street, the cursor on the lane the runner stands
    /// in (or the middle one, before the first step).
    fn open_road(&mut self) {
        let lane = self
            .sheet
            .as_ref()
            .and_then(|sheet| sheet.road.lane())
            .unwrap_or(1);
        self.scene = None;
        self.picker = Some(Picker {
            lane,
            fair: None,
            lower: None,
            bright: None,
        });
        self.read_odds();
    }

    /// Back into the fight the mirror shows waiting on the row. If the
    /// mirror was stale and nothing waits, the row says so on the scene
    /// and nothing is spent.
    fn resume(&mut self) {
        self.open_scene();
        self.request(Command::Resume);
    }

    fn open_scene(&mut self) {
        self.picker = None;
        self.scene = Some(Scene {
            lines: Vec::new(),
            latest: 0,
            over: false,
            waiting: true,
            old_signal: false,
            failed: false,
        });
    }

    /// What waits on the lane under the cursor, from the mirror.
    pub(crate) fn node_ahead(&self) -> Option<Node> {
        let picker = self.picker.as_ref()?;
        self.sheet.as_ref()?.node_ahead(picker.lane)
    }

    /// A step on the road (Enter, or `g`). With the road over for the day
    /// (spent, or the signal down: the panel is showing the day's card
    /// instead of a step) the key closes it. A call the node under the
    /// cursor does not answer is ignored, so a key never spends a ration
    /// on a refusal. A fight opens the scene; a rest or a cache is taken
    /// where the runner stands, and the road stays open on the next step.
    fn call(&mut self, call: Call) {
        let over = self.sheet.as_ref().is_some_and(Sheet::road_over);
        if over {
            self.close();
            return;
        }
        // A draft owed is answered first: the panel is showing the two
        // cards, not a step, and the row would refuse one.
        if self.draft().is_some() {
            return;
        }
        let Some(lane) = self.picker.as_ref().map(|picker| picker.lane) else {
            return;
        };
        let answers = match (self.node_ahead(), call) {
            (Some(Node::Glyph), Call::Fight(Pick::Fair | Pick::Lower)) => true,
            (Some(Node::Bright), Call::Fight(Pick::Bright)) => true,
            (Some(Node::Rest), Call::Clear) => true,
            (Some(Node::Cache), Call::Take) => true,
            (Some(Node::Glyph | Node::Bright | Node::Rest | Node::Cache), _) => false,
            // No mirror yet: the row decides.
            (None, _) => true,
        };
        if !answers {
            return;
        }
        match call {
            Call::Fight(_) => {
                self.open_scene();
                self.request(Command::Step { lane, call });
            }
            Call::Clear | Call::Take => {
                self.request(Command::Step { lane, call });
            }
        }
    }

    /// The draft the mirror shows owed: the road panel offers it ahead of
    /// any step.
    pub(crate) fn draft(&self) -> Option<&'static Draft> {
        self.sheet.as_ref().and_then(Sheet::draft)
    }

    /// `1` or `2` on a draft: take that option. Nothing is sent with no
    /// draft owed, so the keys stay the page keys everywhere else.
    pub(crate) fn take_card(&mut self, option: usize) -> bool {
        let Some(card) = self
            .draft()
            .and_then(|draft| draft.options.get(option).copied())
        else {
            return false;
        };
        self.request(Command::Draft { card });
        true
    }

    /// Enter on the road: the one thing the node under the cursor is
    /// for. A glyph or a bright one is fought, a rest clears the deck, a
    /// cache is taken.
    pub(crate) fn enter(&mut self) {
        let call = match self.node_ahead() {
            Some(Node::Glyph) | None => Call::Fight(Pick::Fair),
            Some(Node::Bright) => Call::Fight(Pick::Bright),
            Some(Node::Rest) => Call::Clear,
            Some(Node::Cache) => Call::Take,
        };
        self.call(call);
    }

    /// `g` on the road: the glyph a level down, on a glyph's node only.
    pub(crate) fn step_down(&mut self) {
        self.call(Call::Fight(Pick::Lower));
    }

    /// The lanes the next step can reach, top to bottom, from the mirror;
    /// every lane before it lands.
    pub(crate) fn open_lanes(&self) -> Vec<u8> {
        match &self.sheet {
            Some(sheet) => sheet.road.open_lanes(),
            None => (0..super::road::LANES as u8).collect(),
        }
    }

    /// The road's cursor, a lane up or down the open ones; the ends hold.
    pub(crate) fn pick_up(&mut self) {
        self.move_cursor(-1);
    }

    pub(crate) fn pick_down(&mut self) {
        self.move_cursor(1);
    }

    fn move_cursor(&mut self, by: isize) {
        let lanes = self.open_lanes();
        let Some(picker) = &mut self.picker else {
            return;
        };
        // A cursor on a lane no longer open (the mirror moved under it)
        // counts as on the nearest one.
        let at = nearest(&lanes, picker.lane);
        let to = at.saturating_add_signed(by).min(lanes.len() - 1);
        picker.lane = lanes[to];
    }

    /// Enter on a finished scene: back to the road, where the next step
    /// (or the day's card) is waiting.
    pub(crate) fn leave_scene(&mut self) {
        self.till = None;
        self.open_road();
    }

    /// Back to the street: the picker, a finished scene, or the page left
    /// under an open one. A fight still on stays on the row and is found
    /// waiting on the next step in (Esc over the scene is the run, not
    /// this).
    pub(crate) fn close(&mut self) {
        self.picker = None;
        self.scene = None;
    }

    pub(crate) fn scene_open(&self) -> bool {
        self.scene.is_some()
    }

    pub(crate) fn picker_open(&self) -> bool {
        self.picker.is_some()
    }

    /// The road's threat words from the mirror as it stands, and the
    /// cursor onto the nearest open lane if the mirror moved the one it
    /// was on out of reach. Only the fights the panel can show are played
    /// out: the nodes on the lanes the next step reaches, and none of
    /// them while a draft is on the panel instead of a step.
    fn read_odds(&mut self) {
        let lanes = self.open_lanes();
        let Some(picker) = &mut self.picker else {
            return;
        };
        picker.lane = lanes[nearest(&lanes, picker.lane)];
        let ahead: Vec<Node> = match &self.sheet {
            Some(sheet) if sheet.draft().is_none() => lanes
                .iter()
                .filter_map(|lane| sheet.node_ahead(*lane))
                .collect(),
            Some(_) | None => Vec::new(),
        };
        let threat = |node, pick| match ahead.contains(&node) {
            true => self
                .sheet
                .as_ref()
                .and_then(|sheet| odds(sheet, pick, ODDS_FIGHTS))
                .map(Threat::of),
            false => None,
        };
        picker.fair = threat(Node::Glyph, Pick::Fair);
        picker.lower = threat(Node::Glyph, Pick::Lower);
        picker.bright = threat(Node::Bright, Pick::Bright);
    }

    /// Stepping up to the counter again: the last word is not repeated.
    pub(crate) fn clear_till(&mut self) {
        self.till = None;
    }

    /// Ask the service to run `command`. One action is out at a time: every
    /// `act_task` holds a pooled connection while it waits for the row
    /// lock, so a held key must not queue one per repeat. A press made
    /// while an answer is pending is dropped. On a draining process
    /// nothing is sent at all: every command in the undercity (a step, a
    /// card, a counter) answers with the word to reconnect.
    pub(crate) fn request(&mut self, command: Command) -> bool {
        if self.action_in_flight {
            return false;
        }
        if self.is_draining.load(Ordering::Relaxed) {
            self.unanswered(DRAINING_LINE, DRAINING_LINE);
            return false;
        }
        self.action_in_flight = true;
        if let Some(scene) = &mut self.scene {
            scene.waiting = true;
            scene.failed = false;
        }
        self.svc.act_task(
            self.user_id,
            self.username.clone(),
            command,
            self.outcome_tx.clone(),
        );
        true
    }

    /// Drain answers. Returns true when the mirror or the scene moved and
    /// the frame needs repainting.
    pub(crate) fn tick(&mut self) -> bool {
        let mut changed = false;
        while let Ok(outcome) = self.outcome_rx.try_recv() {
            changed = true;
            match outcome {
                FightOutcome::Acted { sheet, outcome } => {
                    self.action_in_flight = false;
                    self.sheet = Some(sheet);
                    self.show(outcome.applied, outcome.lines);
                    self.read_odds();
                }
                FightOutcome::Reloaded { sheet } => {
                    self.sheet = Some(sheet);
                    self.read_odds();
                }
                FightOutcome::NoRunner => {
                    self.action_in_flight = false;
                    self.sheet = None;
                    self.close();
                }
                FightOutcome::ActionFailed => {
                    self.action_in_flight = false;
                    self.unanswered(
                        "the static is not answering. try again.",
                        "nobody at the counter is answering. try again.",
                    );
                }
            }
        }
        changed
    }

    /// A command that got no answer from the row: the word on the scene
    /// when one is open (marked `failed`, so Esc leaves it), else at the
    /// till.
    fn unanswered(&mut self, on_scene: &str, at_till: &str) {
        match &mut self.scene {
            Some(scene) => {
                scene.waiting = false;
                scene.failed = true;
                scene.latest = 1;
                scene.lines.push(on_scene.to_string());
            }
            None => self.till = Some(at_till.to_string()),
        }
    }

    /// Put an answer where it was asked for: on the scene when one is
    /// open, else at the till (a counter's panel, or the road, which shows
    /// it as the last step's word). A resumed fight shows the row's memory
    /// of it, opening the scene if none was; a started one begins fresh;
    /// everything else appends.
    fn show(&mut self, applied: Applied, lines: Vec<String>) {
        // A step sent from the road (a rest, a cache) that found a fight
        // on the row the mirror had not seen: the fight gets its scene.
        if self.scene.is_none() && applied == Applied::Resumed {
            self.open_scene();
        }
        let Some(scene) = &mut self.scene else {
            self.till = Some(lines.join(" "));
            return;
        };
        scene.waiting = false;
        scene.latest = lines.len();
        let fight = self.sheet.as_ref().and_then(|sheet| sheet.fight.as_ref());
        match applied {
            Applied::Started { .. } => {
                scene.lines = lines;
                scene.old_signal = fight.is_some_and(|fight| fight.quarry == Quarry::OldSignal);
            }
            Applied::Resumed => {
                scene.lines = fight.map(|fight| fight.log.clone()).unwrap_or_default();
                scene.latest = scene.lines.len();
                scene.old_signal = fight.is_some_and(|fight| fight.quarry == Quarry::OldSignal);
            }
            // A card the turn cannot pay for, or a slot with nothing in
            // it: the fight is still on, and the line says why.
            Applied::Refused(Refusal::NoEnergy | Refusal::NoCard) => scene.lines.extend(lines),
            Applied::Refused(_) => {
                scene.lines = lines;
                scene.over = true;
            }
            Applied::Played { .. } | Applied::Round | Applied::Drafted { .. } => {
                scene.lines.extend(lines)
            }
            Applied::Won { .. }
            | Applied::Slain { .. }
            | Applied::Lost { .. }
            | Applied::Escaped => {
                scene.lines.extend(lines);
                scene.over = true;
            }
            // The till is never asked from inside the scene; an answer
            // that lands here anyway is shown, not lost.
            Applied::Outfitted { .. }
            | Applied::Patched { .. }
            | Applied::Deposited { .. }
            | Applied::Withdrew { .. }
            | Applied::Borrowed { .. }
            | Applied::Repaid { .. }
            | Applied::Reset
            | Applied::Drank { .. }
            | Applied::Carted { .. }
            | Applied::Cleared { .. }
            | Applied::Cached { .. } => scene.lines.extend(lines),
        }
        if scene.lines.len() > SCENE_KEEP {
            let drop = scene.lines.len() - SCENE_KEEP;
            scene.lines.drain(..drop);
        }
    }
}

/// The index in `lanes` of the lane nearest `lane`; the first of two as
/// near.
fn nearest(lanes: &[u8], lane: u8) -> usize {
    lanes
        .iter()
        .enumerate()
        .min_by_key(|(_, open)| open.abs_diff(lane))
        .map(|(index, _)| index)
        .expect("a road always has a lane open")
}

#[cfg(test)]
#[path = "session_test.rs"]
mod session_test;
