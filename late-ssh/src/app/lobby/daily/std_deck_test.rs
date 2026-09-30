use super::*;

#[test]
fn every_card_round_trips_through_its_id_and_nothing_else_parses() {
    for card in fresh_deck() {
        assert_eq!(Card::from_id(card.id()), Some(card));
        let json = serde_json::to_value(card).expect("a card serializes");
        let back: Card = serde_json::from_value(json).expect("and reads back");
        assert_eq!(back, card);
    }
    assert_eq!(Card::from_id(DECK as u8), None);
    assert!(serde_json::from_value::<Card>(serde_json::json!(52)).is_err());
}

#[test]
fn a_shuffle_is_the_whole_deck_once() {
    let deck = shuffled_deck(&mut rand::thread_rng());
    assert!(is_whole_deck(&deck));

    let mut repeated = deck.clone();
    repeated[1] = repeated[0];
    assert!(!is_whole_deck(&repeated), "a repeated card is not a deck");
    assert!(!is_whole_deck(&deck[1..]), "a short deal is not a deck");
}

#[test]
fn face_cards_count_ten_and_the_ace_one() {
    assert_eq!(Rank::Ace.value(), 1);
    assert_eq!(Rank::Nine.value(), 9);
    for rank in [Rank::Ten, Rank::Jack, Rank::Queen, Rank::King] {
        assert_eq!(rank.value(), 10);
    }
    assert_eq!(Card::new(Rank::Ten, CardSuit::Hearts).label(), "10♥");
}
