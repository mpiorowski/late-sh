use super::*;

#[test]
fn new_users_only_receive_shared_tip_candidates() {
    let candidates = tip_candidates(true);

    assert_eq!(candidates.len(), NEW_AND_RETURNING_TIPS.len());
    for tip in &candidates {
        assert!(
            NEW_AND_RETURNING_TIPS
                .iter()
                .any(|candidate| candidate == tip)
        );
    }
    for tip in &candidates {
        assert!(!RETURNING_USER_TIPS.iter().any(|candidate| candidate == tip));
    }
}

#[test]
fn returning_users_receive_combined_tip_candidates() {
    let candidates = tip_candidates(false);

    assert_eq!(
        candidates.len(),
        NEW_AND_RETURNING_TIPS.len() + RETURNING_USER_TIPS.len()
    );
    for tip in NEW_AND_RETURNING_TIPS
        .iter()
        .chain(RETURNING_USER_TIPS.iter())
    {
        assert!(
            candidates
                .iter()
                .any(|candidate| candidate == &tip.as_str())
        );
    }
}

#[test]
fn splash_hint_selection_uses_pool_values_when_available() {
    let tip = choose_splash_hint(true);

    assert!(
        tip_candidates(true)
            .iter()
            .any(|candidate| candidate == &tip)
    );
}

/// Every Arcade daily is named in a tip, so a new one cannot ship without
/// the splash ever mentioning it.
#[test]
fn every_daily_puzzle_has_a_tip() {
    let candidates = tip_candidates(false);
    for puzzle in late_core::models::leaderboard::DailyPuzzle::ALL {
        assert!(
            candidates.iter().any(|tip| tip.contains(puzzle.title())),
            "no splash tip names {}",
            puzzle.title()
        );
    }
}

/// The series starts on the second login, the first being the tour's,
/// leads with the radio and ends after the Nightcap.
#[test]
fn the_series_runs_from_the_second_login_to_the_nightcap() {
    let series: Vec<Option<DidYouKnow>> = (0..=8).map(DidYouKnow::for_login).collect();
    assert_eq!(
        series,
        vec![
            None,
            None,
            Some(DidYouKnow::Radio),
            Some(DidYouKnow::Rung),
            Some(DidYouKnow::People),
            Some(DidYouKnow::Booth),
            Some(DidYouKnow::Artboard),
            Some(DidYouKnow::Nightcap),
            None,
        ]
    );
}

#[test]
fn the_radio_line_counts_the_stations_and_names_the_listen_page() {
    let line = DidYouKnow::Radio.line("https://late.sh/listen");
    let stations = late_core::radio::RadioStation::enabled().count();
    assert!(stations > 0);
    assert_eq!(
        line,
        format!(
            "Did you know? {stations} radio stations and the YouTube booth play in the late CLI, or in a browser at https://late.sh/listen"
        )
    );
}
