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
    let mut state = WhisperState::with_seed(0);
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
fn input_answers_early_and_never_skips() {
    let mut state = WhisperState::with_seed(3);
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
    let mut state = WhisperState::with_seed(1);
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
    let mut state = WhisperState::with_seed(2);
    assert_eq!(
        state.tick(HARD_CAP_TICKS, true),
        WhisperTick::Released { delivered: false }
    );
}

const DAY: fn() -> NaiveDate = || NaiveDate::from_ymd_opt(2026, 8, 31).unwrap();

#[test]
fn glitch_fires_when_due_then_heals_and_reschedules() {
    let mut glitch = ClockGlitch::new(42, 0);
    let due = glitch.next_at;
    assert!((GLITCH_GAP_MIN_TICKS..GLITCH_GAP_MAX_TICKS).contains(&due));

    assert_eq!(glitch.tick(due - 1, DAY(), true, true), GlitchTick::Idle);
    assert_eq!(glitch.tick(due, DAY(), true, true), GlitchTick::Started);
    // The burst seed is stable for the whole hold, then gone.
    let seed = glitch.corruption(due);
    assert!(seed.is_some());
    assert_eq!(glitch.tick(due + 1, DAY(), true, true), GlitchTick::Idle);
    assert_eq!(glitch.corruption(due + GLITCH_HOLD_TICKS - 1), seed);
    assert_eq!(
        glitch.tick(due + GLITCH_HOLD_TICKS, DAY(), true, true),
        GlitchTick::Ended
    );
    assert_eq!(glitch.corruption(due + GLITCH_HOLD_TICKS), None);
    assert_eq!(glitch.fired_today, 1);
    // Rescheduled a full gap out.
    let gap = glitch.next_at - (due + GLITCH_HOLD_TICKS);
    assert!((GLITCH_GAP_MIN_TICKS..GLITCH_GAP_MAX_TICKS).contains(&gap));
}

#[test]
fn glitch_defers_hidden_reschedules_disabled_and_caps_daily() {
    let mut glitch = ClockGlitch::new(7, 0);

    // Due while the clock is off screen: a short defer, never spent unseen.
    let due = glitch.next_at;
    assert_eq!(glitch.tick(due, DAY(), true, false), GlitchTick::Idle);
    let defer = glitch.next_at - due;
    assert!((GLITCH_DEFER_MIN_TICKS..GLITCH_DEFER_MAX_TICKS).contains(&defer));

    // Due while the kill switch is off: a full re-dice.
    let due = glitch.next_at;
    assert_eq!(glitch.tick(due, DAY(), false, true), GlitchTick::Idle);
    assert!((glitch.next_at - due) >= GLITCH_GAP_MIN_TICKS);

    // At the daily cap the burst reschedules instead of firing; the next
    // UTC day resets the counter and fires again.
    glitch.fired_today = GLITCH_DAILY_CAP;
    glitch.today = Some(DAY());
    let due = glitch.next_at;
    assert_eq!(glitch.tick(due, DAY(), true, true), GlitchTick::Idle);
    let due = glitch.next_at;
    assert_eq!(
        glitch.tick(due, DAY().succ_opt().unwrap(), true, true),
        GlitchTick::Started
    );
    assert_eq!(glitch.fired_today, 1);
}

#[test]
fn forced_glitch_waits_out_the_banner_then_bypasses_the_caps() {
    let mut glitch = ClockGlitch::new(9, 0);
    glitch.fired_today = GLITCH_DAILY_CAP;
    glitch.today = Some(DAY());

    glitch.fire_now(100);
    let due = 100 + GLITCH_FORCE_DELAY_TICKS;
    // The fuse burns past the banner; nothing shows early, even hidden.
    assert_eq!(glitch.tick(due - 1, DAY(), true, false), GlitchTick::Idle);
    assert_eq!(glitch.corruption(due - 1), None);
    // Then it bursts despite the daily cap and heals like any burst.
    assert_eq!(glitch.tick(due, DAY(), true, true), GlitchTick::Started);
    assert!(glitch.corruption(due).is_some());
    assert_eq!(
        glitch.tick(due + GLITCH_HOLD_TICKS, DAY(), true, true),
        GlitchTick::Ended
    );
}

#[test]
fn name_flicker_rolls_caps_and_forces() {
    let message = Uuid::now_v7();
    // At the lifetime cap nothing fires naturally, but the admin force
    // hook still does, and every hit counts and heals on schedule.
    let mut flicker = NameFlicker::new(9, NAME_TOTAL_CAP);
    assert!(!flicker.note_own_message(message, 100, DAY(), true));
    flicker.force_next();
    assert!(flicker.note_own_message(message, 100, DAY(), true));
    assert_eq!(flicker.corruption(100).map(|(id, _)| id), Some(message));
    assert!(!flicker.tick(100 + NAME_HOLD_TICKS - 1));
    assert!(flicker.tick(100 + NAME_HOLD_TICKS));
    assert_eq!(flicker.corruption(100 + NAME_HOLD_TICKS), None);
    assert_eq!(flicker.total_hits(), NAME_TOTAL_CAP + 1);

    // The kill switch swallows even a forced hit.
    let mut flicker = NameFlicker::new(9, 0);
    flicker.force_next();
    assert!(!flicker.note_own_message(message, 100, DAY(), false));

    // Under the daily cap a natural hit needs the dice; drive sends until
    // one lands, then the day is spent until the date changes.
    let mut flicker = NameFlicker::new(5, 0);
    let hit_at = (0..2_000)
        .find(|tick| flicker.note_own_message(Uuid::now_v7(), *tick, DAY(), true))
        .expect("a 1-in-24 roll should land within 2000 sends");
    assert!(flicker.tick(hit_at + NAME_HOLD_TICKS));
    assert_eq!(flicker.fired_today, 1);
    assert!(
        (0..2_000).all(|tick| !flicker.note_own_message(Uuid::now_v7(), tick, DAY(), true)),
        "the daily cap must hold for the rest of the day"
    );
    assert!(
        (0..2_000).any(|tick| flicker.note_own_message(
            Uuid::now_v7(),
            tick,
            DAY().succ_opt().unwrap(),
            true
        )),
        "the next day rolls again"
    );
}
