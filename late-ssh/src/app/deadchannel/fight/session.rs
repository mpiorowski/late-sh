//! A session's side of the fight: a mirror of the sheet to draw from, the
//! picker and the scene open over the street, and the requests that ask
//! `FightService` to change the row. Nothing here decides anything: a key
//! press becomes a command, and the bars move when the service's answer
//! arrives. The picker's threat words are the sim's odds over the mirror
//! (`sim::odds`), read once when it opens and again when the mirror moves,
//! never per frame.

use tokio::sync::mpsc;
use uuid::Uuid;

use super::sim::{ODDS_FIGHTS, Threat, odds};
use super::state::{Applied, Command, Pick, Quarry, Sheet};
use super::svc::{FightOutcome, FightService};

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

/// The picker over the street, before a step in: the runner's sheet and
/// the glyphs on offer, each with its threat word.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Picker {
    /// The row under the cursor.
    pub cursor: Pick,
    /// The threat of each pick from the mirror as it stands; `None` when
    /// that step in would not start a fight (the signal down, the rations
    /// spent, nothing below the flicker) or before the mirror landed.
    pub fair: Option<Threat>,
    pub lower: Option<Threat>,
    /// `None` on a step no bright glyph waits behind, too.
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
    /// Crate-visible so a test can put an answer on the wire the way the
    /// service would.
    pub(crate) outcome_tx: mpsc::UnboundedSender<FightOutcome>,
    outcome_rx: mpsc::UnboundedReceiver<FightOutcome>,
}

impl FightSession {
    pub(crate) fn new(user_id: Uuid, username: String, svc: FightService) -> Self {
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
    /// opens a menu over a live fight. Otherwise the picker opens, and the
    /// sheet re-reads so it shows today's bars.
    pub(crate) fn step_up(&mut self) {
        let waiting = self
            .sheet
            .as_ref()
            .is_some_and(|sheet| sheet.fight.is_some());
        if waiting {
            self.step_in(Pick::Fair);
            return;
        }
        self.picker = Some(Picker {
            cursor: Pick::Fair,
            fair: None,
            lower: None,
            bright: None,
        });
        self.read_odds();
        self.reload();
    }

    /// Step in with `pick`: close the picker, open the scene, ask the row.
    /// The row decides: a fight it has waiting resumes whatever the pick,
    /// and a refusal lands on the scene.
    pub(crate) fn step_in(&mut self, pick: Pick) {
        self.picker = None;
        self.scene = Some(Scene {
            lines: Vec::new(),
            latest: 0,
            over: false,
            waiting: true,
            old_signal: false,
            failed: false,
        });
        self.request(Command::Start { pick });
    }

    /// A step-in key on the picker (`f`, `g`, `b`, Enter). When the
    /// mirror shows nothing on offer (the signal down, the rations spent:
    /// the picker is showing the reason instead of glyphs) the key closes
    /// it; a scene opened only to repeat that reason is one Enter too
    /// many. A pick the picker is not offering is ignored. Otherwise it
    /// steps in, and the row decides.
    pub(crate) fn choose(&mut self, pick: Pick) {
        let shut = self
            .sheet
            .as_ref()
            .is_some_and(|sheet| sheet.shut().is_some());
        match (shut, self.offers().contains(&pick)) {
            (true, _) => self.close(),
            (false, true) => self.step_in(pick),
            (false, false) => {}
        }
    }

    /// The picks the picker offers, top to bottom, from the mirror: the
    /// bright glyph on a step one waits behind, the fair fight always,
    /// the step down above level 1. With no mirror yet, the fair fight
    /// alone.
    pub(crate) fn offers(&self) -> Vec<Pick> {
        let mut offers = Vec::with_capacity(3);
        if self.sheet.as_ref().is_some_and(Sheet::bright_waits) {
            offers.push(Pick::Bright);
        }
        offers.push(Pick::Fair);
        if self.sheet.as_ref().is_some_and(|sheet| sheet.level > 1) {
            offers.push(Pick::Lower);
        }
        offers
    }

    /// The picker's cursor, a row up or down the offers; the ends hold.
    pub(crate) fn pick_up(&mut self) {
        self.move_cursor(-1);
    }

    pub(crate) fn pick_down(&mut self) {
        self.move_cursor(1);
    }

    fn move_cursor(&mut self, by: isize) {
        let offers = self.offers();
        let Some(picker) = &mut self.picker else {
            return;
        };
        // A cursor on a pick no longer offered (the mirror moved under
        // it) counts as on the fair fight.
        let fair = offers
            .iter()
            .position(|pick| *pick == Pick::Fair)
            .expect("the fair fight is always offered");
        let at = offers
            .iter()
            .position(|pick| *pick == picker.cursor)
            .unwrap_or(fair);
        let to = at.saturating_add_signed(by).min(offers.len() - 1);
        picker.cursor = offers[to];
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

    /// The picker's threat words from the mirror as it stands, and the
    /// cursor back on the fair fight if the mirror moved the pick it was
    /// on off the offers.
    fn read_odds(&mut self) {
        let offers = self.offers();
        let Some(picker) = &mut self.picker else {
            return;
        };
        if !offers.contains(&picker.cursor) {
            picker.cursor = Pick::Fair;
        }
        let threat = |pick| {
            self.sheet
                .as_ref()
                .and_then(|sheet| odds(sheet, pick, ODDS_FIGHTS))
                .map(Threat::of)
        };
        picker.fair = threat(Pick::Fair);
        picker.lower = threat(Pick::Lower);
        picker.bright = threat(Pick::Bright);
    }

    /// Stepping up to the counter again: the last word is not repeated.
    pub(crate) fn clear_till(&mut self) {
        self.till = None;
    }

    /// Ask the service to run `command`. One action is out at a time: every
    /// `act_task` holds a pooled connection while it waits for the row
    /// lock, so a held key must not queue one per repeat. A press made
    /// while an answer is pending is dropped.
    pub(crate) fn request(&mut self, command: Command) -> bool {
        if self.action_in_flight {
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
                    match &mut self.scene {
                        Some(scene) => {
                            scene.waiting = false;
                            scene.failed = true;
                            scene.latest = 1;
                            scene
                                .lines
                                .push("the static is not answering. try again.".to_string());
                        }
                        None => {
                            self.till =
                                Some("nobody at the counter is answering. try again.".to_string());
                        }
                    }
                }
            }
        }
        changed
    }

    /// Put an answer where it was asked for: on the scene when one is
    /// open, else at the till. A resumed fight shows the row's memory of
    /// it; a started one begins fresh; everything else appends.
    fn show(&mut self, applied: Applied, lines: Vec<String>) {
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
            Applied::Refused(_) => {
                scene.lines = lines;
                scene.over = true;
            }
            Applied::Round => scene.lines.extend(lines),
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
            | Applied::Carted { .. } => scene.lines.extend(lines),
        }
        if scene.lines.len() > SCENE_KEEP {
            let drop = scene.lines.len() - SCENE_KEEP;
            scene.lines.drain(..drop);
        }
    }
}

#[cfg(test)]
#[path = "session_test.rs"]
mod session_test;
