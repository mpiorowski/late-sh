//! A session's side of the fight: a mirror of the sheet to draw from, the
//! scene open over the street, and the requests that ask `FightService`
//! to change the row. Nothing here decides anything: a key press becomes
//! a command, and the bars move when the service's answer arrives.

use tokio::sync::mpsc;
use uuid::Uuid;

use super::state::{Applied, Command, Sheet};
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
}

pub(crate) struct FightSession {
    /// The mirror: `None` until the first reload answers, or when there is
    /// no standing runner.
    pub sheet: Option<Sheet>,
    pub scene: Option<Scene>,
    /// The armorer's last word: the answer to a command sent with no
    /// scene open (a purchase, or its refusal), shown in the panel.
    pub till: Option<String>,
    user_id: Uuid,
    username: String,
    svc: FightService,
    action_in_flight: bool,
    outcome_tx: mpsc::UnboundedSender<FightOutcome>,
    outcome_rx: mpsc::UnboundedReceiver<FightOutcome>,
}

impl FightSession {
    pub(crate) fn new(user_id: Uuid, username: String, svc: FightService) -> Self {
        let (outcome_tx, outcome_rx) = mpsc::unbounded_channel();
        Self {
            sheet: None,
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

    /// Re-read the sheet (the descent into the city).
    pub(crate) fn reload(&mut self) {
        self.svc.reload_task(self.user_id, self.outcome_tx.clone());
    }

    /// Step into the static: open the scene and ask for a fight.
    pub(crate) fn open(&mut self) {
        self.scene = Some(Scene {
            lines: Vec::new(),
            latest: 0,
            over: false,
            waiting: true,
        });
        self.request(Command::Start);
    }

    /// Back to the street. A fight in progress stays on the row and is
    /// found waiting on the next step in.
    pub(crate) fn close(&mut self) {
        self.scene = None;
    }

    pub(crate) fn scene_open(&self) -> bool {
        self.scene.is_some()
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
                }
                FightOutcome::Reloaded { sheet } => {
                    self.sheet = Some(sheet);
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
                            scene.latest = 1;
                            scene
                                .lines
                                .push("the static is not answering. try again.".to_string());
                        }
                        None => {
                            self.till =
                                Some("the armorer is not answering. try again.".to_string());
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
        match applied {
            Applied::Started => scene.lines = lines,
            Applied::Resumed => {
                scene.lines = self
                    .sheet
                    .as_ref()
                    .and_then(|sheet| sheet.fight.as_ref())
                    .map(|fight| fight.log.clone())
                    .unwrap_or_default();
                scene.latest = scene.lines.len();
            }
            Applied::Refused(_) => {
                scene.lines = lines;
                scene.over = true;
            }
            Applied::Round => scene.lines.extend(lines),
            Applied::Won { .. } | Applied::Lost { .. } | Applied::Escaped => {
                scene.lines.extend(lines);
                scene.over = true;
            }
            // The till is never asked from inside the scene; an answer
            // that lands here anyway is shown, not lost.
            Applied::Outfitted { .. } => scene.lines.extend(lines),
        }
        if scene.lines.len() > SCENE_KEEP {
            let drop = scene.lines.len() - SCENE_KEEP;
            scene.lines.drain(..drop);
        }
    }
}
