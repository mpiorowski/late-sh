use super::*;
use rand::{RngCore, SeedableRng, rngs::StdRng};

#[test]
fn random_keeps_one_piece_until_the_view_ends() {
    let now = Instant::now();
    let mut rng = StdRng::seed_from_u64(42);
    let mut slot = None;
    assert!(Playback::sync(
        &mut slot,
        Some(AsciiSelection::Random),
        now,
        &mut rng
    ));
    let initial = slot;
    let mut untouched_rng = rng.clone();
    for minutes in [0, 1, 5, 60, 600] {
        assert!(!Playback::sync(
            &mut slot,
            Some(AsciiSelection::Random),
            now + Duration::from_secs(minutes * 60),
            &mut rng,
        ));
        assert_eq!(slot, initial);
    }
    assert_eq!(
        rng.next_u64(),
        untouched_rng.next_u64(),
        "no redraw rerolls"
    );
    assert!(Playback::sync(&mut slot, None, now, &mut rng));
    assert!(slot.is_none());
    let mut untouched_rng = rng.clone();
    assert!(Playback::sync(
        &mut slot,
        Some(AsciiSelection::Random),
        now,
        &mut rng
    ));
    assert_ne!(
        rng.next_u64(),
        untouched_rng.next_u64(),
        "a new instance draws again"
    );
}

#[test]
fn randomer_changes_landscapes_on_its_own_five_minute_boundaries() {
    let start = Instant::now();
    let mut rng = StdRng::seed_from_u64(7);
    let mut slot = None;
    Playback::sync(&mut slot, Some(AsciiSelection::Randomer), start, &mut rng);
    for boundary in [300, 600, 900] {
        let previous = slot.unwrap().piece;
        let deadline = start + Duration::from_secs(boundary);
        assert!(!Playback::sync(
            &mut slot,
            Some(AsciiSelection::Randomer),
            deadline - Duration::from_nanos(1),
            &mut rng
        ));
        assert_eq!(slot.unwrap().piece, previous);
        assert!(Playback::sync(
            &mut slot,
            Some(AsciiSelection::Randomer),
            deadline,
            &mut rng
        ));
        assert_ne!(slot.unwrap().piece.scene, previous.scene);
        assert!(!Playback::sync(
            &mut slot,
            Some(AsciiSelection::Randomer),
            deadline,
            &mut rng
        ));
    }
}

#[test]
fn a_delayed_tick_changes_once_and_keeps_the_original_phase() {
    let start = Instant::now();
    let mut rng = StdRng::seed_from_u64(12);
    let mut slot = None;
    Playback::sync(&mut slot, Some(AsciiSelection::Randomer), start, &mut rng);
    let previous = slot.unwrap().piece;
    let mut once_rng = rng.clone();
    let expected = choose(Some(previous.scene), &mut once_rng);
    assert!(Playback::sync(
        &mut slot,
        Some(AsciiSelection::Randomer),
        start + Duration::from_millis(1_037_250),
        &mut rng
    ));
    assert_eq!(slot.unwrap().piece, expected);
    assert_eq!(rng.next_u64(), once_rng.next_u64(), "only one draw");
    assert_eq!(
        slot.unwrap().next_change,
        Some(start + Duration::from_secs(1200))
    );
}

#[test]
fn every_style_is_eligible_but_neither_style_of_the_previous_scene_is() {
    let mut rng = StdRng::seed_from_u64(23);
    for exclude in std::iter::once(None).chain(Scene::ALL.into_iter().map(Some)) {
        let mut seen = Vec::new();
        for _ in 0..256 {
            let piece = choose(exclude, &mut rng);
            assert_ne!(Some(piece.scene), exclude);
            if !seen.contains(&piece) {
                seen.push(piece);
            }
        }
        for piece in AsciiPiece::ALL {
            assert_eq!(seen.contains(&piece), Some(piece.scene) != exclude);
        }
    }
}

#[test]
fn changing_modes_starts_a_new_clock_and_fixed_pieces_stop_rotation() {
    let start = Instant::now();
    let mut rng = StdRng::seed_from_u64(5);
    let mut slot = None;
    Playback::sync(&mut slot, Some(AsciiSelection::Random), start, &mut rng);
    let switched = start + Duration::from_secs(70);
    assert!(Playback::sync(
        &mut slot,
        Some(AsciiSelection::Randomer),
        switched,
        &mut rng
    ));
    assert_eq!(slot.unwrap().next_change, Some(switched + ROTATION));
    assert!(Playback::sync(
        &mut slot,
        Some(AsciiSelection::DEFAULT),
        switched,
        &mut rng
    ));
    assert_eq!(slot.unwrap().piece, AsciiPiece::DEFAULT);
    assert_eq!(slot.unwrap().next_change, None);
    assert!(!Playback::sync(
        &mut slot,
        Some(AsciiSelection::DEFAULT),
        start + ROTATION * 9,
        &mut rng
    ));
}

#[test]
fn views_started_at_different_times_have_independent_rotation_deadlines() {
    let start = Instant::now();
    let mut rng = StdRng::seed_from_u64(8);
    let mut first = None;
    let mut second = None;
    let mode = Some(AsciiSelection::Randomer);
    Playback::sync(&mut first, mode, start, &mut rng);
    Playback::sync(&mut second, mode, start + Duration::from_secs(17), &mut rng);
    let second_initial = second;
    assert!(Playback::sync(&mut first, mode, start + ROTATION, &mut rng));
    assert!(!Playback::sync(
        &mut second,
        mode,
        start + ROTATION,
        &mut rng
    ));
    assert_eq!(second, second_initial);
    let first_rotated = first;
    assert!(Playback::sync(
        &mut second,
        mode,
        start + ROTATION + Duration::from_secs(17),
        &mut rng
    ));
    assert_eq!(first, first_rotated);
}

#[test]
fn resolve_reads_a_fixed_piece_directly_and_a_random_one_only_from_its_own_draw() {
    let now = Instant::now();
    let mut rng = StdRng::seed_from_u64(31);
    let dawn = AsciiSelection::Piece(AsciiPiece::ALL[AsciiPiece::ALL.len() - 1]);
    assert_eq!(
        Playback::resolve(None, dawn),
        Some(AsciiPiece::ALL[AsciiPiece::ALL.len() - 1]),
        "a fixed piece needs no playback"
    );
    assert_eq!(Playback::resolve(None, AsciiSelection::Random), None);
    let mut slot = None;
    Playback::sync(&mut slot, Some(AsciiSelection::Random), now, &mut rng);
    let drawn = slot.unwrap().piece();
    assert_eq!(Playback::resolve(slot, AsciiSelection::Random), Some(drawn));
    assert_eq!(
        Playback::resolve(slot, AsciiSelection::Randomer),
        None,
        "another mode's draw is never shown"
    );
}
