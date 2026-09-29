use super::*;
use crate::app::lobby::daily::std_deck::Rank;

#[test]
fn a_click_on_a_covered_edge_picks_that_card_and_past_it_the_last() {
    let slots = CardSlots {
        rect: Rect::new(10, 5, Tier::Full.fan_width(4), 5),
        step: Tier::Full.fan_step(),
        count: 4,
    };
    assert_eq!(slots.at(10, 5), Some(0));
    assert_eq!(slots.at(14, 7), Some(0));
    assert_eq!(slots.at(15, 7), Some(1));
    // The last card is drawn whole, so its full width is its own.
    assert_eq!(slots.at(10 + Tier::Full.fan_width(4) - 1, 9), Some(3));
    assert_eq!(slots.at(9, 5), None);
    assert_eq!(slots.at(12, 10), None);
}

#[test]
fn a_fan_shows_every_rank_and_ends_on_a_whole_card() {
    let cards = [
        Card::new(Rank::Ten, CardSuit::Hearts),
        Card::new(Rank::Jack, CardSuit::Hearts),
        Card::new(Rank::Queen, CardSuit::Hearts),
    ];
    let row: Vec<(Face, Style)> = cards
        .iter()
        .map(|card| (Face::Up(*card), Style::default()))
        .collect();
    let lines = fan(&row, Tier::Full);
    let index_row: String = lines[1]
        .spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect();
    assert_eq!(index_row.chars().count() as u16, Tier::Full.fan_width(3));
    for rank in ["10♥", "J♥", "Q♥"] {
        assert!(index_row.contains(rank), "{rank} hidden in {index_row:?}");
    }
}
