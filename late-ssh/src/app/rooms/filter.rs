use super::mock::PlaceholderKind;
use super::svc::GameKind;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum RoomsFilter {
    #[default]
    All,
    Blackjack,
    Poker,
    Chess,
    Battleship,
    Tron,
}

impl RoomsFilter {
    pub const ALL: [Self; 6] = [
        Self::All,
        Self::Blackjack,
        Self::Poker,
        Self::Chess,
        Self::Battleship,
        Self::Tron,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Blackjack => "Blackjack",
            Self::Poker => "Poker",
            Self::Chess => "Chess",
            Self::Battleship => "Battleship",
            Self::Tron => "Tron",
        }
    }

    pub fn matches_real(self, kind: GameKind) -> bool {
        matches!(
            (self, kind),
            (Self::All, _) | (Self::Blackjack, GameKind::Blackjack)
        )
    }

    pub fn matches_placeholder(self, kind: PlaceholderKind) -> bool {
        matches!(
            (self, kind),
            (Self::All, _)
                | (Self::Poker, PlaceholderKind::Poker)
                | (Self::Chess, PlaceholderKind::Chess)
                | (Self::Battleship, PlaceholderKind::Battleship)
                | (Self::Tron, PlaceholderKind::Tron)
        )
    }

    pub fn cycle(self, forward: bool) -> Self {
        let idx = Self::ALL.iter().position(|f| *f == self).unwrap_or(0);
        let len = Self::ALL.len();
        let next = if forward {
            (idx + 1) % len
        } else {
            (idx + len - 1) % len
        };
        Self::ALL[next]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cycle_wraps_in_both_directions() {
        assert_eq!(RoomsFilter::All.cycle(true), RoomsFilter::Blackjack);
        assert_eq!(RoomsFilter::Tron.cycle(true), RoomsFilter::All);
        assert_eq!(RoomsFilter::All.cycle(false), RoomsFilter::Tron);
    }

    #[test]
    fn all_matches_everything() {
        assert!(RoomsFilter::All.matches_real(GameKind::Blackjack));
        assert!(RoomsFilter::All.matches_placeholder(PlaceholderKind::Poker));
        assert!(RoomsFilter::All.matches_placeholder(PlaceholderKind::Chess));
    }

    #[test]
    fn blackjack_only_matches_blackjack() {
        assert!(RoomsFilter::Blackjack.matches_real(GameKind::Blackjack));
        assert!(!RoomsFilter::Blackjack.matches_placeholder(PlaceholderKind::Poker));
    }
}
