#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CardSuit {
    Hearts,
    Diamonds,
    Clubs,
    Spades,
}

impl CardSuit {
    pub fn short(self) -> &'static str {
        match self {
            Self::Hearts => "H",
            Self::Diamonds => "D",
            Self::Clubs => "C",
            Self::Spades => "S",
        }
    }

    pub fn symbol(self) -> &'static str {
        match self {
            Self::Hearts => "♥",
            Self::Diamonds => "♦",
            Self::Clubs => "♣",
            Self::Spades => "♠",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CardRank {
    Ace,
    Number(u8),
    Jack,
    Queen,
    King,
}

impl CardRank {
    pub fn label(self) -> &'static str {
        match self {
            Self::Ace => "A",
            Self::Number(2) => "2",
            Self::Number(3) => "3",
            Self::Number(4) => "4",
            Self::Number(5) => "5",
            Self::Number(6) => "6",
            Self::Number(7) => "7",
            Self::Number(8) => "8",
            Self::Number(9) => "9",
            Self::Number(10) => "10",
            Self::Jack => "J",
            Self::Queen => "Q",
            Self::King => "K",
            Self::Number(_) => "?",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlayingCard {
    pub suit: CardSuit,
    pub rank: CardRank,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AsciiCardTheme {
    Minimal,
    Boxed,
    Outline,
}

pub const ASCII_CARD_THEMES: [AsciiCardTheme; 3] = [
    AsciiCardTheme::Minimal,
    AsciiCardTheme::Boxed,
    AsciiCardTheme::Outline,
];

pub const OUTLINE_CARD_WIDTH: usize = 9;

impl AsciiCardTheme {
    pub fn id(self) -> &'static str {
        match self {
            Self::Minimal => "minimal",
            Self::Boxed => "boxed",
            Self::Outline => "outline",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Minimal => "Minimal",
            Self::Boxed => "Boxed",
            Self::Outline => "Outline",
        }
    }

    pub fn card_height(self) -> usize {
        match self {
            Self::Minimal | Self::Boxed => 1,
            Self::Outline => 5,
        }
    }

    pub fn render_face_compact(self, card: PlayingCard) -> String {
        let rank = card.rank.label();
        match self {
            Self::Minimal => format!("{rank:<2}{}", card.suit.short()),
            Self::Boxed => format!("|{rank:<2}{}|", card.suit.short()),
            Self::Outline => format!("[{rank:<2}{}]", card.suit.short()),
        }
    }

    pub fn render_back_compact(self) -> &'static str {
        match self {
            Self::Minimal => "## ",
            Self::Boxed => "|## |",
            Self::Outline => "[###]",
        }
    }

    pub fn render_empty_compact(self) -> &'static str {
        match self {
            Self::Minimal => ".. ",
            Self::Boxed => "|__ |",
            Self::Outline => "[   ]",
        }
    }

    pub fn render_stock_count_compact(self, remaining: usize) -> String {
        match self {
            Self::Minimal => {
                if remaining == 0 {
                    "RST".to_string()
                } else {
                    format!("{remaining:>2} ")
                }
            }
            Self::Boxed => {
                if remaining == 0 {
                    "|RST|".to_string()
                } else {
                    format!("|{remaining:>3}|")
                }
            }
            Self::Outline => {
                if remaining == 0 {
                    "[RST]".to_string()
                } else {
                    format!("[{remaining:>3}]")
                }
            }
        }
    }

    pub fn render_face_lines(self, card: PlayingCard) -> Vec<String> {
        match self {
            Self::Minimal | Self::Boxed => vec![self.render_face_compact(card)],
            Self::Outline => render_outline_face(card),
        }
    }

    pub fn render_back_lines(self) -> Vec<String> {
        match self {
            Self::Minimal | Self::Boxed => vec![self.render_back_compact().to_string()],
            Self::Outline => vec![
                "┌───────┐".to_string(),
                "│╱╲╱╲╱╲╱│".to_string(),
                "│╲╱╲╱╲╱╲│".to_string(),
                "│╱╲╱╲╱╲╱│".to_string(),
                "└───────┘".to_string(),
            ],
        }
    }

    pub fn render_empty_lines(self) -> Vec<String> {
        match self {
            Self::Minimal | Self::Boxed => vec![self.render_empty_compact().to_string()],
            Self::Outline => vec![
                "┌───────┐".to_string(),
                "│       │".to_string(),
                "│       │".to_string(),
                "│       │".to_string(),
                "└───────┘".to_string(),
            ],
        }
    }

    pub fn render_stock_count_lines(self, remaining: usize) -> Vec<String> {
        match self {
            Self::Minimal | Self::Boxed => vec![self.render_stock_count_compact(remaining)],
            Self::Outline => {
                let center = if remaining == 0 {
                    " RESET ".to_string()
                } else {
                    format!("{remaining:^7}")
                };
                vec![
                    "┌───────┐".to_string(),
                    "│ STOCK │".to_string(),
                    format!("│{center}│"),
                    "│       │".to_string(),
                    "└───────┘".to_string(),
                ]
            }
        }
    }
}

fn render_outline_face(card: PlayingCard) -> Vec<String> {
    let top_index = outline_index(card.rank.label(), card.suit.symbol(), false);
    let bottom_index = outline_index(card.rank.label(), card.suit.symbol(), true);
    let center = outline_center_art(card);

    vec![
        "┌───────┐".to_string(),
        format!("│{top_index:<7}│"),
        format!("│{center}│"),
        format!("│{bottom_index:>7}│"),
        "└───────┘".to_string(),
    ]
}

fn outline_center_art(card: PlayingCard) -> String {
    match card.rank {
        CardRank::Ace => centered_art("  A  "),
        CardRank::Number(value @ 2..=10) => centered_art(number_art(value)),
        CardRank::Jack => centered_art(format!("J/{}\\", suit_monogram(card.suit))),
        CardRank::Queen => centered_art(format!("Q<{}>", suit_monogram(card.suit))),
        CardRank::King => centered_art(format!("K#{}", suit_monogram(card.suit))),
        CardRank::Number(_) => "   ?   ".to_string(),
    }
}

fn outline_index(rank: &str, suit: &str, reversed: bool) -> String {
    if reversed {
        format!("{suit} {rank}")
    } else {
        format!("{rank}{suit}")
    }
}

fn centered_art(content: impl AsRef<str>) -> String {
    format!("{:^7}", content.as_ref())
}

fn number_art(value: u8) -> &'static str {
    match value {
        2 => " : : ",
        3 => " .:. ",
        4 => " :*: ",
        5 => " =+= ",
        6 => " <>>< ",
        7 => " /7\\ ",
        8 => " <8> ",
        9 => " (9) ",
        10 => "10/0",
        _ => " ?? ",
    }
}

fn suit_monogram(suit: CardSuit) -> &'static str {
    match suit {
        CardSuit::Hearts => "H",
        CardSuit::Diamonds => "D",
        CardSuit::Clubs => "C",
        CardSuit::Spades => "S",
    }
}

#[cfg(test)]
mod tests {
    use super::{AsciiCardTheme, CardRank, CardSuit, PlayingCard};

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
}
