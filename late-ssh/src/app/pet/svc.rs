use anyhow::{Context, Result};
use chrono::{NaiveDate, Utc};
use late_core::db::Db;
use late_core::models::chips::{ChipMove, UserChips};
use late_core::models::pet::{
    Fed, Form, PetCompanion, PetDay, PetGuest, PetMood, PetSpecies, PetStay, PetVisitor, Pick,
    Stage, TreatEarned, TreatKind, Trick, VisitKind,
};
use tokio::sync::{broadcast, mpsc};
use tracing::{Instrument, info_span};
use uuid::Uuid;

use crate::app::activity::event::ActivityEvent;
use crate::app::pet::state::{DayCounted, Growth, Practiced, call_line, fed_today, stays_since};
use crate::app::pet::ui::Portrait;
use crate::pg_listener::{Channel, Signal};

/// What the first pet of the UTC day pays.
pub(crate) const PET_CHIP_BONUS: i64 = 100;

/// One thing an owner's session asks of its pet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PetCommand {
    /// A click on it: the first of the UTC day pays the chips.
    Pat,
    /// The owner pressed a key today with the pet out.
    CountDay,
    /// Hatch the egg as the animal its owner picked.
    Hatch(PetSpecies),
    /// Enter the next stage drawn with the pick its owner made.
    Grow(Pick),
    /// A win, or a door's minutes, asks for the day's treat: the kind is
    /// the family of games it came from.
    Earn(TreatKind),
    /// Give the treat that waits.
    Give,
    /// A toy or a ball: the first play of the day is a bond day.
    Play,
    Practice(Trick),
    /// The owner looked at who came by.
    SeenVisits,
    /// Turn a hatched pet into another animal (`/pet cat`).
    Species(PetSpecies),
    /// Start over as an egg (`/pet reset yes`).
    Reset,
}

impl PetCommand {
    pub(crate) fn action(self) -> PetAction {
        match self {
            PetCommand::Pat => PetAction::Pat,
            PetCommand::CountDay => PetAction::CountDay,
            PetCommand::Hatch(_) => PetAction::Hatch,
            PetCommand::Grow(_) => PetAction::Grow,
            PetCommand::Earn(_) => PetAction::Earn,
            PetCommand::Give => PetAction::Give,
            PetCommand::Play => PetAction::Play,
            PetCommand::Practice(_) => PetAction::Practice,
            PetCommand::SeenVisits => PetAction::SeenVisits,
            PetCommand::Species(_) => PetAction::Species,
            PetCommand::Reset => PetAction::Reset,
        }
    }
}

/// Every action the service runs, for the metric label.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PetAction {
    Pat,
    CountDay,
    Hatch,
    Grow,
    Earn,
    Give,
    Play,
    Practice,
    SeenVisits,
    Species,
    Reset,
    /// A visitor's pat.
    VisitPat,
    /// A visitor's pet calling.
    VisitCall,
}

/// How one action settled, for the metric label.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PetActionResult {
    Stored,
    Refused,
    Failed,
}

/// A treat's beats, for the metric label.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreatBeat {
    Earned,
    Given,
    /// Asked for and not earned: the day has its treat, or one still waits.
    Refused,
}

/// A visit's beats, for the metric label.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisitBeat {
    Pat,
    Call,
    Refused,
}

/// Why a command did nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Refusal {
    /// The owner put it away. Only a session that has not heard of the
    /// switch yet gets here.
    Away,
    /// It does not have the days (an egg) or the treats (a hatched pet)
    /// for the next stage yet, or the pick is
    /// for a stage it is not at (a hatch for a hatched pet, a drawing for
    /// an egg or an elder).
    NotReady,
    /// An egg eats nothing, plays with nothing, learns nothing, and is no
    /// animal yet.
    Egg,
}

/// What one command did to the pet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Did {
    /// The first pat of the day: the chips are paid.
    Patted,
    AlreadyPatted,
    DayCounted(DayCounted),
    DayAlreadyCounted,
    Hatched(PetSpecies),
    /// It entered this stage.
    Grew(Stage),
    Earned(TreatKind),
    /// The day has its treat already, or one still waits.
    NotEarned(TreatKind),
    Gave {
        kind: TreatKind,
        bonded: bool,
    },
    NothingToGive,
    Played {
        bonded: bool,
    },
    Practiced {
        trick: Trick,
        practiced: Practiced,
    },
    VisitsSeen,
    Became(PetSpecies),
    StartedOver,
    Refused(Refusal),
}

impl Did {
    fn result(self) -> PetActionResult {
        match self {
            Did::Patted
            | Did::DayCounted(_)
            | Did::Hatched(_)
            | Did::Grew(_)
            | Did::Earned(_)
            | Did::Gave { .. }
            | Did::Played { .. }
            | Did::VisitsSeen
            | Did::Became(_)
            | Did::StartedOver => PetActionResult::Stored,
            Did::Practiced { practiced, .. } => match practiced {
                Practiced::Progress { .. } | Practiced::Learned => PetActionResult::Stored,
                Practiced::AlreadyToday | Practiced::Known | Practiced::Egg => {
                    PetActionResult::Refused
                }
            },
            Did::AlreadyPatted
            | Did::DayAlreadyCounted
            | Did::NotEarned(_)
            | Did::NothingToGive
            | Did::Refused(_) => PetActionResult::Refused,
        }
    }
}

/// The stored pet as a session draws it: the row, the pouch, the last
/// treat it ate, the visits its owner has not looked at, the pets staying
/// with it and where it is staying itself.
#[derive(Debug, Clone)]
pub struct PetSnapshot {
    pub row: PetCompanion,
    /// The treat in the pouch, if one waits.
    pub waiting: Option<TreatKind>,
    /// The last treat given: the pet's title on the day it names.
    pub fed: Option<Fed>,
    pub visitors: Vec<PetVisitor>,
    /// The pets that came calling within the last `STAY_MINUTES`, newest
    /// first.
    pub guests: Vec<PetGuest>,
    /// Where this pet went calling within the last `STAY_MINUTES`.
    pub calling: Option<PetStay>,
}

/// What a session's request came back with. Sent to the asking session
/// only; other sessions learn of a stored change from `subscribe_changes`.
#[derive(Debug, Clone)]
pub(crate) enum PetOutcome {
    Acted {
        pet: Box<PetSnapshot>,
        command: PetCommand,
        did: Did,
    },
    Reloaded {
        pet: Box<PetSnapshot>,
    },
    Failed {
        command: PetCommand,
    },
    /// The answer to a visit this session's user paid someone else's pet.
    Visited(Visited),
    /// The put-away switch was written, or its write failed and was logged.
    Switched,
}

/// What a profile shows of a pet: enough to draw it and caption it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProfilePet {
    species: PetSpecies,
    pub mood: PetMood,
    pub name: Option<String>,
    pub form: Form,
    pub days_together: i32,
    /// The treat it ate today: the title after its name and the thing it
    /// carries, until the UTC day ends.
    pub fed_today: Option<TreatKind>,
    /// The tricks it knows: its portrait runs through them.
    pub tricks: Vec<Trick>,
    /// The pets staying with it as the profile loaded, newest first.
    pub guests: Vec<PetGuest>,
}

impl ProfilePet {
    /// The animal it is, `None` for an egg: an egg is no animal yet,
    /// whatever species its row still holds.
    pub fn animal(&self) -> Option<PetSpecies> {
        self.portrait().animal()
    }

    /// Its body in its form and stored mood, carrying what it ate today.
    pub fn portrait(&self) -> Portrait<'_> {
        Portrait::new(
            self.species,
            self.form,
            self.mood,
            self.fed_today,
            &self.tricks,
        )
    }
}

/// What a visit to someone else's pet came to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Visited {
    /// `pet` is the name the host's pet goes by.
    Patted {
        pet: String,
    },
    /// The caller's pet went over for `STAY_MINUTES` and has `line` to
    /// say about it.
    Called {
        caller: String,
        host: String,
        line: &'static str,
    },
    AlreadyToday(VisitKind),
    /// The host put its pet away, or never signed in to have one.
    NobodyHome,
    /// The caller has no pet out to send, or only an egg.
    NobodyToSend,
    /// A pet's owner is not its visitor.
    OwnPet,
    Failed,
}

/// The work half's answer for one command.
pub(super) struct Acted {
    pub(super) pet: PetSnapshot,
    pub(super) did: Did,
}

/// The one writer of a pet's growth, its pouch and its visits. The row is
/// the truth: every command locks it, loads a `Growth` from it, runs one
/// rule, and stores the result, so any number of sessions on any number of
/// replicas act one after the other on the same pet. Sessions hold a mirror
/// to draw from (`PetState`), fed by their own outcomes and by the
/// `pet_changed` notify. The name, the mood and the switch are
/// single-column writes beside it.
#[derive(Clone)]
pub struct PetService {
    db: Db,
    activity_feed: broadcast::Sender<ActivityEvent>,
    changes_tx: broadcast::Sender<Uuid>,
}

impl PetService {
    pub fn new(db: Db, activity_feed: broadcast::Sender<ActivityEvent>) -> Self {
        let (changes_tx, _) = broadcast::channel(256);
        Self {
            db,
            activity_feed,
            changes_tx,
        }
    }

    /// User ids whose stored pet changed, from any replica.
    pub(crate) fn subscribe_changes(&self) -> broadcast::Receiver<Uuid> {
        self.changes_tx.subscribe()
    }

    /// Load the user's pet at session bootstrap, creating the row (an egg)
    /// on the first login.
    pub async fn bootstrap(&self, user_id: Uuid) -> Result<PetSnapshot> {
        let client = self.db.get().await?;
        let row = PetCompanion::ensure(&client, user_id).await?;
        Self::snapshot(&**client, row).await
    }

    async fn snapshot(
        client: &impl tokio_postgres::GenericClient,
        row: PetCompanion,
    ) -> Result<PetSnapshot> {
        let waiting = PetCompanion::waiting_treat(client, row.user_id).await?;
        let fed = PetCompanion::last_fed(client, row.user_id).await?;
        let visitors =
            PetCompanion::visitors_since(client, row.user_id, row.visits_seen_at).await?;
        let stays = stays_since(Utc::now());
        let guests = PetCompanion::guests_since(client, row.user_id, stays).await?;
        let calling = PetCompanion::stay_since(client, row.user_id, stays).await?;
        Ok(PetSnapshot {
            row,
            waiting,
            fed,
            visitors,
            guests,
            calling,
        })
    }

    /// What `user_id`'s profile shows of their pet: `None` for one put
    /// away, and for an account that has not signed in since pets grew.
    pub(crate) async fn profile_pet(
        client: &impl tokio_postgres::GenericClient,
        user_id: Uuid,
    ) -> Result<Option<ProfilePet>> {
        let Some(row) = PetCompanion::find_out(client, user_id).await? else {
            return Ok(None);
        };
        let now = Utc::now();
        // An egg is nothing yet: it goes by no title.
        let fed_today = match row.stage() {
            Stage::Egg => None,
            Stage::Baby | Stage::Young | Stage::Adult | Stage::Elder => fed_today(
                PetCompanion::last_fed(client, user_id).await?,
                now.date_naive(),
            ),
        };
        Ok(Some(ProfilePet {
            species: row.species(),
            mood: row.mood(),
            name: row.name.clone(),
            form: row.form(),
            days_together: row.days_together,
            fed_today,
            tricks: row.tricks(),
            guests: PetCompanion::guests_since(client, user_id, stays_since(now)).await?,
        }))
    }

    /// Run one command for an owner's session and answer on `reply`.
    pub(crate) fn act_task(
        &self,
        user_id: Uuid,
        command: PetCommand,
        reply: mpsc::UnboundedSender<PetOutcome>,
    ) {
        let svc = self.clone();
        let span = info_span!("pet.act_task", user_id = %user_id, action = ?command.action());
        tokio::spawn(
            async move {
                let outcome = match svc.act(user_id, command, Self::today()).await {
                    Ok(acted) => {
                        crate::metrics::record_pet_action(command.action(), acted.did.result());
                        svc.record(acted.did);
                        svc.announce(acted.did, user_id).await;
                        PetOutcome::Acted {
                            pet: Box::new(acted.pet),
                            command,
                            did: acted.did,
                        }
                    }
                    Err(e) => {
                        crate::metrics::record_pet_action(
                            command.action(),
                            PetActionResult::Failed,
                        );
                        tracing::error!(error = ?e, "failed to apply pet command");
                        PetOutcome::Failed { command }
                    }
                };
                // A closed channel is a session that left; nothing to tell.
                let _ = reply.send(outcome);
            }
            .instrument(span),
        );
    }

    /// The counters one settled command moves beside the action counter.
    fn record(&self, did: Did) {
        match did {
            Did::DayCounted(_) => crate::metrics::record_pet_day(),
            Did::Hatched(_) => crate::metrics::record_pet_stage_up(Stage::Baby),
            Did::Grew(stage) => crate::metrics::record_pet_stage_up(stage),
            Did::Earned(kind) => crate::metrics::record_pet_treat(kind, TreatBeat::Earned),
            Did::NotEarned(kind) => crate::metrics::record_pet_treat(kind, TreatBeat::Refused),
            Did::Gave { kind, .. } => crate::metrics::record_pet_treat(kind, TreatBeat::Given),
            Did::Patted
            | Did::AlreadyPatted
            | Did::DayAlreadyCounted
            | Did::NothingToGive
            | Did::Played { .. }
            | Did::Practiced { .. }
            | Did::VisitsSeen
            | Did::Became(_)
            | Did::StartedOver
            | Did::Refused(_) => {}
        }
    }

    /// Lock, apply, store. Everything a command claims (the day's chips, a
    /// treat, a bond day) rides the transaction of the write that claims
    /// it, and the locked row is the only witness of "already today".
    /// `today` is the caller's so the pet's siblings can age one in a test.
    pub(super) async fn act(
        &self,
        user_id: Uuid,
        command: PetCommand,
        today: NaiveDate,
    ) -> Result<Acted> {
        let mut client = self.db.get().await?;
        // An account whose bootstrap load failed still has a row to lock.
        PetCompanion::ensure(&client, user_id).await?;
        let tx = client.transaction().await?;
        let mut row = PetCompanion::lock(&*tx, user_id).await?;
        if !row.enabled {
            let pet = Self::snapshot(&*tx, row).await?;
            tx.commit().await?;
            return Ok(Acted {
                pet,
                did: Did::Refused(Refusal::Away),
            });
        }

        let mut growth = Growth::from_row(&row);
        let before = growth.clone();
        // What a host draws of this pet while it is staying somewhere.
        let drawn = (row.species(), row.form());
        let hatched = match growth.stage() {
            Stage::Egg => false,
            Stage::Baby | Stage::Young | Stage::Adult | Stage::Elder => true,
        };
        // `touched`: the command wrote something a mirror shows outside the
        // growth columns (the pat's day, the pouch). `wrote`: it wrote the
        // row itself and bumped its revision (the visits seen, the species).
        let mut wrote = false;
        let (did, touched) = match command {
            PetCommand::Pat => match PetCompanion::pet_day(&*tx, user_id, today).await? {
                PetDay::FirstToday => {
                    UserChips::apply(
                        &*tx,
                        user_id,
                        ChipMove::PetPetted,
                        PET_CHIP_BONUS,
                        &today.to_string(),
                    )
                    .await
                    .context("crediting the petting chips")?;
                    (Did::Patted, true)
                }
                PetDay::AlreadyToday => (Did::AlreadyPatted, false),
                PetDay::Away => (Did::Refused(Refusal::Away), false),
            },
            PetCommand::CountDay => match growth.count_day(today) {
                Some(counted) => (Did::DayCounted(counted), false),
                None => (Did::DayAlreadyCounted, false),
            },
            PetCommand::Hatch(species) => match growth.hatch() {
                true => {
                    PetCompanion::set_species(&*tx, user_id, species).await?;
                    (Did::Hatched(species), false)
                }
                false => (Did::Refused(Refusal::NotReady), false),
            },
            PetCommand::Grow(pick) => match growth.grow(pick) {
                Some(stage) => (Did::Grew(stage), false),
                None => (Did::Refused(Refusal::NotReady), false),
            },
            PetCommand::Earn(kind) => {
                match PetCompanion::earn_treat(&*tx, user_id, kind, today).await? {
                    TreatEarned::Earned => (Did::Earned(kind), true),
                    TreatEarned::AlreadyToday | TreatEarned::Waiting => {
                        (Did::NotEarned(kind), false)
                    }
                }
            }
            PetCommand::Give => match hatched {
                false => (Did::Refused(Refusal::Egg), false),
                true => match PetCompanion::give_treat(&*tx, user_id, today).await? {
                    Some(kind) => {
                        growth.eat();
                        (
                            Did::Gave {
                                kind,
                                bonded: growth.bond(today),
                            },
                            true,
                        )
                    }
                    None => (Did::NothingToGive, false),
                },
            },
            PetCommand::Play => match hatched {
                false => (Did::Refused(Refusal::Egg), false),
                true => (
                    Did::Played {
                        bonded: growth.bond(today),
                    },
                    false,
                ),
            },
            PetCommand::Practice(trick) => (
                Did::Practiced {
                    trick,
                    practiced: growth.practice(trick, today),
                },
                false,
            ),
            PetCommand::SeenVisits => {
                row = PetCompanion::mark_visits_seen(&*tx, user_id).await?;
                wrote = true;
                (Did::VisitsSeen, false)
            }
            PetCommand::Species(species) => match hatched {
                false => (Did::Refused(Refusal::Egg), false),
                true => {
                    row = PetCompanion::set_species(&*tx, user_id, species).await?;
                    wrote = true;
                    (Did::Became(species), false)
                }
            },
            PetCommand::Reset => {
                growth.reset();
                (Did::StartedOver, false)
            }
        };

        let grew = growth != before;
        let row = match (grew, touched) {
            (true, _) => PetCompanion::store(&*tx, growth.to_write(user_id)).await?,
            (false, true) => PetCompanion::touch(&*tx, user_id).await?,
            (false, false) => row,
        };
        if grew || touched || wrote {
            PetCompanion::notify_changed(&*tx, user_id).await?;
        }
        if (row.species(), row.form()) != drawn {
            PetCompanion::notify_hosts(&*tx, user_id, stays_since(Utc::now())).await?;
        }
        let pet = Self::snapshot(&*tx, row).await?;
        tx.commit().await?;
        Ok(Acted { pet, did })
    }

    /// The activity event for what a command did. A private feed line, sent
    /// after the commit: the first pat of the day tells the session it paid.
    async fn announce(&self, did: Did, user_id: Uuid) {
        match did {
            Did::Patted => {}
            Did::AlreadyPatted
            | Did::DayCounted(_)
            | Did::DayAlreadyCounted
            | Did::Hatched(_)
            | Did::Grew(_)
            | Did::Earned(_)
            | Did::NotEarned(_)
            | Did::Gave { .. }
            | Did::NothingToGive
            | Did::Played { .. }
            | Did::Practiced { .. }
            | Did::VisitsSeen
            | Did::Became(_)
            | Did::StartedOver
            | Did::Refused(_) => return,
        }
        let client = match self.db.get().await {
            Ok(client) => client,
            Err(e) => {
                tracing::error!(error = ?e, "failed to announce a petting");
                return;
            }
        };
        let username = late_core::models::profile::fetch_username(&client, user_id).await;
        let _ = self
            .activity_feed
            .send(ActivityEvent::pet_petted(user_id, username));
    }

    /// Pay `owner_id`'s pet a visit from `visitor_id` and answer on
    /// `reply`: a pat from its profile, or the visitor's own pet calling.
    pub(crate) fn visit_task(
        &self,
        visitor_id: Uuid,
        owner_id: Uuid,
        kind: VisitKind,
        reply: mpsc::UnboundedSender<PetOutcome>,
    ) {
        let svc = self.clone();
        let action = match kind {
            VisitKind::Pat => PetAction::VisitPat,
            VisitKind::Call => PetAction::VisitCall,
        };
        let span = info_span!(
            "pet.visit_task",
            visitor_id = %visitor_id,
            owner_id = %owner_id,
            action = ?action
        );
        tokio::spawn(
            async move {
                let visited = match svc.visit(visitor_id, owner_id, kind, Self::today()).await {
                    Ok(visited) => {
                        let (result, beat) = match &visited {
                            Visited::Patted { .. } => (PetActionResult::Stored, VisitBeat::Pat),
                            Visited::Called { .. } => (PetActionResult::Stored, VisitBeat::Call),
                            Visited::AlreadyToday(_)
                            | Visited::NobodyHome
                            | Visited::NobodyToSend
                            | Visited::OwnPet
                            | Visited::Failed => (PetActionResult::Refused, VisitBeat::Refused),
                        };
                        crate::metrics::record_pet_action(action, result);
                        crate::metrics::record_pet_visit(beat);
                        visited
                    }
                    Err(e) => {
                        crate::metrics::record_pet_action(action, PetActionResult::Failed);
                        tracing::error!(error = ?e, "failed to visit a pet");
                        Visited::Failed
                    }
                };
                let _ = reply.send(PetOutcome::Visited(visited));
            }
            .instrument(span),
        );
    }

    /// The visit is the row in `pet_visits`: the insert claims it, once per
    /// visitor, pet, kind and day. Both pets are read without a row lock;
    /// the claim is what makes a visit happen once. The host's row is
    /// touched and its owner's sessions told in the same transaction. A
    /// call is also a stay: the caller's pet is its host's guest for
    /// `STAY_MINUTES` from the claim, so the caller's row is touched and
    /// its sessions told too.
    async fn visit(
        &self,
        visitor_id: Uuid,
        owner_id: Uuid,
        kind: VisitKind,
        today: NaiveDate,
    ) -> Result<Visited> {
        if visitor_id == owner_id {
            return Ok(Visited::OwnPet);
        }
        let mut client = self.db.get().await?;
        let tx = client.transaction().await?;
        let Some(host) = PetCompanion::find_out(&*tx, owner_id).await? else {
            tx.commit().await?;
            return Ok(Visited::NobodyHome);
        };
        let visited = match kind {
            VisitKind::Pat => Visited::Patted {
                pet: display_name(&host),
            },
            VisitKind::Call => {
                let caller = match PetCompanion::find_out(&*tx, visitor_id).await? {
                    Some(caller) => caller,
                    None => {
                        tx.commit().await?;
                        return Ok(Visited::NobodyToSend);
                    }
                };
                match caller.stage() {
                    Stage::Egg => {
                        tx.commit().await?;
                        return Ok(Visited::NobodyToSend);
                    }
                    Stage::Baby | Stage::Young | Stage::Adult | Stage::Elder => {}
                }
                let caller_fed = PetCompanion::last_fed(&*tx, visitor_id).await?;
                let host_fed = PetCompanion::last_fed(&*tx, owner_id).await?;
                Visited::Called {
                    line: call_line(
                        fed_today(caller_fed, today),
                        fed_today(host_fed, today),
                        host.stage(),
                    ),
                    caller: display_name(&caller),
                    host: display_name(&host),
                }
            }
        };
        if !PetCompanion::claim_visit(&*tx, owner_id, visitor_id, kind, today).await? {
            tx.commit().await?;
            return Ok(Visited::AlreadyToday(kind));
        }
        // Every mirror a visit changes orders its snapshots by `revision`:
        // the host's, and for a call the caller's (its tile says where its
        // pet is staying). Two rows are touched in the order of their ids,
        // so two people calling on each other cannot lock each other out.
        let touched = match kind {
            VisitKind::Pat => vec![owner_id],
            VisitKind::Call => vec![owner_id.min(visitor_id), owner_id.max(visitor_id)],
        };
        for user_id in touched {
            PetCompanion::touch(&*tx, user_id).await?;
            PetCompanion::notify_changed(&*tx, user_id).await?;
        }
        tx.commit().await?;
        Ok(visited)
    }

    /// Re-read the stored pet for a session whose mirror went stale.
    pub(crate) fn reload_task(&self, user_id: Uuid, reply: mpsc::UnboundedSender<PetOutcome>) {
        let svc = self.clone();
        let span = info_span!("pet.reload_task", user_id = %user_id);
        tokio::spawn(
            async move {
                match svc.bootstrap(user_id).await {
                    Ok(pet) => {
                        let _ = reply.send(PetOutcome::Reloaded { pet: Box::new(pet) });
                    }
                    Err(e) => {
                        tracing::error!(error = ?e, "failed to reload the pet");
                    }
                }
            }
            .instrument(span),
        );
    }

    /// What the notify worker subscribes to.
    pub const CHANNELS: &'static [Channel] = &[Channel::PetChanged];

    /// Fan `pet_changed` out to this replica's sessions. A change committed
    /// while the listener was reconnecting is not replayed: the mirror of
    /// an idle second session lags until its owner's next action or change,
    /// which both carry the stored pet, so a resync has nothing to re-read.
    pub fn start_notify_worker(
        &self,
        mut signals: mpsc::UnboundedReceiver<Signal>,
    ) -> tokio::task::JoinHandle<()> {
        let service = self.clone();
        tokio::spawn(async move {
            while let Some(signal) = signals.recv().await {
                match signal {
                    Signal::Resync => {}
                    Signal::Notify { payload, .. } => service.publish_change(&payload),
                }
            }
        })
    }

    pub(super) fn publish_change(&self, payload: &str) {
        match Uuid::parse_str(payload) {
            // No receiver is a replica with no sessions; nothing to tell.
            Ok(user_id) => {
                let _ = self.changes_tx.send(user_id);
            }
            Err(error) => {
                tracing::warn!(error = ?error, payload, "unreadable pet_changed payload");
            }
        }
    }

    pub fn set_name_task(&self, user_id: Uuid, name: Option<String>) {
        let svc = self.clone();
        tokio::spawn(async move {
            if let Err(e) = svc.set_name(user_id, name.as_deref()).await {
                tracing::error!(error = ?e, user_id = %user_id, "failed to set pet name");
            }
        });
    }

    /// A pet staying somewhere is named in its host's box, so its hosts
    /// are told once the name is written.
    async fn set_name(&self, user_id: Uuid, name: Option<&str>) -> Result<()> {
        let client = self.db.get().await?;
        PetCompanion::set_name(&client, user_id, name).await?;
        PetCompanion::notify_hosts(&**client, user_id, stays_since(Utc::now())).await
    }

    /// Bring the pet out or put it away, and tell the asking session on
    /// `reply` once the write is over: until then a command of its own can
    /// still find the row as it was.
    pub(crate) fn set_enabled_task(
        &self,
        user_id: Uuid,
        enabled: bool,
        reply: mpsc::UnboundedSender<PetOutcome>,
    ) {
        let svc = self.clone();
        tokio::spawn(async move {
            if let Err(e) = svc.set_enabled(user_id, enabled).await {
                tracing::error!(error = ?e, user_id = %user_id, enabled, "failed to switch the pet");
            }
            let _ = reply.send(PetOutcome::Switched);
        });
    }

    /// A pet put away is nobody's guest, so its hosts are told once the
    /// switch is written.
    async fn set_enabled(&self, user_id: Uuid, enabled: bool) -> Result<()> {
        let client = self.db.get().await?;
        PetCompanion::set_enabled(&client, user_id, enabled).await?;
        PetCompanion::notify_hosts(&**client, user_id, stays_since(Utc::now())).await
    }

    /// Record the mood the session inferred, so a profile can show it. Fired
    /// on every mood change and once more, `Asleep`, when the session ends.
    pub fn set_mood_task(&self, user_id: Uuid, mood: PetMood) {
        let svc = self.clone();
        tokio::spawn(async move {
            if let Err(e) = svc.set_mood(user_id, mood).await {
                tracing::error!(error = ?e, user_id = %user_id, "failed to set pet mood");
            }
        });
    }

    async fn set_mood(&self, user_id: Uuid, mood: PetMood) -> Result<()> {
        let client = self.db.get().await?;
        PetCompanion::set_mood(&client, user_id, mood).await
    }

    pub fn today() -> NaiveDate {
        Utc::now().date_naive()
    }
}

/// What a pet goes by: its name, or what it is while it has none.
fn display_name(row: &PetCompanion) -> String {
    match (&row.name, row.stage()) {
        (Some(name), _) => name.clone(),
        (None, Stage::Egg) => "the egg".to_string(),
        (None, Stage::Baby | Stage::Young | Stage::Adult | Stage::Elder) => {
            format!("the {}", row.species().as_str())
        }
    }
}

#[cfg(test)]
#[path = "svc_test.rs"]
mod svc_test;
