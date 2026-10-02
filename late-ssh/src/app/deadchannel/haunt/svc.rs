//! Orchestration for the haunting: the gate and arming at session start,
//! the splash door, the glitch scheduler, the own-name flicker, the hit
//! claims, the breakthrough and its invitation, the bio screen, and the `/haunt` admin
//! controls. This is the only haunting layer that touches `App`, logging,
//! metrics, and persistence; the machines in `state.rs` stay pure, and the
//! root files keep one routing line each.

use late_core::db::Db;
use late_core::models::deadchannel_name_hit::NameHitSignal;
use late_core::models::user::{FirstContactBioVerdict, FirstContactHitClaim, User};
use tokio::sync::oneshot;
use tracing::{Instrument, info_span};

use super::state::{
    ActiveHit, BIO_RESCREEN_AFTER_HOURS, BioStanding, Breakthrough, BreakthroughPhase,
    BreakthroughRoll, BreakthroughTick, ClockGlitch, FirstContactGate, FirstContactMarks,
    GLITCH_TOTAL_CAP, GlitchTick, HauntCommand, HauntState, HitStage, InvitationClaim,
    NAME_TOTAL_CAP, NameFlicker, NameRoll, PendingClaim, WHISPER_GAP_HOURS, WHISPER_TOTAL_CAP,
    WhisperState, WhisperTick, bio_hash, glitch_caps, name_caps,
};
use crate::app::ai::screen::{BioScreen, screen_bio};
use crate::app::ai::svc::AiService;
use crate::app::common::primitives::Banner;
use crate::app::deadchannel::runner;
use crate::app::state::App;
use crate::metrics::{self, BioScreenOutcome, FirstContactBeat};
use crate::state::State;

/// Evaluate the eligibility gate for a connecting user: the row that
/// already loaded plus one primary-key read of the online-time table (a
/// failed read counts as no hours: the gate fails closed and the warning
/// is the one place the miss shows). When the bio has no usable verdict,
/// claim a screen for it in the background; the verdict lands on the row
/// for the next session: filling your bio tonight means the static can
/// find you tomorrow (GAME.md).
///
/// Staff only, by decision in code, the same rule `arm` applies: anyone
/// else cannot be haunted, so the connect path spends nothing on them: no
/// online-time round trip, no paid bio screen.
pub(crate) async fn bootstrap_gate(state: &State, is_staff: bool, user: &User) -> FirstContactGate {
    if !is_staff {
        return FirstContactGate::closed();
    }
    let online_milliseconds = async {
        let client = state.db.get().await?;
        late_core::models::leaderboard::total_online_milliseconds(&client, user.id).await
    }
    .await;
    let online_milliseconds = match online_milliseconds {
        Ok(milliseconds) => milliseconds,
        Err(error) => {
            tracing::warn!(user_id = %user.id, username = %user.username, error = ?error, "failed to read online time for the first contact gate");
            0
        }
    };
    let gate = FirstContactGate::evaluate(
        chrono::Utc::now(),
        online_milliseconds,
        &user.settings,
        state.ai_service.is_enabled(),
    );
    // Who the static could choose, and why not: one line per connect with
    // every leg's number, and the verdict counted so the thresholds can be
    // tuned against how many people each one turns away.
    let verdict = gate.verdict();
    metrics::record_first_contact_gate(verdict, is_staff);
    tracing::info!(
        user_id = %user.id,
        username = %user.username,
        is_staff,
        active_hours = gate.active_hours,
        touched_settings = gate.touched_settings,
        bio_chars = gate.bio_chars,
        bio = ?gate.bio,
        verdict = ?verdict,
        "first contact gate evaluated"
    );
    if gate.needs_bio_screen() {
        screen_bio_task(
            state.db.clone(),
            state.ai_service.clone(),
            user.id,
            user.username.clone(),
            late_core::models::user::extract_bio(&user.settings),
        );
    }
    gate
}

/// Build the session's haunting slot. Stage 1 arms for staff (admins and
/// moderators, `Permissions::can_moderate`) and nobody else, by decision
/// in code; stages 2-4 arm behind the gate, or for any staff member whose
/// funnel already has a stage-2 hit (eligibility gates entering, never
/// continuing).
/// The chain order is the spec: three clock bursts open stage 2, the
/// third name hit arms the stage-3 whisper (it fires on the next fresh
/// connect, then once more on a later day), and a day after the last
/// delivered whisper the next own send breaks through and the stage-4
/// invitation DM follows.
/// Dice are per session on purpose: the same person on two evenings, or
/// two people side by side, roll differently.
pub(crate) fn arm(
    is_staff: bool,
    user_id: uuid::Uuid,
    username: &str,
    marks: FirstContactMarks,
    gate: FirstContactGate,
) -> HauntState {
    let stage1 = is_staff;
    let chosen = stage1 && (gate.passes() || marks.name_hits > 0);
    let whisper_armed =
        chosen && marks.name_hits >= NAME_TOTAL_CAP && marks.whisper_due(chrono::Utc::now());
    // What this session can fire, for whom. Stage 1 off is the quiet
    // default for everyone who is not staff, so only an armed session is
    // worth a line.
    if stage1 {
        tracing::info!(
            user_id = %user_id,
            username = %username,
            is_staff,
            chosen,
            whisper_armed,
            glitch_hits = marks.glitch_hits,
            name_hits = marks.name_hits,
            whisper_hits = marks.whisper_hits,
            "first contact armed"
        );
    }
    HauntState {
        whisper: whisper_armed.then(|| WhisperState::for_user(user_id, marks.whisper_hits)),
        clock_glitch: stage1.then(|| ClockGlitch::new(session_seed(user_id), 0, marks.glitch_hits)),
        name_flicker: chosen.then(|| NameFlicker::new(session_seed(user_id), marks.name_hits)),
        breakthrough: chosen.then(|| Breakthrough::for_user(user_id)),
        pending_invitation: None,
        witness: None,
        marks,
        gate,
        stage1,
        chosen,
        pending_claims: Vec::new(),
    }
}

fn session_seed(user_id: uuid::Uuid) -> u64 {
    (user_id.as_u128() as u64) ^ (uuid::Uuid::now_v7().as_u128() as u64)
}

/// One world tick for the whole haunting. Returns true when the frame
/// changed. The splash block in `tick.rs` must run first (it advances
/// `splash_ticks` and consults `HauntState::holds_splash_door` before
/// expiring the splash on its own).
pub(crate) fn tick(app: &mut App) -> bool {
    let mut changed = false;
    changed |= tick_claims(app);
    if app.show_splash {
        changed |= tick_splash_door(app);
    }
    changed |= tick_clock_glitch(app);
    changed |= tick_name_flicker(app);
    changed |= tick_witness(app);
    changed |= tick_breakthrough(app);
    changed |= tick_commands(app);
    changed
}

/// Drain answered hit claims. A won claim starts the beat this tick (the
/// one frame it costs); a capped one re-dices; a failed one defers. The
/// row's count is mirrored into the marks on every answer, so a share
/// spent on another device or replica quiets this session too.
fn tick_claims(app: &mut App) -> bool {
    let mut answered = Vec::new();
    app.haunt
        .pending_claims
        .retain_mut(|pending| match pending.rx.try_recv() {
            Ok(outcome) => {
                answered.push((pending.stage, outcome));
                false
            }
            Err(oneshot::error::TryRecvError::Empty) => true,
            Err(oneshot::error::TryRecvError::Closed) => {
                answered.push((
                    pending.stage,
                    Err(anyhow::anyhow!("claim task dropped its sender")),
                ));
                false
            }
        });
    let tick = app.marquee_tick;
    let mut changed = false;
    for (stage, outcome) in answered {
        match (stage, outcome) {
            (HitStage::Glitch, Ok(FirstContactHitClaim::Won { hits })) => {
                if let Some(glitch) = app.haunt.clock_glitch.as_mut() {
                    glitch.start(tick, hits);
                }
                app.haunt.marks.glitch_hits = hits;
                metrics::record_first_contact_beat(FirstContactBeat::GlitchBurst);
                tracing::info!(user_id = %app.user_id, username = %app.username, hits, "first contact clock glitch burst");
                changed = true;
            }
            (HitStage::Glitch, Ok(FirstContactHitClaim::Capped { hits })) => {
                if let Some(glitch) = app.haunt.clock_glitch.as_mut() {
                    glitch.claim_capped(tick, hits);
                }
                app.haunt.marks.glitch_hits = hits;
                tracing::debug!(user_id = %app.user_id, username = %app.username, hits, "first contact clock glitch capped by the row");
            }
            (HitStage::Glitch, Err(error)) => {
                if let Some(glitch) = app.haunt.clock_glitch.as_mut() {
                    glitch.claim_failed(tick);
                }
                tracing::warn!(user_id = %app.user_id, username = %app.username, error = ?error, "first contact clock glitch claim failed");
            }
            (
                HitStage::Name {
                    message_id,
                    room_id,
                },
                Ok(FirstContactHitClaim::Won { hits }),
            ) => {
                if let Some(flicker) = app.haunt.name_flicker.as_mut() {
                    flicker.start(message_id, tick, hits);
                }
                app.haunt.marks.name_hits = hits;
                publish_name_hit(app, message_id, room_id);
                metrics::record_first_contact_beat(FirstContactBeat::NameFlicker);
                tracing::info!(user_id = %app.user_id, username = %app.username, hits, "first contact name flicker hit");
                changed = true;
            }
            (HitStage::Name { .. }, Ok(FirstContactHitClaim::Capped { hits })) => {
                if let Some(flicker) = app.haunt.name_flicker.as_mut() {
                    flicker.claim_capped(hits);
                }
                app.haunt.marks.name_hits = hits;
                tracing::debug!(user_id = %app.user_id, username = %app.username, hits, "first contact name flicker capped by the row");
            }
            (HitStage::Name { .. }, Err(error)) => {
                if let Some(flicker) = app.haunt.name_flicker.as_mut() {
                    flicker.claim_failed();
                }
                tracing::warn!(user_id = %app.user_id, username = %app.username, error = ?error, "first contact name flicker claim failed");
            }
        }
    }
    changed
}

/// Drive the armed whisper for one splash tick. Release (natural or hard
/// cap) closes the splash here and claims one capped
/// delivery only on a delivered line; the last stamp is also what starts
/// the invitation clock.
fn tick_splash_door(app: &mut App) -> bool {
    let Some(whisper) = app.haunt.whisper.as_mut() else {
        return false;
    };
    match whisper.tick(app.splash_ticks) {
        WhisperTick::Holding => {}
        WhisperTick::Released { delivered } => {
            app.haunt.whisper = None;
            app.show_splash = false;
            // Any swallowed Esc left the parser mid-escape; same reset the
            // normal splash skip does.
            app.vt_input.reset();
            if delivered {
                let now = chrono::Utc::now();
                app.haunt.marks.whisper_hits = app.haunt.marks.whisper_hits.saturating_add(1);
                app.haunt.marks.whisper_at = Some(now);
                app.profile_state.service().claim_first_contact_whisper(
                    app.user_id,
                    now,
                    chrono::Duration::hours(WHISPER_GAP_HOURS),
                    WHISPER_TOTAL_CAP,
                );
                metrics::record_first_contact_beat(FirstContactBeat::WhisperDelivered);
                tracing::info!(user_id = %app.user_id, username = %app.username, hits = app.haunt.marks.whisper_hits, "first contact whisper delivered");
            }
        }
    }
    // The splash pays a frame per tick anyway while it is up.
    true
}

/// The stage-1 scheduler, one world tick. A burst only spends itself
/// while the sidebar clock is actually on screen, and the row decides
/// the caps: `Due` sends a claim, and the burst starts on the tick the
/// claim comes back won (`tick_claims`). Only its start and end frames
/// cost a repaint: render reads `corruption(marquee_tick)` in between,
/// and the ~200ms hold spans the sidebar's ~132ms wake cadence on its own.
fn tick_clock_glitch(app: &mut App) -> bool {
    if app.haunt.clock_glitch.is_none() {
        return false;
    }
    let clock_visible = !app.show_splash && app.right_sidebar_visible();
    let glitch = app.haunt.clock_glitch.as_mut().expect("checked above");
    match glitch.tick(app.marquee_tick, clock_visible) {
        GlitchTick::Due => {
            let rx = app
                .profile_state
                .service()
                .claim_first_contact_glitch_burst(
                    app.user_id,
                    chrono::Utc::now().date_naive(),
                    glitch_caps(),
                );
            app.haunt.pending_claims.push(PendingClaim {
                stage: HitStage::Glitch,
                rx,
            });
            false
        }
        GlitchTick::Started => {
            // A forced (`/haunt glitch`) burst: shown at once, counted
            // uncapped.
            app.haunt.marks.glitch_hits = glitch.total_hits();
            app.profile_state
                .service()
                .record_first_contact_glitch_hit(app.user_id);
            metrics::record_first_contact_beat(FirstContactBeat::GlitchBurst);
            tracing::info!(user_id = %app.user_id, username = %app.username, hits = app.haunt.marks.glitch_hits, "first contact clock glitch burst (forced)");
            true
        }
        GlitchTick::Ended => true,
        GlitchTick::Idle => false,
    }
}

/// The stage-2 roller: once the clock has spent its share of bursts,
/// every own message that lands with its own author header rolls the dice
/// (grouped continuations never reach here; their label does not draw).
/// A roll that lands claims a hit on the row; on a won claim
/// (`tick_claims`) that message's author label corrupts in two ~800ms
/// waves with different glyphs (the corruption rides the chat rows cache
/// key, so start, the wave edge, and heal each rebuild the rows exactly
/// once), and the row's counter is what arms the
/// stage-3 whisper at its third hit.
fn tick_name_flicker(app: &mut App) -> bool {
    // Drained even while unarmed, so a stale echo id never waits around
    // for a later `/haunt arm`.
    let landed = app.chat.take_own_message_landed();
    let stage_open = app.haunt.marks.glitch_hits >= GLITCH_TOTAL_CAP;
    let Some(flicker) = app.haunt.name_flicker.as_mut() else {
        return false;
    };
    let mut changed = false;
    changed |= flicker.tick(app.marquee_tick);
    let Some((message_id, room_id)) = landed else {
        return changed;
    };
    match flicker.note_own_message(message_id, app.marquee_tick, stage_open) {
        NameRoll::Miss => {}
        NameRoll::Claim => {
            let rx = app.profile_state.service().claim_first_contact_name_hit(
                app.user_id,
                chrono::Utc::now().date_naive(),
                name_caps(),
            );
            app.haunt.pending_claims.push(PendingClaim {
                stage: HitStage::Name {
                    message_id,
                    room_id,
                },
                rx,
            });
        }
        NameRoll::Forced => {
            app.haunt.marks.name_hits = flicker.total_hits();
            app.profile_state
                .service()
                .record_first_contact_name_hit(app.user_id);
            // A forced hit skips the row's caps, not the room: `/haunt
            // name` is the one way to watch the public half of stage 2
            // land, so it has to travel like a real one.
            publish_name_hit(app, message_id, room_id);
            metrics::record_first_contact_beat(FirstContactBeat::NameFlicker);
            tracing::info!(user_id = %app.user_id, username = %app.username, hits = app.haunt.marks.name_hits, "first contact name flicker hit (forced)");
            changed = true;
        }
    }
    changed
}

/// Put the live hit on the wire for the rest of the room. Read back off
/// the machine that is already painting it, so the seed the room gets is
/// the seed this screen is using: same characters, same two waves,
/// everywhere. This replica hears it back over the same listener as the
/// others; `tick_witness` recognises the live hit and declines the copy.
fn publish_name_hit(app: &App, message_id: uuid::Uuid, room_id: uuid::Uuid) {
    let Some((live_id, seed)) = app
        .haunt
        .name_flicker
        .as_ref()
        .and_then(NameFlicker::live_hit)
    else {
        return;
    };
    debug_assert_eq!(live_id, message_id);
    app.chat.service.publish_name_hit(NameHitSignal {
        message_id,
        room_id,
        user_id: app.user_id,
        seed,
    });
}

/// Replay the beats this session hears on other people's names. Every
/// session runs this, haunted or not: stage 2 is the rung the room
/// watches somebody climb. The chat state hands a beat over only once the
/// message it names is on this screen (`take_witnessed_hit_landed`), so
/// the name corrupts as the message arrives, then heals, on the seed that
/// rode the wire.
fn tick_witness(app: &mut App) -> bool {
    // Drained even outside the audience, so a stale beat never waits
    // around for a later `/haunt arm`.
    let landed = app.chat.take_witnessed_hit_landed();
    let changed = ActiveHit::tick(&mut app.haunt.witness, app.marquee_tick);
    let Some((message_id, seed)) = landed else {
        return changed;
    };
    // The audience is exactly stage 1's: staff are haunted where only
    // staff can see it.
    if !app.haunt.stage1 {
        return changed;
    }
    // This session's own hit coming back off the wire: its own machine is
    // already painting it. A second device of the same person never
    // claimed, holds no live hit, and witnesses it like anyone else.
    let own_hit = app
        .haunt
        .name_flicker
        .as_ref()
        .and_then(NameFlicker::live_hit)
        .map(|(id, _)| id);
    if own_hit == Some(message_id) {
        return changed;
    }
    app.haunt.witness = Some(ActiveHit::new(message_id, app.marquee_tick, seed));
    tracing::debug!(
        user_id = %app.user_id,
        username = %app.username,
        message_id = %message_id,
        "first contact name flicker witnessed"
    );
    true
}

/// Stage 4, the breakthrough. Once the invitation is due, the next send
/// this session submits asks the invitation task for the once-ever claim
/// (the same person's send from another device never does: that screen may
/// have nobody in front of it); the task answers before it sends anything,
/// the scene plays here on a won claim, and the same task sends the DM once
/// the line has finished typing, so a session that drops mid-scene still
/// gets its invitation. A claim taken by another device stamps the marks
/// and plays nothing; a failed ask is the task's to log, and this session
/// stops asking. Self-serve on purpose (the chosen one's own session
/// notices), so there is no cross-user sweep.
fn tick_breakthrough(app: &mut App) -> bool {
    // Drained even while unarmed, like the landing echo.
    let sent_here = app.chat.take_own_send_succeeded();
    let due = app.haunt.marks.breakthrough_due(chrono::Utc::now());
    let answer = match app.haunt.pending_invitation.as_mut() {
        None => None,
        Some(rx) => match rx.try_recv() {
            Ok(claim) => Some(claim),
            Err(oneshot::error::TryRecvError::Empty) => None,
            Err(oneshot::error::TryRecvError::Closed) => {
                // The task answers on every path before it returns, so a
                // closed channel means it died without a word of its own.
                tracing::error!(user_id = %app.user_id, username = %app.username, "first contact invitation task dropped its claim answer");
                Some(InvitationClaim::Failed)
            }
        },
    };
    if answer.is_some() {
        app.haunt.pending_invitation = None;
    }
    let Some(breakthrough) = app.haunt.breakthrough.as_mut() else {
        return false;
    };
    let mut changed = false;
    match answer {
        None => {}
        Some(InvitationClaim::Won) => {
            breakthrough.start(app.marquee_tick);
            app.haunt.marks.invited_at = Some(chrono::Utc::now());
            metrics::record_first_contact_beat(FirstContactBeat::Breakthrough);
            tracing::info!(user_id = %app.user_id, username = %app.username, "first contact breakthrough");
            changed = true;
        }
        Some(InvitationClaim::Taken) => {
            breakthrough.claim_taken();
            app.haunt.marks.invited_at = Some(chrono::Utc::now());
            tracing::debug!(user_id = %app.user_id, username = %app.username, "first contact breakthrough claim taken by another session");
        }
        Some(InvitationClaim::Failed) => {
            breakthrough.claim_failed();
        }
    }
    match breakthrough.tick(app.marquee_tick) {
        BreakthroughTick::Idle => {}
        BreakthroughTick::Playing | BreakthroughTick::Ended => changed = true,
    }
    if !sent_here {
        return changed;
    }
    match breakthrough.note_own_send(due) {
        BreakthroughRoll::Wait => {}
        BreakthroughRoll::Claim => {
            app.haunt.pending_invitation =
                Some(app.chat.service.send_first_contact_invitation_task(
                    app.user_id,
                    app.username.clone(),
                    Breakthrough::dm_delay(),
                ));
        }
    }
    changed
}

/// Route splash input into the held door. Returns true when swallowed:
/// while the whisper holds the door every key, Esc included, does nothing
/// at all. The scene plays on its own clock (the static pulses, the line
/// types itself), which is what keeps a swallowed key from reading as a
/// hung terminal.
pub(crate) fn swallows_splash_input(app: &mut App) -> bool {
    if app.haunt.whisper.is_none() {
        return false;
    }
    // A swallowed ESC leaves the parser mid-escape; same reset as the
    // normal splash skip.
    app.vt_input.reset();
    true
}

/// Re-run the splash with the whisper armed, ignoring the due rule (the
/// pool still follows the marks, so a replay after the first door speaks
/// the second line). The `/haunt replay` admin test hook (also used by
/// tests); a completed replay still claims delivery through the normal
/// release path, where the row's cap and gap decide whether it counts.
pub(crate) fn replay_whisper(app: &mut App) {
    app.haunt.whisper = Some(WhisperState::for_user(
        app.user_id,
        app.haunt.marks.whisper_hits,
    ));
    app.show_splash = true;
    app.splash_ticks = 0;
}

/// Screen a bio for the gate, in the background of a login. The claim on
/// the row is what makes this cost one AI call per bio text however many
/// sessions or replicas notice it at once: a lost claim is another
/// session's screen in flight. A call that breaks leaves the pending
/// claim to expire (`BIO_RESCREEN_AFTER_HOURS`) rather than releasing it,
/// so a flapping API cannot burn a call per login.
fn screen_bio_task(db: Db, ai: AiService, user_id: uuid::Uuid, username: String, bio: String) {
    tokio::spawn(
        async move {
            let hash = bio_hash(&bio);
            let outcome = screen_bio_flow(&db, &ai, user_id, &hash, &bio).await;
            let outcome = match outcome {
                Ok(outcome) => outcome,
                Err(error) => {
                    tracing::warn!(user_id = %user_id, username = %username, error = ?error, "first contact bio screen failed");
                    BioScreenOutcome::CallFailed
                }
            };
            metrics::record_first_contact_bio_screen(outcome);
            tracing::info!(user_id = %user_id, username = %username, hash, outcome = ?outcome, "first contact bio screen");
        }
        .instrument(info_span!("haunt.bio_screen_task", user_id = %user_id)),
    );
}

async fn screen_bio_flow(
    db: &Db,
    ai: &AiService,
    user_id: uuid::Uuid,
    hash: &str,
    bio: &str,
) -> anyhow::Result<BioScreenOutcome> {
    let client = db.get().await?;
    let now = chrono::Utc::now();
    let won = User::claim_first_contact_bio_screen(
        &client,
        user_id,
        hash,
        now,
        chrono::Duration::hours(BIO_RESCREEN_AFTER_HOURS),
    )
    .await?;
    if !won {
        return Ok(BioScreenOutcome::LostClaim);
    }
    drop(client);
    let verdict = match screen_bio(ai, bio).await? {
        BioScreen::Passed => FirstContactBioVerdict::Passed,
        BioScreen::Failed => FirstContactBioVerdict::Failed,
        BioScreen::Unavailable => return Ok(BioScreenOutcome::Unavailable),
    };
    let client = db.get().await?;
    let landed =
        User::set_first_contact_bio_verdict(&client, user_id, hash, verdict, chrono::Utc::now())
            .await?;
    if !landed {
        return Ok(BioScreenOutcome::TextChanged);
    }
    Ok(match verdict {
        FirstContactBioVerdict::Passed => BioScreenOutcome::Passed,
        FirstContactBioVerdict::Failed => BioScreenOutcome::Failed,
        FirstContactBioVerdict::Pending => unreachable!("a screen never returns pending"),
    })
}

fn on_off(value: bool) -> &'static str {
    match value {
        true => "on",
        false => "off",
    }
}

fn bio_standing_label(standing: BioStanding) -> &'static str {
    match standing {
        BioStanding::TooShort => "too short",
        BioStanding::AiOff => "ai off",
        BioStanding::Unscreened => "screening",
        BioStanding::Pending => "pending",
        BioStanding::Passed => "passed",
        BioStanding::Failed => "failed",
    }
}

/// Drain the `/haunt` admin command. The composer only records it for
/// admins, so everything here trusts the caller.
fn tick_commands(app: &mut App) -> bool {
    let Some(command) = app.chat.take_requested_haunt() else {
        return false;
    };
    match command {
        HauntCommand::Status => {
            let door = match app.haunt.whisper.is_some() {
                true => "door armed",
                false => "door idle",
            };
            let glitch = match &app.haunt.clock_glitch {
                None => "glitch idle".to_string(),
                Some(_) if app.haunt.marks.glitch_hits >= GLITCH_TOTAL_CAP => {
                    "glitch quiet (share spent)".to_string()
                }
                Some(glitch) => {
                    let minutes = glitch.next_in_ticks(app.marquee_tick) * 66 / 60_000;
                    format!("glitch in ~{minutes}m")
                }
            };
            let whisper = format!(
                "whispers {}/{WHISPER_TOTAL_CAP}",
                app.haunt.marks.whisper_hits.min(WHISPER_TOTAL_CAP)
            );
            let invite = match (
                app.haunt.marks.invited_at,
                app.haunt.breakthrough.as_ref().map(Breakthrough::phase),
            ) {
                (Some(_), _) => "invited",
                (None, None) => "breakthrough idle",
                (None, Some(BreakthroughPhase::Idle)) => {
                    match app.haunt.marks.breakthrough_due(chrono::Utc::now()) {
                        true => "breakthrough due on next send",
                        false => "breakthrough waiting",
                    }
                }
                (None, Some(BreakthroughPhase::Claiming)) => "breakthrough claiming",
                (None, Some(BreakthroughPhase::Playing { .. })) => "breaking through",
                (None, Some(BreakthroughPhase::Failed)) => "breakthrough ask failed this session",
            };
            // Whether a beat of somebody else's is on this screen right now.
            let witness = match app.haunt.witness.is_some() {
                true => "witnessing",
                false => "witness idle",
            };
            let gate = app.haunt.gate;
            // The runner's sheet as this session mirrors it: what the
            // strip and the frame HUD are painting from.
            let sheet = match &app.fight.sheet {
                Some(sheet) => format!(
                    "runner lv {} · signal {}/{} · rations {}/{} · bits {} · locker {} · owed {} · {} · {}",
                    sheet.level,
                    sheet.signal,
                    sheet.max_signal(),
                    sheet.rations_left,
                    crate::app::deadchannel::fight::data::RATIONS_PER_DAY,
                    sheet.bits,
                    sheet.stash,
                    sheet.debt,
                    crate::app::deadchannel::fight::ui::weapon_name(sheet),
                    crate::app::deadchannel::fight::ui::armor_name(sheet),
                ),
                None => "no runner sheet".to_string(),
            };
            app.banner = Some(Banner::info(&format!(
                "Haunt stage1 {} · chosen {} (active {}h, settings {}, bio {}ch {}) · {glitch} · glitch hits {}/{GLITCH_TOTAL_CAP} · name hits {}/{NAME_TOTAL_CAP} · {witness} · {door} · {whisper} · {invite} · {sheet}",
                on_off(app.haunt.stage1),
                on_off(app.haunt.chosen),
                gate.active_hours,
                gate.touched_settings,
                gate.bio_chars,
                bio_standing_label(gate.bio),
                app.haunt.marks.glitch_hits,
                app.haunt.marks.name_hits
            )));
        }
        HauntCommand::Arm => {
            // A session the gate passed over armed nothing past stage 1;
            // this arms the repeatable machines so a beat is testable
            // without reconnecting or a passing bio.
            app.banner = Some(Banner::success("Haunt armed for this session"));
            app.haunt.stage1 = true;
            app.haunt.chosen = true;
            if app.haunt.clock_glitch.is_none() {
                app.haunt.clock_glitch = Some(ClockGlitch::new(
                    session_seed(app.user_id),
                    app.marquee_tick,
                    app.haunt.marks.glitch_hits,
                ));
            }
            if app.haunt.name_flicker.is_none() {
                app.haunt.name_flicker = Some(NameFlicker::new(
                    session_seed(app.user_id),
                    app.haunt.marks.name_hits,
                ));
            }
            if app.haunt.breakthrough.is_none() {
                app.haunt.breakthrough = Some(Breakthrough::for_user(app.user_id));
            }
        }
        HauntCommand::Glitch => match app.haunt.clock_glitch.as_mut() {
            None => {
                app.banner = Some(Banner::error("Glitch is not armed - /haunt arm first"));
            }
            Some(glitch) => {
                glitch.fire_now(app.marquee_tick);
                app.banner = Some(Banner::success("Glitch fires in ~7s - watch the clock"));
            }
        },
        HauntCommand::Name => match app.haunt.name_flicker.as_mut() {
            None => {
                app.banner = Some(Banner::error(
                    "Name flicker is not armed - /haunt arm first",
                ));
            }
            Some(flicker) => {
                flicker.force_next();
                app.banner = Some(Banner::success(
                    "Next message you send flickers - watch your name",
                ));
            }
        },
        HauntCommand::Replay => {
            replay_whisper(app);
        }
        HauntCommand::Invite => match (app.haunt.marks.invited_at, app.haunt.breakthrough.as_mut())
        {
            (Some(_), _) => {
                app.banner = Some(Banner::error(
                    "Already invited - /haunt reset to clear the marks",
                ));
            }
            (None, None) => {
                app.banner = Some(Banner::error(
                    "Breakthrough is not armed - /haunt arm first",
                ));
            }
            (None, Some(breakthrough)) => {
                breakthrough.force_next();
                app.banner = Some(Banner::success(
                    "Next message you send breaks through - the DM follows",
                ));
            }
        },
        HauntCommand::Reset => {
            app.haunt.marks = FirstContactMarks {
                glitch_hits: 0,
                name_hits: 0,
                whisper_hits: 0,
                whisper_at: None,
                invited_at: None,
            };
            app.haunt.pending_claims.clear();
            if let Some(glitch) = app.haunt.clock_glitch.as_mut() {
                *glitch = ClockGlitch::new(session_seed(app.user_id), app.marquee_tick, 0);
            }
            if let Some(flicker) = app.haunt.name_flicker.as_mut() {
                *flicker = NameFlicker::new(session_seed(app.user_id), 0);
            }
            if let Some(breakthrough) = app.haunt.breakthrough.as_mut() {
                *breakthrough = Breakthrough::for_user(app.user_id);
            }
            app.haunt.pending_invitation = None;
            app.profile_state.service().reset_first_contact(app.user_id);
            app.banner = Some(Banner::success(
                "First-contact marks cleared - the chain starts over next session",
            ));
        }
        HauntCommand::Welcome => {
            app.chat
                .service
                .post_wire_line_task(runner::data::welcome(&app.username));
            app.banner = Some(Banner::success(
                "Welcome posted on the wire - read it in #deadchannel",
            ));
        }
    }
    true
}
