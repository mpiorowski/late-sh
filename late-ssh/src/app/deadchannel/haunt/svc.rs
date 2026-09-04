//! Orchestration for the haunting: the gate and arming at session start,
//! the splash door, the glitch scheduler, the own-name flicker, the hit
//! claims, the invitation, the bio screen, and the `/haunt` admin
//! controls. This is the only haunting layer that touches `App`, logging,
//! metrics, and persistence; the machines in `state.rs` stay pure, and the
//! root files keep one routing line each.

use late_core::db::Db;
use late_core::models::app_flag::{AppFlag, AppFlags};
use late_core::models::user::{FirstContactBioVerdict, FirstContactHitClaim, User};
use tokio::sync::{oneshot, watch};
use tracing::{Instrument, info_span};

use super::state::{
    BIO_RESCREEN_AFTER_HOURS, BioStanding, ClockGlitch, FirstContactGate, FirstContactMarks,
    GLITCH_TOTAL_CAP, GlitchTick, HauntCommand, HauntState, HitStage, INVITE_DELAY_DAYS,
    NAME_TOTAL_CAP, NameFlicker, NameRoll, PendingClaim, PendingFlagWrite, WHISPER_GAP_HOURS,
    WHISPER_TOTAL_CAP, WhisperState, WhisperTick, bio_hash, glitch_caps, name_caps,
};
use crate::app::ai::screen::{BioScreen, screen_bio};
use crate::app::ai::svc::AiService;
use crate::app::common::primitives::Banner;
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
/// The flags come first, and they are the same pair `arm` reads: with
/// `haunt_enabled` off, or the `haunt_live` fuse unlit for a non-staff user,
/// nobody can be haunted this session, so the connect path spends
/// nothing on them: no online-time round trip, no paid bio screen. With
/// the flags unread (`None`) the gate is shut, like every other stage.
pub(crate) async fn bootstrap_gate(state: &State, is_staff: bool, user: &User) -> FirstContactGate {
    let snapshot: Option<AppFlags> = *state.app_flags.subscribe().borrow();
    let armable =
        snapshot.is_some_and(|flags| flags.haunt_enabled && (is_staff || flags.haunt_live));
    if !armable {
        // Staff are haunted while the fuse is unlit, so a shut gate for
        // one of them means haunting is off (or the flags are unread) and
        // is the line to look for when the ladder seems dead. For everyone
        // else an unlit fuse is the normal state of the world.
        if is_staff {
            tracing::info!(user_id = %user.id, username = %user.username, is_staff, "first contact gate shut: haunting off or flags unread");
        } else {
            tracing::debug!(user_id = %user.id, username = %user.username, is_staff, "first contact gate shut: fuse unlit");
        }
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
/// moderators, `Permissions::can_moderate`) always and
/// for everyone once the `haunt_live` fuse is lit; stages 2-4 arm behind
/// the gate, or for anyone whose funnel already has a stage-2 hit
/// (eligibility gates entering, never continuing). First contact is a
/// nonrenewable resource: with the flags unread (`None`) nothing arms.
/// The chain order is the spec: three clock bursts open stage 2, the
/// third name hit arms the stage-3 whisper (it fires on the next fresh
/// connect, then once more on a later day), and the last delivered
/// whisper schedules the stage-4 invitation.
/// Dice are per session on purpose: the same person on two evenings, or
/// two people side by side, roll differently.
pub(crate) fn arm(
    is_staff: bool,
    flags: watch::Receiver<Option<AppFlags>>,
    user_id: uuid::Uuid,
    username: &str,
    marks: FirstContactMarks,
    gate: FirstContactGate,
) -> HauntState {
    let snapshot: Option<AppFlags> = *flags.borrow();
    let enabled = snapshot.is_some_and(|flags| flags.haunt_enabled);
    let live = snapshot.is_some_and(|flags| flags.haunt_live);
    let stage1 = enabled && (is_staff || live);
    let chosen = stage1 && (gate.passes() || marks.name_hits > 0);
    let whisper_armed =
        chosen && marks.name_hits >= NAME_TOTAL_CAP && marks.whisper_due(chrono::Utc::now());
    // What this session can fire, for whom. Stage 1 off is the quiet
    // default for everyone while the fuse is unlit, so only an armed
    // session is worth a line.
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
        marks,
        gate,
        stage1,
        chosen,
        flags,
        pending_claims: Vec::new(),
        pending_flag_writes: Vec::new(),
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
    changed |= tick_flag_writes(app);
    if app.show_splash {
        changed |= tick_splash_door(app);
    }
    changed |= tick_clock_glitch(app);
    changed |= tick_name_flicker(app);
    tick_invitation(app);
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
            (HitStage::Name { message_id }, Ok(FirstContactHitClaim::Won { hits })) => {
                if let Some(flicker) = app.haunt.name_flicker.as_mut() {
                    flicker.start(message_id, tick, hits);
                }
                app.haunt.marks.name_hits = hits;
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

/// Drain answered `/haunt on|off|live` writes into the banner. The row is
/// the truth: "off" is only said once the row says off, and a failed
/// write is said out loud, since the admin who flipped the kill switch
/// is the one person who must not be told a comforting lie.
fn tick_flag_writes(app: &mut App) -> bool {
    let mut answered = Vec::new();
    app.haunt
        .pending_flag_writes
        .retain_mut(|pending| match pending.rx.try_recv() {
            Ok(outcome) => {
                answered.push((pending.flag, pending.enabled, pending.done, outcome));
                false
            }
            Err(oneshot::error::TryRecvError::Empty) => true,
            Err(oneshot::error::TryRecvError::Closed) => {
                answered.push((
                    pending.flag,
                    pending.enabled,
                    pending.done,
                    Err(anyhow::anyhow!("flag write task dropped its sender")),
                ));
                false
            }
        });
    let mut changed = false;
    for (flag, enabled, done, outcome) in answered {
        match outcome {
            Ok(()) => {
                tracing::info!(user_id = %app.user_id, username = %app.username, key = flag.key(), enabled, "haunt flag set");
                app.banner = Some(Banner::success(done));
            }
            Err(error) => {
                tracing::error!(user_id = %app.user_id, username = %app.username, key = flag.key(), enabled, error = ?error, "failed to set haunt flag");
                app.banner = Some(Banner::error(&format!(
                    "Flag {} not written: {error}",
                    flag.key()
                )));
            }
        }
        changed = true;
    }
    changed
}

/// Drive the armed whisper for one splash tick. Release (natural, hard
/// cap, or kill switch) closes the splash here and claims one capped
/// delivery only on a delivered line; the last stamp is also what starts
/// the invitation clock.
fn tick_splash_door(app: &mut App) -> bool {
    let enabled = app.haunt.enabled();
    let Some(whisper) = app.haunt.whisper.as_mut() else {
        return false;
    };
    match whisper.tick(app.splash_ticks, enabled) {
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
    let enabled = app.haunt.enabled();
    let clock_visible = !app.show_splash && app.right_sidebar_visible();
    let glitch = app.haunt.clock_glitch.as_mut().expect("checked above");
    match glitch.tick(app.marquee_tick, enabled, clock_visible) {
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
    // for a later `/haunt on`.
    let landed = app.chat.take_own_message_landed();
    let enabled = app.haunt.enabled();
    let stage_open = app.haunt.marks.glitch_hits >= GLITCH_TOTAL_CAP;
    let Some(flicker) = app.haunt.name_flicker.as_mut() else {
        return false;
    };
    let mut changed = false;
    changed |= flicker.tick(app.marquee_tick);
    let Some(message_id) = landed else {
        return changed;
    };
    match flicker.note_own_message(message_id, app.marquee_tick, enabled, stage_open) {
        NameRoll::Miss => {}
        NameRoll::Claim => {
            let rx = app.profile_state.service().claim_first_contact_name_hit(
                app.user_id,
                chrono::Utc::now().date_naive(),
                name_caps(),
            );
            app.haunt.pending_claims.push(PendingClaim {
                stage: HitStage::Name { message_id },
                rx,
            });
        }
        NameRoll::Forced => {
            app.haunt.marks.name_hits = flicker.total_hits();
            app.profile_state
                .service()
                .record_first_contact_name_hit(app.user_id);
            metrics::record_first_contact_beat(FirstContactBeat::NameFlicker);
            tracing::info!(user_id = %app.user_id, username = %app.username, hits = app.haunt.marks.name_hits, "first contact name flicker hit (forced)");
            changed = true;
        }
    }
    changed
}

/// The stage-4 clock: some days after the last delivered whisper, the
/// game's first voice sends its one persistent DM. Self-serve on purpose (the
/// chosen one's own session notices), so there is no cross-user sweep;
/// the conditional settings claim in the send task keeps two devices
/// from double-sending.
fn tick_invitation(app: &mut App) {
    if !app.haunt.chosen
        || app.haunt.marks.invited_at.is_some()
        || !app.haunt.marks.whispers_spent()
        || !app.haunt.enabled()
    {
        return;
    }
    let Some(whisper_at) = app.haunt.marks.whisper_at else {
        return;
    };
    let now = chrono::Utc::now();
    if now - whisper_at < chrono::Duration::days(INVITE_DELAY_DAYS) {
        return;
    }
    send_invitation(app, now);
}

fn send_invitation(app: &mut App, now: chrono::DateTime<chrono::Utc>) {
    // Local stamp stops this session re-spawning the task every tick; the
    // DB claim inside the task is the cross-session guard.
    app.haunt.marks.invited_at = Some(now);
    app.chat
        .service
        .send_first_contact_invitation_task(app.user_id, app.username.clone());
    metrics::record_first_contact_beat(FirstContactBeat::InvitationRequested);
    tracing::info!(user_id = %app.user_id, username = %app.username, "first contact invitation requested");
}

/// Route splash input into the held door. Returns true when consumed:
/// input is acknowledged (the machine surges static and dissolves the
/// skip hint) but the splash does not skip. Silently ignoring input would
/// read as a hung terminal, the exact panic the hard rules forbid.
pub(crate) fn note_splash_input(app: &mut App) -> bool {
    let Some(whisper) = app.haunt.whisper.as_mut() else {
        return false;
    };
    whisper.note_input(app.splash_ticks);
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

/// Ask for a process-wide switch to flip, or explain why this session
/// cannot. The banner comes on the tick the row answers
/// (`tick_flag_writes`), not now.
fn set_flag(app: &mut App, flag: AppFlag, enabled: bool, done: &'static str) {
    match &app.app_flags {
        Some(service) => {
            let rx = service.set_task(flag, enabled);
            app.haunt.pending_flag_writes.push(PendingFlagWrite {
                flag,
                enabled,
                done,
                rx,
            });
        }
        None => {
            app.banner = Some(Banner::error("No flag service on this session"));
        }
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
            let invite = match app.haunt.marks.invited_at {
                Some(_) => "invited",
                None => "invite pending",
            };
            let gate = app.haunt.gate;
            app.banner = Some(Banner::info(&format!(
                "Haunt {} · live {} · stage1 {} · chosen {} (active {}h, settings {}, bio {}ch {}) · {glitch} · glitch hits {}/{GLITCH_TOTAL_CAP} · name hits {}/{NAME_TOTAL_CAP} · {door} · {whisper} · {invite}",
                on_off(app.haunt.enabled()),
                on_off(app.haunt.live()),
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
        HauntCommand::On => {
            set_flag(app, AppFlag::HauntEnabled, true, "Haunt on (every replica)");
            // A session that connected while the switch was off, or that
            // the gate passed over, armed nothing; turning the haunt on
            // arms the repeatable machines so the flip is testable without
            // reconnecting or a passing bio.
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
        }
        HauntCommand::Off => {
            // A live whisper drops on its own next splash tick and the
            // schedulers stop firing: every machine reads this switch, on
            // every replica once the notify lands.
            set_flag(
                app,
                AppFlag::HauntEnabled,
                false,
                "Haunt off (every replica)",
            );
        }
        HauntCommand::LiveOn => {
            set_flag(
                app,
                AppFlag::HauntLive,
                true,
                "Fuse lit: stage 1 for everyone from their next connect",
            );
        }
        HauntCommand::LiveOff => {
            set_flag(app, AppFlag::HauntLive, false, "Fuse out: staff only");
        }
        HauntCommand::Glitch => match app.haunt.clock_glitch.as_mut() {
            None => {
                app.banner = Some(Banner::error("Glitch is not armed - /haunt on first"));
            }
            Some(glitch) => {
                glitch.fire_now(app.marquee_tick);
                app.banner = Some(Banner::success("Glitch fires in ~7s - watch the clock"));
            }
        },
        HauntCommand::Name => match app.haunt.name_flicker.as_mut() {
            None => {
                app.banner = Some(Banner::error("Name flicker is not armed - /haunt on first"));
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
        HauntCommand::Invite => match app.haunt.marks.invited_at {
            Some(_) => {
                app.banner = Some(Banner::error(
                    "Already invited - /haunt reset to clear the marks",
                ));
            }
            None => {
                send_invitation(app, chrono::Utc::now());
                app.banner = Some(Banner::success("Invitation sent - check your DMs"));
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
            app.profile_state.service().reset_first_contact(app.user_id);
            app.banner = Some(Banner::success(
                "First-contact marks cleared - the chain starts over next session",
            ));
        }
    }
    true
}
