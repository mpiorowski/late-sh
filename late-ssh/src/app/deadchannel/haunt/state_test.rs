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
fn natural_delivery_without_input() {
    let mut state = WhisperState::with_seed(0, 0);
    let line_len = state.line().chars().count();

    // Held until the answer beat, then the line types itself.
    hold_through(&mut state, 1, ANSWER_TICK - 1);
    assert_eq!(
        state,
        WhisperState {
            line: WHISPER_LINES[0],
            phase: WhisperPhase::Held,
            last_input_tick: None,
            first_input_tick: None,
            seed: 0,
        }
    );
    assert_eq!(state.typed_chars(ANSWER_TICK - 1), (0, false));

    hold_through(&mut state, ANSWER_TICK, ANSWER_TICK);
    assert_eq!(
        state.phase,
        WhisperPhase::Typing {
            from_tick: ANSWER_TICK
        }
    );
    assert_eq!(state.typed_chars(ANSWER_TICK + 5), (5, true));

    // Fully typed, lingers, then releases delivered with no input ever
    // recorded and no corruption windows open.
    let typed_done = ANSWER_TICK + line_len;
    hold_through(&mut state, ANSWER_TICK + 1, typed_done + LINGER_TICKS - 1);
    assert_eq!(
        state.tick(typed_done + LINGER_TICKS, true),
        WhisperTick::Released { delivered: true }
    );
    assert_eq!(
        state,
        WhisperState {
            line: WHISPER_LINES[0],
            phase: WhisperPhase::Released { delivered: true },
            last_input_tick: None,
            first_input_tick: None,
            seed: 0,
        }
    );
    assert!(typed_done + LINGER_TICKS < HARD_CAP_TICKS);
    assert_eq!(state.surge_progress(typed_done), None);
    assert_eq!(state.dissolve_progress(typed_done), None);
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
fn input_answers_early_and_never_skips() {
    let mut state = WhisperState::with_seed(3, 0);
    let line_len = state.line().chars().count();

    // Esc at tick 20: the line starts in answer, the static surges, the
    // hint starts dissolving. The door stays held.
    hold_through(&mut state, 1, 19);
    state.note_input(20);
    assert_eq!(state.phase, WhisperPhase::Typing { from_tick: 20 });
    assert_eq!(state.surge_progress(20), Some(0.0));
    assert_eq!(state.surge_progress(20 + SURGE_TICKS), None);
    assert_eq!(state.dissolve_progress(20 + DISSOLVE_TICKS), Some(1.0));

    // A second keypress mid-typing re-surges but does not restart the line.
    hold_through(&mut state, 20, 30);
    state.note_input(31);
    assert_eq!(state.phase, WhisperPhase::Typing { from_tick: 20 });
    assert_eq!(state.surge_progress(32), Some(1.0 / SURGE_TICKS as f32));

    let typed_done = 20 + line_len;
    hold_through(&mut state, 31, typed_done + LINGER_TICKS - 1);
    assert_eq!(
        state.tick(typed_done + LINGER_TICKS, true),
        WhisperTick::Released { delivered: true }
    );
    assert_eq!(
        state,
        WhisperState {
            line: WHISPER_LINES[3],
            phase: WhisperPhase::Released { delivered: true },
            last_input_tick: Some(31),
            first_input_tick: Some(20),
            seed: 3,
        }
    );

    // Input after release is inert.
    state.note_input(typed_done + LINGER_TICKS + 1);
    assert_eq!(state.last_input_tick, Some(31));
}

#[test]
fn kill_switch_drops_the_scene_unspent() {
    let mut state = WhisperState::with_seed(1, 0);
    hold_through(&mut state, 1, ANSWER_TICK + 4);
    assert_eq!(
        state.tick(ANSWER_TICK + 5, false),
        WhisperTick::Released { delivered: false }
    );
    // Released stays released, whatever comes later.
    assert_eq!(
        state.tick(ANSWER_TICK + 6, true),
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
fn glitch_asks_when_due_then_starts_on_a_won_claim_and_heals() {
    let mut glitch = ClockGlitch::new(42, 0, 0);
    let due = glitch.next_at;
    assert!((GLITCH_GAP_MIN_TICKS..GLITCH_GAP_MAX_TICKS).contains(&due));

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
        .expect("a 1-in-24 roll should land within 2000 sends")
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
    assert_eq!(flicker.corruption(100).map(|(id, _)| id), Some(message));
    assert!(!flicker.tick(100 + NAME_HOLD_TICKS - 1));
    assert!(flicker.tick(100 + NAME_HOLD_TICKS));
    assert_eq!(flicker.corruption(100 + NAME_HOLD_TICKS), None);
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

    // The free legs come before the paid one: short of the hours, or of
    // the touched settings, the bio is not screened at all.
    let gate = FirstContactGate::evaluate(now, tenured - 1, &settings(&bio, json!(null)), true);
    assert_eq!(gate.bio, BioStanding::Unscreened);
    assert!(!gate.needs_bio_screen());
    let one_setting_unscreened = json!({ "bio": bio, "theme_id": "night" });
    let gate = FirstContactGate::evaluate(now, tenured, &one_setting_unscreened, true);
    assert_eq!(gate.touched_settings, 1);
    assert!(!gate.needs_bio_screen());

    // AI off and nothing on record: fail closed, and no screen to claim.
    let gate = FirstContactGate::evaluate(now, tenured, &settings(&bio, json!(null)), false);
    assert_eq!(gate.bio, BioStanding::AiOff);
    assert!(!gate.needs_bio_screen());

    // Short bios never spend a screen.
    let gate = FirstContactGate::evaluate(now, tenured, &settings("hi", json!(null)), true);
    assert_eq!(gate.bio, BioStanding::TooShort);
    assert!(!gate.needs_bio_screen());

    // A pass on the current text opens the leg; a pass is final, even
    // years old and even with AI off now.
    let passed = settings(
        &bio,
        screen(&hash, "passed", now - chrono::Duration::days(400)),
    );
    let gate = FirstContactGate::evaluate(now, tenured, &passed, false);
    assert_eq!(gate.bio, BioStanding::Passed);
    assert!(gate.passes());
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
    assert_eq!(parse_haunt_command("/haunt live"), Some(None));
    assert_eq!(parse_haunt_command("/haunt on off"), Some(None));
    assert_eq!(parse_haunt_command("/haunted"), None);
}
