use super::*;

/// Drive the machine from `from` to `to` inclusive, asserting it holds the
/// door the whole way.
fn hold_through(state: &mut WhisperState, from: usize, to: usize) {
    for tick in from..=to {
        assert_eq!(
            state.tick(tick, true),
            WhisperTick::Holding,
            "expected the door held at tick {tick}"
        );
    }
}

#[test]
fn the_door_plays_on_its_own_clock_and_releases_delivered() {
    let mut state = WhisperState::with_seed(0, 0);
    let line_len = state.line().chars().count();

    // The static pulses from the first frame, before anyone presses
    // anything, and rests between pulses.
    assert_eq!(state.surge_progress(0), Some(0.0));
    assert_eq!(state.surge_progress(SURGE_TICKS), None);
    assert_eq!(state.surge_progress(SURGE_PERIOD_TICKS), Some(0.0));

    // Held until the base splash line has typed, the hint still intact.
    hold_through(&mut state, 1, VOICE_TICK - 1);
    assert_eq!(
        state,
        WhisperState {
            line: WHISPER_LINES[0],
            phase: WhisperPhase::Held,
            seed: 0,
        }
    );
    assert_eq!(state.typed_chars(VOICE_TICK - 1), (0, false));
    assert_eq!(state.dissolve_progress(VOICE_TICK - 1), None);

    // Then the voice starts on its own and the skip hint dissolves.
    hold_through(&mut state, VOICE_TICK, VOICE_TICK);
    assert_eq!(
        state.phase,
        WhisperPhase::Typing {
            from_tick: VOICE_TICK
        }
    );
    assert_eq!(state.typed_chars(VOICE_TICK + 5), (5, true));
    assert_eq!(
        state.dissolve_progress(VOICE_TICK + DISSOLVE_TICKS),
        Some(1.0)
    );

    // Fully typed, lingers, then releases delivered, and pulses no more.
    let typed_done = VOICE_TICK + line_len;
    hold_through(&mut state, VOICE_TICK + 1, typed_done + LINGER_TICKS - 1);
    assert_eq!(
        state.tick(typed_done + LINGER_TICKS, true),
        WhisperTick::Released { delivered: true }
    );
    assert_eq!(
        state,
        WhisperState {
            line: WHISPER_LINES[0],
            phase: WhisperPhase::Released { delivered: true },
            seed: 0,
        }
    );
    assert_eq!(state.surge_progress(SURGE_PERIOD_TICKS * 20), None);

    // Every line in both pools finishes well inside the hard cap, so the
    // cap never cuts a door short of its mark.
    for line in WHISPER_LINES.iter().chain(WHISPER_LINES_SECOND.iter()) {
        assert!(
            VOICE_TICK + line.chars().count() + LINGER_TICKS < HARD_CAP_TICKS,
            "{line:?} outruns the hard cap"
        );
    }
}

#[test]
fn the_second_door_speaks_from_its_own_pool_and_the_marks_space_the_two_apart() {
    use chrono::TimeZone;

    // The pool follows how many whispers have already played, never the
    // seed alone: one person hears two different lines.
    let first = WhisperState::with_seed(0, 0);
    let second = WhisperState::with_seed(0, 1);
    assert_eq!(first.line(), WHISPER_LINES[0]);
    assert_eq!(second.line(), WHISPER_LINES_SECOND[0]);
    assert!(!WHISPER_LINES.contains(&second.line()));

    // Due, a gap, the cap.
    let now = Utc.with_ymd_and_hms(2026, 9, 2, 22, 0, 0).unwrap();
    let marks = |whisper_hits: u32, whisper_at: Option<DateTime<Utc>>| FirstContactMarks {
        glitch_hits: GLITCH_TOTAL_CAP,
        name_hits: NAME_TOTAL_CAP,
        whisper_hits,
        whisper_at,
        invited_at: None,
    };
    let never = marks(0, None);
    assert!(never.whisper_due(now));
    assert!(!never.whispers_spent());
    let tonight = marks(
        1,
        Some(now - chrono::Duration::hours(WHISPER_GAP_HOURS - 1)),
    );
    assert!(!tonight.whisper_due(now));
    let later_day = marks(1, Some(now - chrono::Duration::hours(WHISPER_GAP_HOURS)));
    assert!(later_day.whisper_due(now));
    let spent = marks(WHISPER_TOTAL_CAP, Some(now - chrono::Duration::days(30)));
    assert!(!spent.whisper_due(now));
    assert!(spent.whispers_spent());
}

#[test]
fn kill_switch_drops_the_scene_unspent() {
    let mut state = WhisperState::with_seed(1, 0);
    hold_through(&mut state, 1, VOICE_TICK + 4);
    assert_eq!(
        state.tick(VOICE_TICK + 5, false),
        WhisperTick::Released { delivered: false }
    );
    // Released stays released, whatever comes later.
    assert_eq!(
        state.tick(VOICE_TICK + 6, true),
        WhisperTick::Released { delivered: false }
    );
}

#[test]
fn hard_cap_opens_the_door() {
    // A machine that somehow never advanced still releases at the cap,
    // undelivered because the line never finished.
    let mut state = WhisperState::with_seed(2, 0);
    assert_eq!(
        state.tick(HARD_CAP_TICKS, true),
        WhisperTick::Released { delivered: false }
    );
}

#[test]
fn breakthrough_comes_due_a_delay_after_the_second_door_and_never_after_the_invite() {
    use chrono::TimeZone;

    let now = Utc.with_ymd_and_hms(2026, 9, 13, 20, 0, 0).unwrap();
    let marks = |whisper_hits: u32,
                 whisper_at: Option<DateTime<Utc>>,
                 invited_at: Option<DateTime<Utc>>| FirstContactMarks {
        glitch_hits: GLITCH_TOTAL_CAP,
        name_hits: NAME_TOTAL_CAP,
        whisper_hits,
        whisper_at,
        invited_at,
    };
    let a_day_ago = Some(now - chrono::Duration::hours(INVITE_DELAY_HOURS));
    assert!(marks(WHISPER_TOTAL_CAP, a_day_ago, None).breakthrough_due(now));
    // A door still owed, the same evening as the last door, or already
    // invited: not due.
    assert!(!marks(WHISPER_TOTAL_CAP - 1, a_day_ago, None).breakthrough_due(now));
    let tonight = Some(now - chrono::Duration::hours(INVITE_DELAY_HOURS - 1));
    assert!(!marks(WHISPER_TOTAL_CAP, tonight, None).breakthrough_due(now));
    assert!(!marks(WHISPER_TOTAL_CAP, a_day_ago, Some(now)).breakthrough_due(now));
}

#[test]
fn breakthrough_claims_on_a_due_send_then_plays_its_scene_and_heals() {
    let mut breakthrough = Breakthrough::with_seed(9);
    let len = BREAKTHROUGH_LINE.chars().count();

    // Not due, or the kill switch off: sends pass untouched.
    assert_eq!(
        breakthrough.note_own_send(true, false),
        BreakthroughRoll::Wait
    );
    assert_eq!(
        breakthrough.note_own_send(false, true),
        BreakthroughRoll::Wait
    );
    // Due: one claim, and no second ask while it is out.
    assert_eq!(
        breakthrough.note_own_send(true, true),
        BreakthroughRoll::Claim
    );
    assert_eq!(
        breakthrough.note_own_send(true, true),
        BreakthroughRoll::Wait
    );
    assert_eq!(breakthrough.tick(100, true), BreakthroughTick::Idle);
    assert_eq!(breakthrough.typed_chars(100), None);

    // Won at tick 100: static alone first, then the line types itself.
    breakthrough.start(100);
    assert_eq!(breakthrough.tick(100, true), BreakthroughTick::Playing);
    assert_eq!(breakthrough.surge_progress(100), Some(0.0));
    assert_eq!(
        breakthrough.typed_chars(100 + BREAKTHROUGH_VOICE_TICK - 1),
        Some((0, false))
    );
    assert_eq!(
        breakthrough.typed_chars(100 + BREAKTHROUGH_VOICE_TICK + 5),
        Some((5, true))
    );
    // A send mid-scene asks for nothing.
    assert_eq!(
        breakthrough.note_own_send(true, true),
        BreakthroughRoll::Wait
    );

    // The whole line holds, then the screen heals.
    let end = 100 + BREAKTHROUGH_VOICE_TICK + len + BREAKTHROUGH_LINGER_TICKS;
    assert_eq!(breakthrough.typed_chars(end - 1), Some((len, false)));
    assert_eq!(breakthrough.tick(end - 1, true), BreakthroughTick::Playing);
    assert_eq!(breakthrough.tick(end, true), BreakthroughTick::Ended);
    assert_eq!(
        breakthrough,
        Breakthrough {
            phase: BreakthroughPhase::Idle,
            seed: 9,
            force_next: false,
        }
    );
    // The line names the voice the DM comes from.
    assert!(BREAKTHROUGH_LINE.contains(VOICE_USERNAME));
}

#[test]
fn breakthrough_force_a_failed_ask_and_the_kill_switch() {
    let mut breakthrough = Breakthrough::with_seed(1);

    // `/haunt invite`: the next send claims without the delay, once.
    breakthrough.force_next();
    assert_eq!(
        breakthrough.note_own_send(true, false),
        BreakthroughRoll::Claim
    );
    // The ask failed: nothing plays, and no send asks again this session,
    // due or not, so a broken voice is not re-queried on every send.
    breakthrough.claim_failed();
    assert_eq!(
        breakthrough.note_own_send(true, true),
        BreakthroughRoll::Wait
    );
    // `/haunt invite` re-opens it.
    breakthrough.force_next();
    assert_eq!(
        breakthrough.note_own_send(true, false),
        BreakthroughRoll::Claim
    );
    // A claim taken by another device settles back to waiting.
    breakthrough.claim_taken();
    assert_eq!(breakthrough.phase(), BreakthroughPhase::Idle);
    // The kill switch cuts a live scene.
    breakthrough.start(10);
    assert_eq!(breakthrough.tick(11, false), BreakthroughTick::Ended);
    assert_eq!(
        breakthrough,
        Breakthrough {
            phase: BreakthroughPhase::Idle,
            seed: 1,
            force_next: false,
        }
    );
}

#[test]
fn glitch_asks_when_due_then_starts_on_a_won_claim_and_heals() {
    let mut glitch = ClockGlitch::new(42, 0, 0);
    // The first burst of a session comes early, inside a short evening.
    let due = glitch.next_at;
    assert!((GLITCH_FIRST_MIN_TICKS..GLITCH_FIRST_MAX_TICKS).contains(&due));

    assert_eq!(glitch.tick(due - 1, true, true), GlitchTick::Idle);
    assert_eq!(glitch.tick(due, true, true), GlitchTick::Due);
    // The schedule holds while the row decides: no second ask, no frame.
    assert_eq!(glitch.tick(due + 1, true, true), GlitchTick::Idle);
    assert_eq!(glitch.corruption(due + 1), None);

    glitch.start(due + 2, 1);
    let seed = glitch.corruption(due + 2);
    assert!(seed.is_some());
    assert_eq!(glitch.tick(due + 3, true, true), GlitchTick::Idle);
    assert_eq!(glitch.corruption(due + 2 + GLITCH_HOLD_TICKS - 1), seed);
    assert_eq!(
        glitch.tick(due + 2 + GLITCH_HOLD_TICKS, true, true),
        GlitchTick::Ended
    );
    assert_eq!(glitch.corruption(due + 2 + GLITCH_HOLD_TICKS), None);
    assert_eq!(glitch.total_hits(), 1);
    // Rescheduled a full gap out.
    let gap = glitch.next_at - (due + 2 + GLITCH_HOLD_TICKS);
    assert!((GLITCH_GAP_MIN_TICKS..GLITCH_GAP_MAX_TICKS).contains(&gap));
}

#[test]
fn glitch_defers_hidden_reschedules_disabled_and_obeys_the_row() {
    let mut glitch = ClockGlitch::new(7, 0, 0);

    // Due while the clock is off screen: a short defer, never spent unseen.
    let due = glitch.next_at;
    assert_eq!(glitch.tick(due, true, false), GlitchTick::Idle);
    let defer = glitch.next_at - due;
    assert!((GLITCH_DEFER_MIN_TICKS..GLITCH_DEFER_MAX_TICKS).contains(&defer));

    // Due while the kill switch is off: a full re-dice.
    let due = glitch.next_at;
    assert_eq!(glitch.tick(due, false, true), GlitchTick::Idle);
    assert!((glitch.next_at - due) >= GLITCH_GAP_MIN_TICKS);

    // The row says capped for today: nothing shows, a full re-dice, and
    // the mirror takes the row's count.
    let due = glitch.next_at;
    assert_eq!(glitch.tick(due, true, true), GlitchTick::Due);
    glitch.claim_capped(due + 1, 2);
    assert_eq!(glitch.corruption(due + 1), None);
    assert_eq!(glitch.total_hits(), 2);
    assert!((glitch.next_at - (due + 1)) >= GLITCH_GAP_MIN_TICKS);

    // The row could not be asked: a short defer, then ask again.
    let due = glitch.next_at;
    assert_eq!(glitch.tick(due, true, true), GlitchTick::Due);
    glitch.claim_failed(due + 1);
    let defer = glitch.next_at - (due + 1);
    assert!((GLITCH_DEFER_MIN_TICKS..GLITCH_DEFER_MAX_TICKS).contains(&defer));
    let due = glitch.next_at;
    assert_eq!(glitch.tick(due, true, true), GlitchTick::Due);

    // A capped answer at the lifetime share quiets the clock for good.
    glitch.claim_capped(due + 1, GLITCH_TOTAL_CAP);
    let due = glitch.next_at;
    assert_eq!(
        glitch.tick(due + GLITCH_GAP_MAX_TICKS, true, true),
        GlitchTick::Idle
    );
}

#[test]
fn forced_glitch_waits_out_the_banner_then_bypasses_the_caps_and_the_quiet() {
    let mut glitch = ClockGlitch::new(9, 0, GLITCH_TOTAL_CAP);
    // The natural schedule never asks again, however due it comes.
    let due = glitch.next_at;
    for tick in [due, due + 1, due + GLITCH_GAP_MAX_TICKS] {
        assert_eq!(glitch.tick(tick, true, true), GlitchTick::Idle);
    }

    glitch.fire_now(due);
    let forced = due + GLITCH_FORCE_DELAY_TICKS;
    // The fuse burns past the banner; nothing shows early, even hidden.
    assert_eq!(glitch.tick(forced - 1, true, false), GlitchTick::Idle);
    assert_eq!(glitch.corruption(forced - 1), None);
    // Then it bursts at once, no claim, and heals like any burst.
    assert_eq!(glitch.tick(forced, true, true), GlitchTick::Started);
    assert!(glitch.corruption(forced).is_some());
    assert_eq!(
        glitch.tick(forced + GLITCH_HOLD_TICKS, true, true),
        GlitchTick::Ended
    );
    // A forced burst still counts toward the ladder.
    assert_eq!(glitch.total_hits(), GLITCH_TOTAL_CAP + 1);
}

#[test]
fn name_flicker_waits_for_the_clock_stage() {
    // While stage 1 has bursts left the dice never roll, however many
    // sends land; the admin force hook ignores the gate.
    let mut flicker = NameFlicker::new(5, 0);
    assert!(
        (0..2_000).all(
            |tick| flicker.note_own_message(Uuid::now_v7(), tick, true, false) == NameRoll::Miss
        ),
        "a closed stage must never roll"
    );
    flicker.force_next();
    assert_eq!(
        flicker.note_own_message(Uuid::now_v7(), 100, true, false),
        NameRoll::Forced
    );
}

fn roll_until_claim(flicker: &mut NameFlicker) -> usize {
    (0..2_000)
        .find(|tick| flicker.note_own_message(Uuid::now_v7(), *tick, true, true) == NameRoll::Claim)
        .expect("a 1-in-3 roll should land within 2000 sends")
}

#[test]
fn name_flicker_rolls_claims_and_forces() {
    let message = Uuid::now_v7();
    // At the lifetime cap nothing rolls, but the force hook shows at once
    // and heals on schedule.
    let mut flicker = NameFlicker::new(9, NAME_TOTAL_CAP);
    assert_eq!(
        flicker.note_own_message(message, 100, true, true),
        NameRoll::Miss
    );
    flicker.force_next();
    assert_eq!(
        flicker.note_own_message(message, 100, true, true),
        NameRoll::Forced
    );
    let (hit_id, first_seed) = flicker.corruption(100).expect("the hit shows");
    assert_eq!(hit_id, message);
    // One hit is two waves: the same message, a different corruption
    // seed from the wave edge on, and a repaint edge at that boundary
    // and at the heal, nowhere else.
    assert!(!flicker.tick(100 + NAME_WAVE_TICKS - 1));
    assert_eq!(
        flicker.corruption(100 + NAME_WAVE_TICKS - 1),
        Some((message, first_seed))
    );
    assert!(flicker.tick(100 + NAME_WAVE_TICKS));
    let (_, second_seed) = flicker
        .corruption(100 + NAME_WAVE_TICKS)
        .expect("the second wave shows");
    assert_ne!(second_seed, first_seed);
    assert!(!flicker.tick(100 + NAME_WAVE_TICKS + 1));
    assert!(!flicker.tick(100 + NAME_HOLD_TICKS - 1));
    assert_eq!(
        flicker.corruption(100 + NAME_HOLD_TICKS - 1),
        Some((message, second_seed))
    );
    assert!(flicker.tick(100 + NAME_HOLD_TICKS));
    assert_eq!(flicker.corruption(100 + NAME_HOLD_TICKS), None);
    assert!(!flicker.tick(100 + NAME_HOLD_TICKS + 1));
    assert_eq!(flicker.total_hits(), NAME_TOTAL_CAP + 1);

    // The kill switch swallows even a forced hit.
    let mut flicker = NameFlicker::new(9, 0);
    flicker.force_next();
    assert_eq!(
        flicker.note_own_message(message, 100, false, true),
        NameRoll::Miss
    );

    // A natural roll asks the row and shows nothing until it answers;
    // while the claim is out no other send rolls.
    let mut flicker = NameFlicker::new(5, 0);
    let hit_at = roll_until_claim(&mut flicker);
    assert_eq!(flicker.corruption(hit_at), None);
    assert!(
        (0..2_000).all(
            |tick| flicker.note_own_message(Uuid::now_v7(), tick, true, true) == NameRoll::Miss
        ),
        "no roll while a claim is out"
    );
    flicker.start(message, hit_at + 2, 1);
    assert_eq!(
        flicker.corruption(hit_at + 2).map(|(id, _)| id),
        Some(message)
    );
    assert_eq!(flicker.total_hits(), 1);
    assert!(flicker.tick(hit_at + 2 + NAME_HOLD_TICKS));

    // A capped answer shows nothing and takes the row's count, which here
    // is the lifetime share: the dice stop.
    let claim_at = roll_until_claim(&mut flicker);
    flicker.claim_capped(NAME_TOTAL_CAP);
    assert_eq!(flicker.corruption(claim_at), None);
    assert_eq!(flicker.total_hits(), NAME_TOTAL_CAP);
    assert!(
        (0..2_000).all(
            |tick| flicker.note_own_message(Uuid::now_v7(), tick, true, true) == NameRoll::Miss
        ),
        "the lifetime cap must hold"
    );

    // A failed answer only frees the dice.
    let mut flicker = NameFlicker::new(5, 0);
    roll_until_claim(&mut flicker);
    flicker.claim_failed();
    assert_eq!(flicker.total_hits(), 0);
    roll_until_claim(&mut flicker);
}

#[test]
fn the_gate_needs_all_three_legs_and_screens_only_when_useful() {
    use chrono::TimeZone;
    use serde_json::json;

    let now = Utc.with_ymd_and_hms(2026, 9, 2, 12, 0, 0).unwrap();
    let tenured = ACTIVE_MIN_HOURS * 60 * 60 * 1000;
    // Trimmed, since `extract_bio` trims before counting.
    let bio = "a real person wrote this "
        .repeat(BIO_MIN_CHARS / 20)
        .trim()
        .to_string();
    let hash = bio_hash(&bio);
    let settings = |bio: &str, screen: serde_json::Value| {
        json!({
            "bio": bio,
            "theme_id": "night",
            "country": "PL",
            "first_contact_bio": screen,
        })
    };
    let screen = |hash: &str, verdict: &str, at: DateTime<Utc>| json!({ "hash": hash, "verdict": verdict, "at": at.to_rfc3339() });

    // Never screened: this session claims a screen; the gate is shut.
    let gate = FirstContactGate::evaluate(now, tenured, &settings(&bio, json!(null)), true);
    assert_eq!(
        gate,
        FirstContactGate {
            active_hours: ACTIVE_MIN_HOURS,
            touched_settings: 2,
            bio_chars: bio.chars().count(),
            bio: BioStanding::Unscreened,
        }
    );
    assert!(gate.needs_bio_screen());
    assert!(!gate.passes());
    assert_eq!(gate.verdict(), GateVerdict::BioUnscreened);

    // The free legs come before the paid one: short of the hours, or of
    // the touched settings, the bio is not screened at all, and the
    // verdict names the free leg, not the bio.
    let gate = FirstContactGate::evaluate(now, tenured - 1, &settings(&bio, json!(null)), true);
    assert_eq!(gate.bio, BioStanding::Unscreened);
    assert!(!gate.needs_bio_screen());
    assert_eq!(gate.verdict(), GateVerdict::TooFewHours);
    let one_setting_unscreened = json!({ "bio": bio, "theme_id": "night" });
    let gate = FirstContactGate::evaluate(now, tenured, &one_setting_unscreened, true);
    assert_eq!(gate.touched_settings, 1);
    assert!(!gate.needs_bio_screen());
    assert_eq!(gate.verdict(), GateVerdict::TooFewSettings);

    // AI off and nothing on record: fail closed, and no screen to claim.
    let gate = FirstContactGate::evaluate(now, tenured, &settings(&bio, json!(null)), false);
    assert_eq!(gate.bio, BioStanding::AiOff);
    assert!(!gate.needs_bio_screen());
    assert_eq!(gate.verdict(), GateVerdict::BioAiOff);

    // Short bios never spend a screen.
    let gate = FirstContactGate::evaluate(now, tenured, &settings("hi", json!(null)), true);
    assert_eq!(gate.bio, BioStanding::TooShort);
    assert!(!gate.needs_bio_screen());
    assert_eq!(gate.verdict(), GateVerdict::BioTooShort);

    // A screen in flight, and a failed one, each report as themselves.
    let pending = settings(&bio, screen(&hash, "pending", now));
    let gate = FirstContactGate::evaluate(now, tenured, &pending, true);
    assert_eq!(gate.verdict(), GateVerdict::BioPending);
    let failed = settings(&bio, screen(&hash, "failed", now));
    let gate = FirstContactGate::evaluate(now, tenured, &failed, true);
    assert_eq!(gate.verdict(), GateVerdict::BioFailed);

    // A pass on the current text opens the leg; a pass is final, even
    // years old and even with AI off now.
    let passed = settings(
        &bio,
        screen(&hash, "passed", now - chrono::Duration::days(400)),
    );
    let gate = FirstContactGate::evaluate(now, tenured, &passed, false);
    assert_eq!(gate.bio, BioStanding::Passed);
    assert!(gate.passes());
    assert_eq!(gate.verdict(), GateVerdict::Passed);
    // ...but only with the other two legs.
    assert!(
        // One millisecond short of the last hour is short.
        !FirstContactGate::evaluate(now, tenured - 1, &passed, true).passes()
    );
    let one_setting = json!({ "bio": bio, "theme_id": "night", "first_contact_bio": screen(&hash, "passed", now) });
    assert!(!FirstContactGate::evaluate(now, tenured, &one_setting, true).passes());

    // A verdict for other text is no verdict: the rewritten bio is screened.
    let gate = FirstContactGate::evaluate(
        now,
        tenured,
        &settings(&bio, screen("stale", "passed", now)),
        true,
    );
    assert_eq!(gate.bio, BioStanding::Unscreened);

    // A fresh failure or a fresh pending claim holds; a stale one is
    // claimed again.
    let fresh = now - chrono::Duration::hours(1);
    let stale = now - chrono::Duration::hours(BIO_RESCREEN_AFTER_HOURS + 1);
    let gate = FirstContactGate::evaluate(
        now,
        tenured,
        &settings(&bio, screen(&hash, "failed", fresh)),
        true,
    );
    assert_eq!(gate.bio, BioStanding::Failed);
    assert!(!gate.needs_bio_screen());
    let gate = FirstContactGate::evaluate(
        now,
        tenured,
        &settings(&bio, screen(&hash, "failed", stale)),
        true,
    );
    assert_eq!(gate.bio, BioStanding::Unscreened);
    let gate = FirstContactGate::evaluate(
        now,
        tenured,
        &settings(&bio, screen(&hash, "pending", fresh)),
        true,
    );
    assert_eq!(gate.bio, BioStanding::Pending);
    let gate = FirstContactGate::evaluate(
        now,
        tenured,
        &settings(&bio, screen(&hash, "pending", stale)),
        true,
    );
    assert_eq!(gate.bio, BioStanding::Unscreened);

    // The gate bootstrap hands back when the fuse is unlit: no leg holds,
    // and nothing is worth the paid screen.
    let closed = FirstContactGate::closed();
    assert_eq!(
        closed,
        FirstContactGate {
            active_hours: 0,
            touched_settings: 0,
            bio_chars: 0,
            bio: BioStanding::TooShort,
        }
    );
    assert!(!closed.passes());
    assert!(!closed.needs_bio_screen());
}

#[test]
fn haunt_commands_parse_the_fuse_words() {
    assert_eq!(
        parse_haunt_command("/haunt"),
        Some(Some(HauntCommand::Status))
    );
    assert_eq!(
        parse_haunt_command("/haunt live on"),
        Some(Some(HauntCommand::LiveOn))
    );
    assert_eq!(
        parse_haunt_command("/haunt  live   off "),
        Some(Some(HauntCommand::LiveOff))
    );
    assert_eq!(
        parse_haunt_command("/haunt welcome"),
        Some(Some(HauntCommand::Welcome))
    );
    assert_eq!(parse_haunt_command("/haunt live"), Some(None));
    assert_eq!(parse_haunt_command("/haunt on off"), Some(None));
    assert_eq!(parse_haunt_command("/haunted"), None);
}

#[test]
fn a_witness_paints_the_same_corruption_the_haunted_does() {
    // The seed rides the wire, so the room's screens swap the characters
    // the sender's screen swaps, in the same two waves, even though every
    // one of them started painting at a different tick.
    let message = Uuid::now_v7();
    let mut flicker = NameFlicker::new(9, 0);
    flicker.force_next();
    assert_eq!(
        flicker.note_own_message(message, 100, true, true),
        NameRoll::Forced
    );
    let (_, seed) = flicker.live_hit().expect("the hit is showing");

    // Nine ticks later than the sender: another replica, another eye.
    let mut witness = Some(ActiveHit::new(message, 109, seed));
    for wave in 0..NAME_WAVES {
        let offset = wave * NAME_WAVE_TICKS;
        assert_eq!(
            witness
                .as_ref()
                .and_then(|hit| hit.corruption(109 + offset)),
            flicker
                .corruption(100 + offset)
                .map(|(_, seed)| (message, seed)),
            "wave {wave} must corrupt identically on both screens"
        );
    }
    assert!(ActiveHit::tick(&mut witness, 109 + NAME_HOLD_TICKS));
    assert_eq!(witness, None);
}
