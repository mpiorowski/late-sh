use crate::app::games::cards::{AsciiCardTheme, CardRank, CardSuit, PlayingCard};

#[test]
fn boxed_theme_keeps_fixed_width_for_single_and_double_digit_ranks() {
    let ace = AsciiCardTheme::Boxed.render_face_compact(PlayingCard {
        suit: CardSuit::Hearts,
        rank: CardRank::Ace,
    });
    let ten = AsciiCardTheme::Boxed.render_face_compact(PlayingCard {
        suit: CardSuit::Spades,
        rank: CardRank::Number(10),
    });

    assert_eq!(ace, "|A H|");
    assert_eq!(ten, "|10S|");
    assert_eq!(ace.len(), ten.len());
}

#[test]
fn boxed_theme_has_distinct_face_back_and_empty_tokens() {
    assert_eq!(AsciiCardTheme::Boxed.render_back_compact(), "|## |");
    assert_eq!(AsciiCardTheme::Boxed.render_empty_compact(), "|__ |");
    assert_eq!(AsciiCardTheme::Boxed.render_stock_count_compact(0), "|RST|");
}

#[test]
fn outline_theme_emits_three_line_cards() {
    let face = AsciiCardTheme::Outline.render_face_lines(PlayingCard {
        suit: CardSuit::Diamonds,
        rank: CardRank::Queen,
    });

    assert_eq!(
        face,
        vec![
            "┌───────┐",
            "│Q♦     │",
            "│ Q<D>  │",
            "│    ♦ Q│",
            "└───────┘",
        ]
    );
    assert_eq!(AsciiCardTheme::Outline.card_height(), 5);
    assert_eq!(
        AsciiCardTheme::Outline.render_stock_count_lines(24),
        vec![
            "┌───────┐",
            "│ STOCK │",
            "│  24   │",
            "│       │",
            "└───────┘",
        ]
    );
}

#[test]
fn outline_theme_lines_have_consistent_width() {
    let face = AsciiCardTheme::Outline.render_face_lines(PlayingCard {
        suit: CardSuit::Hearts,
        rank: CardRank::Number(10),
    });

    assert!(face.iter().all(|line| line.chars().count() == 9));
    assert!(
        AsciiCardTheme::Outline
            .render_back_lines()
            .iter()
            .all(|line| line.chars().count() == 9)
    );
    assert!(
        AsciiCardTheme::Outline
            .render_empty_lines()
            .iter()
            .all(|line| line.chars().count() == 9)
    );
}
