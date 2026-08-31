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
