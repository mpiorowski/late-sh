use super::*;
use late_core::models::leaderboard::ArcadeGame;

/// Every game in the roster has a page, and none of them is empty: a lobby
/// row carries no description, so this card is the only place the rules
/// are written (`DESIGN_GUIDE.md` §4.5).
#[test]
fn every_arcade_game_has_a_rules_page() {
    for game in ArcadeGame::all() {
        let rules = rules_for(game);
        assert!(rules.title.ends_with(" Rules"), "{}", rules.title);
        let text: String = rules
            .body
            .iter()
            .flat_map(|line| line.spans.iter())
            .map(|span| span.content.as_ref())
            .collect();
        assert!(
            text.trim().len() > 40,
            "{} has no rules on its page",
            rules.title
        );
    }
}

/// The chips on a daily's page are the ones its tiers pay, so the card and
/// the lobby's marks cannot disagree.
#[test]
fn a_dailys_page_states_the_chips_its_tiers_pay() {
    use late_core::models::leaderboard::DailyPuzzle;

    for puzzle in DailyPuzzle::ALL.iter().copied() {
        let rules = rules_for(ArcadeGame::Puzzle(puzzle));
        let text: String = rules
            .body
            .iter()
            .flat_map(|line| line.spans.iter())
            .map(|span| span.content.as_ref())
            .collect();
        for (_, tier) in puzzle.tiers().list() {
            assert!(
                text.contains(&tier.chips().to_string()),
                "{} does not say it pays {} chips",
                puzzle.title(),
                tier.chips()
            );
        }
    }
}
